//! Pure/bounded helpers for the request-only PlayPlay diagnostic.
//! No native token bytes, credentials, or response bytes belong in saved reports.

use std::{
    io::{Read, Write},
    time::Duration,
};

use bytes::Bytes;
use http::{HeaderMap, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const DLL_SIZE: usize = 42_852_224;
pub const DLL_SHA256: &str = "0731eca3ec438395815907c04653c63a917b55bf0ebdd83c96f041424a92b54b";
pub const TOKEN_OFFSET: usize = 0x1967fe8;
pub const TOKEN_SHA256: &str = "856af68df2235a74f5cf1e45ef3de1172111ee5fc7c265826e12912c049f534d";
pub const BODY_LIMIT: usize = 64 * 1024;

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Extract only from the SAME bytes that are hashed. Never execute/load the DLL.
/// The caller supplies the fixed researched profile; CLI cannot override its hash.
pub fn checked_token(
    mut input: impl Read,
    size: usize,
    offset: usize,
    expected_hash: &str,
) -> Result<[u8; 16], &'static str> {
    let end = offset.checked_add(16).ok_or("invalid_profile_bounds")?;
    if end > size {
        return Err("invalid_profile_bounds");
    }
    let mut hash = Sha256::new();
    let mut token = [0u8; 16];
    let mut position = 0usize;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buffer).map_err(|_| "dll_read_failed")?;
        if n == 0 {
            break;
        }
        let next = position.checked_add(n).ok_or("dll_size_mismatch")?;
        if next > size {
            return Err("dll_size_mismatch");
        }
        hash.update(&buffer[..n]);
        let from = position.max(offset);
        let to = next.min(end);
        if from < to {
            token[from - offset..to - offset]
                .copy_from_slice(&buffer[from - position..to - position]);
        }
        position = next;
    }
    if position != size {
        return Err("dll_size_mismatch");
    }
    if format!("{:x}", hash.finalize()) != expected_hash {
        return Err("unsupported_dll_hash");
    }
    Ok(token)
}

pub struct Fields<'a> {
    pub version: i32,
    pub token: &'a [u8],
    pub cache_id: Option<&'a [u8]>,
    pub interactivity: i32,
    pub content_type: i32,
    pub timestamp: i64,
    pub field7: Option<i32>,
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn scalar(field: u8, value: i64, out: &mut Vec<u8>) {
    out.push(field << 3);
    varint(value as u64, out);
}

fn blob(field: u8, value: &[u8], out: &mut Vec<u8>) {
    out.push((field << 3) | 2);
    varint(value.len() as u64, out);
    out.extend_from_slice(value);
}

/// Native candidate omits empty/zero fields 1–6. `false` models the historical
/// explicitly-present proto2 fields for offline fixtures, NOT a live v2 profile.
/// Field names are historical comparison labels, not recovered native names.
pub fn encode(fields: &Fields<'_>, omit_defaults: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if !omit_defaults || fields.version != 0 {
        scalar(1, i64::from(fields.version), &mut out);
    }
    if !omit_defaults || !fields.token.is_empty() {
        blob(2, fields.token, &mut out);
    }
    if let Some(cache) = fields.cache_id {
        if !omit_defaults || !cache.is_empty() {
            blob(3, cache, &mut out);
        }
    }
    if !omit_defaults || fields.interactivity != 0 {
        scalar(4, i64::from(fields.interactivity), &mut out);
    }
    if !omit_defaults || fields.content_type != 0 {
        scalar(5, i64::from(fields.content_type), &mut out);
    }
    if !omit_defaults || fields.timestamp != 0 {
        scalar(6, fields.timestamp, &mut out);
    }
    if let Some(value) = fields.field7 {
        scalar(7, i64::from(value), &mut out);
    }
    out
}

#[derive(Default)]
pub struct Capture {
    pub bytes: Vec<u8>,
    pub observed_bytes: usize,
    pub complete: bool,
    pub failure: Option<&'static str>,
}

impl Capture {
    fn append(&mut self, chunk: &[u8], limit: usize) -> bool {
        self.observed_bytes = self.observed_bytes.saturating_add(chunk.len());
        let remaining = limit.saturating_sub(self.bytes.len());
        self.bytes
            .extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if chunk.len() > remaining {
            self.failure = Some("body_limit_exceeded");
            return false;
        }
        true
    }
}

/// Caller applies a deadline; a timed-out future leaves the bounded prefix and
/// counts in `capture`. Neither an error nor a truncation triggers another POST.
pub async fn capture_body<B>(body: &mut B, capture: &mut Capture, limit: usize)
where
    B: BodyExt<Data = Bytes> + Unpin,
{
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                if let Ok(data) = frame.into_data() {
                    if !capture.append(&data, limit) {
                        return;
                    }
                }
            }
            Err(_) => {
                capture.failure = Some("body_stream_error");
                return;
            }
        }
    }
    capture.complete = true;
}

pub async fn capture_with_deadline<B>(mut body: B, limit: usize, timeout: Duration) -> Capture
where
    B: BodyExt<Data = Bytes> + Unpin,
{
    let mut capture = Capture::default();
    if tokio::time::timeout(timeout, capture_body(&mut body, &mut capture, limit))
        .await
        .is_err()
    {
        capture.failure = Some("body_deadline_exceeded");
    }
    capture
}

fn read_varint(input: &[u8], position: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *input.get(*position)?;
        *position += 1;
        if shift == 63 && byte > 1 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Shape only: lengths of fields 1 and 2, not a license or an audio key.
/// Absent fields remain `None`. This conservative inspector is NOT a clone of
/// the native parser: duplicate/wrong-wire known fields, groups and over-budget
/// messages are unclassified, not necessarily invalid protobuf. Nonminimal
/// varints are accepted; no native canonical-encoding rule is inferred.
fn protobuf_shape(input: &[u8]) -> Option<[Option<usize>; 2]> {
    let mut position = 0;
    let mut sizes = [None; 2];
    let mut fields = 0;
    while position < input.len() {
        fields += 1;
        if fields > 1024 {
            return None;
        }
        let tag = read_varint(input, &mut position)?;
        let field = tag >> 3;
        if field == 0 || field > 0x1fff_ffff {
            return None;
        }
        let wire = tag & 7;
        let size = match wire {
            0 => {
                read_varint(input, &mut position)?;
                0
            }
            1 => 8,
            2 => usize::try_from(read_varint(input, &mut position)?).ok()?,
            5 => 4,
            _ => return None,
        };
        if (1..=2).contains(&field) {
            let index = (field - 1) as usize;
            if sizes[index].is_some() || wire != 2 {
                return None;
            }
            sizes[index] = Some(size);
        }
        position = position.checked_add(size)?;
        if position > input.len() {
            return None;
        }
    }
    Some(sizes)
}

fn known_error(value: &Value) -> Option<&'static str> {
    const ALLOWED: &[&str] = &[
        "PERMISSION_DENIED",
        "UNAUTHENTICATED",
        "INVALID_ARGUMENT",
        "INVALID_TOKEN",
        "EXPIRED_TOKEN",
        "INVALID_VERSION",
        "UNSUPPORTED_VERSION",
        "PREMIUM_REQUIRED",
        "INSUFFICIENT_SCOPE",
        "FORBIDDEN",
        "NOT_FOUND",
        "RATE_LIMITED",
        "RESOURCE_EXHAUSTED",
        "UNAVAILABLE",
        "INTERNAL",
        "INVALID_PLAYPLAY_TOKEN",
        "EXPIRED_PLAYPLAY_TOKEN",
        "UNSUPPORTED_PLAYPLAY_VERSION",
    ];
    for pointer in ["/error", "/error/code", "/error/status", "/code", "/status"] {
        if let Some(code) = value.pointer(pointer).and_then(Value::as_str) {
            if let Some(allowed) = ALLOWED.iter().find(|a| code.eq_ignore_ascii_case(a)) {
                return Some(*allowed);
            }
        }
    }
    None
}

fn mime(headers: &HeaderMap) -> &'static str {
    let raw = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    for allowed in [
        "application/x-protobuf",
        "application/protobuf",
        "application/octet-stream",
        "application/json",
        "text/plain",
        "text/html",
    ] {
        if raw.eq_ignore_ascii_case(allowed) {
            return allowed;
        }
    }
    if raw.is_empty() { "missing" } else { "other" }
}

fn numeric_header(headers: &HeaderMap, name: header::HeaderName) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

/// Strict allowlist: no body previews, arbitrary server messages/header values,
/// URLs, cookies, keys, tokens, or username can escape through this function.
pub fn response_summary(status: u16, headers: &HeaderMap, capture: &Capture) -> Value {
    let mut shape = "unavailable";
    let mut field_sizes = [None; 2];
    let mut error = None;
    if capture.complete {
        if capture.bytes.is_empty() {
            shape = "empty";
        } else if let Ok(value) = serde_json::from_slice::<Value>(&capture.bytes) {
            shape = "json";
            error = known_error(&value);
        } else if let Some(sizes) = protobuf_shape(&capture.bytes) {
            shape = "protobuf_syntax_only";
            field_sizes = sizes;
        } else {
            shape = "unrecognized";
        }
    }
    json!({
        "status": status,
        "outcome": if !capture.complete { "incomplete_response" }
                   else if (200..300).contains(&status) { "http_2xx_unvalidated" }
                   else { "http_non_2xx" },
        "content_type": mime(headers),
        "declared_content_length": numeric_header(headers, header::CONTENT_LENGTH),
        "retry_after_seconds": numeric_header(headers, header::RETRY_AFTER),
        "observed_body_bytes": capture.observed_bytes,
        "retained_body_bytes": capture.bytes.len(),
        "body_complete": capture.complete,
        "body_failure": capture.failure,
        "body_shape": shape,
        "recognized_error_code": error,
        "field_1_length_if_protobuf": field_sizes[0],
        "field_2_length_if_protobuf": field_sizes[1],
        "license_validated": false,
        "decryption_tested": false,
        "playback_tested": false,
        "tier_restriction_established": false
    })
}

/// Persist before console output, which may be closed or backpressured. A file
/// failure still permits a best-effort console copy; neither writer can panic us.
pub fn persist_report(
    report: &Value,
    output: &mut impl Write,
    console: &mut impl Write,
) -> Result<bool, &'static str> {
    let mut bytes = serde_json::to_vec_pretty(report).map_err(|_| "report_serialization_failed")?;
    bytes.push(b'\n');
    let saved = output
        .write_all(&bytes)
        .and_then(|_| output.flush())
        .is_ok();
    let displayed = console
        .write_all(&bytes)
        .and_then(|_| console.flush())
        .is_ok();
    if saved {
        Ok(displayed)
    } else {
        Err("report_write_failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use http_body_util::{Full, StreamBody};
    use std::io::Cursor;

    fn fields(token: &[u8]) -> Fields<'_> {
        Fields {
            version: 5,
            token,
            cache_id: Some(b""),
            interactivity: 1,
            content_type: 1,
            timestamp: 1_700_000_000,
            field7: None,
        }
    }

    #[test]
    fn native_audio_fixture_and_historical_fixture_are_distinct() {
        let token = [0xaa; 16]; // Synthetic, never a real token.
        let mut expected = vec![0x08, 0x05, 0x12, 0x10];
        expected.extend_from_slice(&token);
        expected.extend_from_slice(&[0x20, 1, 0x28, 1, 0x30, 0x80, 0xe2, 0xcf, 0xaa, 0x06]);
        assert_eq!(encode(&fields(&token), true), expected);
        let old = Fields {
            version: 2,
            cache_id: None,
            ..fields(&token)
        };
        expected[1] = 2;
        assert_eq!(encode(&old, false), expected);
        assert_eq!(expected.len(), 30);
    }

    #[test]
    fn presence_unknown_enum_and_field7_are_preserved_without_invented_names() {
        let empty = Fields {
            version: 0,
            token: b"",
            cache_id: Some(b""),
            interactivity: 0,
            content_type: 0,
            timestamp: 0,
            field7: None,
        };
        assert!(encode(&empty, true).is_empty());
        assert_eq!(
            encode(&empty, false),
            [8, 0, 18, 0, 26, 0, 32, 0, 40, 0, 48, 0]
        );
        assert_eq!(
            encode(
                &Fields {
                    interactivity: 3,
                    field7: Some(0),
                    ..empty
                },
                true
            ),
            [0x20, 3, 0x38, 0]
        );
    }

    #[test]
    fn native_video_fixture_omits_zero_field5_and_negative_int32_sign_extends() {
        let token = [0xaa; 16];
        let video = encode(
            &Fields {
                content_type: 0,
                ..fields(&token)
            },
            true,
        );
        assert_eq!(video.len(), 28);
        let mut bytes = Vec::new();
        scalar(1, -1, &mut bytes);
        assert_eq!(bytes.len(), 11);
        assert_eq!(bytes[0], 8);
        assert_eq!(&bytes[1..10], &[0xff; 9]);
        assert_eq!(bytes[10], 1);
    }

    #[test]
    fn encoder_sign_extends_negative_enum_and_timestamp() {
        let input = Fields {
            version: 0,
            token: b"",
            cache_id: None,
            interactivity: -1,
            content_type: 0,
            timestamp: -1,
            field7: None,
        };
        let mut expected = vec![0x20];
        expected.extend_from_slice(&[0xff; 9]);
        expected.extend_from_slice(&[1, 0x30]);
        expected.extend_from_slice(&[0xff; 9]);
        expected.push(1);
        assert_eq!(encode(&input, true), expected);
    }

    #[test]
    fn profile_hash_size_offset_and_short_reads_are_checked() {
        let bytes: Vec<u8> = (0..64).collect();
        let expected = sha256(&bytes);
        let token = checked_token(Cursor::new(&bytes), 64, 9, &expected).unwrap();
        assert_eq!(token, bytes[9..25]);
        assert_eq!(
            checked_token(Cursor::new(&bytes), 63, 9, &expected),
            Err("dll_size_mismatch")
        );
        assert_eq!(
            checked_token(Cursor::new(&bytes), 65, 9, &expected),
            Err("dll_size_mismatch")
        );
        assert_eq!(
            checked_token(Cursor::new(&bytes), 64, 60, &expected),
            Err("invalid_profile_bounds")
        );
        assert_eq!(
            checked_token(Cursor::new(&bytes), 64, 9, "wrong"),
            Err("unsupported_dll_hash")
        );
        struct SmallReads(Cursor<Vec<u8>>);
        impl Read for SmallReads {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let n = out.len().min(3);
                self.0.read(&mut out[..n])
            }
        }
        assert_eq!(
            checked_token(SmallReads(Cursor::new(bytes)), 64, 9, &expected).unwrap(),
            token
        );
    }

    #[tokio::test]
    async fn bounded_original_body_handles_empty_exact_and_oversize() {
        for (input, limit, complete) in [("", 4, true), ("abcd", 4, true), ("abcde", 4, false)] {
            let mut body = Full::new(Bytes::from(input));
            let mut capture = Capture::default();
            capture_body(&mut body, &mut capture, limit).await;
            assert_eq!(capture.complete, complete);
            assert!(capture.bytes.len() <= limit);
            assert_eq!(capture.observed_bytes, input.len());
        }
    }

    #[tokio::test]
    async fn stream_failure_is_not_a_complete_response() {
        let stream = futures_util::stream::iter([
            Ok(http_body::Frame::data(Bytes::from_static(b"partial"))),
            Err(std::io::Error::other("DO_NOT_LEAK")),
        ]);
        let mut body = StreamBody::new(stream);
        let mut capture = Capture::default();
        capture_body(&mut body, &mut capture, BODY_LIMIT).await;
        assert_eq!(capture.observed_bytes, 7);
        assert_eq!(capture.failure, Some("body_stream_error"));
        let report = response_summary(403, &HeaderMap::new(), &capture);
        assert_eq!(report["outcome"], "incomplete_response");
        assert_eq!(report["recognized_error_code"], Value::Null);
        assert!(!report.to_string().contains("DO_NOT_LEAK"));
    }

    #[tokio::test]
    async fn production_deadline_preserves_partial_original_response() {
        let stream = futures_util::stream::iter([Ok::<_, std::io::Error>(http_body::Frame::data(
            Bytes::from_static(b"partial"),
        ))])
        .chain(futures_util::stream::pending());
        let capture = capture_with_deadline(
            StreamBody::new(stream),
            BODY_LIMIT,
            Duration::from_millis(5),
        )
        .await;
        assert_eq!(capture.observed_bytes, 7);
        assert_eq!(capture.bytes.len(), 7);
        assert_eq!(capture.failure, Some("body_deadline_exceeded"));
        let report = response_summary(429, &HeaderMap::new(), &capture);
        assert_eq!(report["status"], 429);
        assert_eq!(report["outcome"], "incomplete_response");
        assert_eq!(report["body_shape"], "unavailable");
    }

    struct BrokenWriter;
    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn broken_console_cannot_discard_saved_response() {
        let report = json!({"response": {"status": 403}, "playplay_dispatch_calls": 1});
        let mut output = Vec::new();
        assert_eq!(
            persist_report(&report, &mut output, &mut BrokenWriter),
            Ok(false)
        );
        assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), report);
        assert_eq!(output.last(), Some(&b'\n'));
    }

    #[test]
    fn failed_report_sink_still_allows_console_copy() {
        let report = json!({"response": {"status": 403}});
        let mut console = Vec::new();
        assert_eq!(
            persist_report(&report, &mut BrokenWriter, &mut console),
            Err("report_write_failed")
        );
        assert_eq!(serde_json::from_slice::<Value>(&console).unwrap(), report);
    }

    #[test]
    fn server_messages_and_headers_cannot_escape_allowlist() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "application/json; secret=DO_NOT_LEAK".parse().unwrap(),
        );
        headers.insert(header::SET_COOKIE, "DO_NOT_LEAK".parse().unwrap());
        headers.insert(
            header::LOCATION,
            "https://example.invalid/?token=DO_NOT_LEAK"
                .parse()
                .unwrap(),
        );
        let bytes =
            br#"{"error":{"code":"invalid_token","message":"DO_NOT_LEAK"},"token":"DO_NOT_LEAK"}"#
                .to_vec();
        let capture = Capture {
            observed_bytes: bytes.len(),
            bytes,
            complete: true,
            failure: None,
        };
        let summary = response_summary(403, &headers, &capture);
        assert_eq!(summary["recognized_error_code"], "INVALID_TOKEN");
        assert_eq!(summary["content_type"], "application/json");
        assert!(!summary.to_string().contains("DO_NOT_LEAK"));
        let capture = Capture {
            bytes: br#"{"error":"DO_NOT_LEAK"}"#.to_vec(),
            complete: true,
            ..Capture::default()
        };
        assert_eq!(
            response_summary(403, &headers, &capture)["recognized_error_code"],
            Value::Null
        );
    }

    fn response_fixture(first: Option<usize>, second: Option<usize>) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (field, size) in [(1, first), (2, second)] {
            if let Some(size) = size {
                blob(field, &vec![0xaa; size], &mut bytes);
            }
        }
        bytes
    }

    #[test]
    fn response_field_lengths_are_observations_not_validation() {
        for first in [None, Some(0), Some(15), Some(16), Some(17)] {
            for second in [None, Some(0), Some(3), Some(4), Some(5)] {
                let bytes = response_fixture(first, second);
                assert_eq!(protobuf_shape(&bytes), Some([first, second]));
                let capture = Capture {
                    observed_bytes: bytes.len(),
                    bytes,
                    complete: true,
                    failure: None,
                };
                let summary = response_summary(200, &HeaderMap::new(), &capture);
                assert_eq!(summary["field_1_length_if_protobuf"], json!(first));
                assert_eq!(summary["field_2_length_if_protobuf"], json!(second));
                assert_eq!(summary["license_validated"], false);
                assert_eq!(summary["decryption_tested"], false);
                assert_eq!(summary["playback_tested"], false);
            }
        }
    }

    #[test]
    fn synthetic_two_field_envelope_is_24_bytes_not_a_live_reconstruction() {
        let bytes = response_fixture(Some(16), Some(4));
        assert_eq!(bytes.len(), 24);
        assert_eq!(protobuf_shape(&bytes), Some([Some(16), Some(4)]));
        // A complete first field alone is also recognized; field 2 is optional
        // to this inspector. Missing field 2 does not imply a zero-valued field.
        assert_eq!(protobuf_shape(&bytes[..18]), Some([Some(16), None]));
        for end in 1..bytes.len() {
            if end != 18 {
                assert_eq!(protobuf_shape(&bytes[..end]), None, "prefix {end}");
            }
        }
    }

    #[test]
    fn unknown_wire_fields_do_not_acquire_semantics() {
        let mut bytes = response_fixture(Some(16), Some(4));
        scalar(3, 9, &mut bytes);
        bytes.push((4 << 3) | 1);
        bytes.extend_from_slice(&[0; 8]);
        blob(5, b"DO_NOT_LEAK", &mut bytes);
        bytes.push((6 << 3) | 5);
        bytes.extend_from_slice(&[0; 4]);
        assert_eq!(protobuf_shape(&bytes), Some([Some(16), Some(4)]));
        let unknown_only = &bytes[24..];
        assert_eq!(protobuf_shape(unknown_only), Some([None, None]));
        // Repeated unknown fields remain opaque rather than being interpreted
        // as another key, policy value, or native parser acceptance rule.
        bytes.extend_from_within(24..);
        assert_eq!(protobuf_shape(&bytes), Some([Some(16), Some(4)]));
    }

    #[test]
    fn duplicate_or_wrong_wire_known_fields_are_conservatively_unclassified() {
        for field in [1, 2] {
            for size in [0, 4, 16] {
                let mut bytes = response_fixture(Some(16), Some(4));
                blob(field, &vec![0; size], &mut bytes);
                assert_eq!(protobuf_shape(&bytes), None);
            }
            for wire in [0, 1, 5] {
                let size = match wire {
                    1 => 8,
                    5 => 4,
                    _ => 1,
                };
                let mut bytes = vec![(field << 3) | wire];
                bytes.resize(bytes.len() + size, 0);
                assert_eq!(protobuf_shape(&bytes), None);
            }
        }
        // Legal protobuf groups are outside this inspector's supported subset.
        assert_eq!(protobuf_shape(&[0x1b, 0x1c]), None);
    }

    #[test]
    fn response_varints_and_inspection_budget_have_explicit_boundaries() {
        // Nonminimal tag and length encodings remain syntax-only observations.
        let mut nonminimal = vec![0x8a, 0, 0x90, 0];
        nonminimal.extend_from_slice(&[0xaa; 16]);
        nonminimal.extend_from_slice(&[0x92, 0, 0x84, 0]);
        nonminimal.extend_from_slice(&[0xaa; 4]);
        assert_eq!(protobuf_shape(&nonminimal), Some([Some(16), Some(4)]));

        let mut maximum_field = Vec::new();
        varint((0x1fff_ffff_u64 << 3) | 2, &mut maximum_field);
        maximum_field.push(0);
        assert_eq!(protobuf_shape(&maximum_field), Some([None, None]));
        let mut oversized_field = Vec::new();
        varint(0x2000_0000_u64 << 3, &mut oversized_field);
        oversized_field.push(0);
        assert_eq!(protobuf_shape(&oversized_field), None);
        let mut impossible_length = vec![0x0a];
        varint(u64::MAX, &mut impossible_length);
        assert_eq!(protobuf_shape(&impossible_length), None);
        assert_eq!(protobuf_shape(&[0x18, 0x80]), None);
        assert_eq!(protobuf_shape(&[0x0a, 0x80]), None);

        let mut maximum_fields = [0x18, 0].repeat(1024);
        assert_eq!(protobuf_shape(&maximum_fields), Some([None, None]));
        maximum_fields.extend_from_slice(&[0x18, 0]);
        assert_eq!(protobuf_shape(&maximum_fields), None);
    }

    #[test]
    fn response_material_never_escapes_even_with_matching_lengths() {
        let mut bytes = Vec::new();
        blob(1, b"KEY_MATERIAL_123", &mut bytes);
        blob(2, b"LEAK", &mut bytes);
        assert_eq!(bytes.len(), 24);
        let mut capture = Capture {
            observed_bytes: bytes.len(),
            bytes,
            complete: true,
            failure: None,
        };
        for status in [200, 201, 403] {
            let summary = response_summary(status, &HeaderMap::new(), &capture);
            assert_eq!(summary["field_1_length_if_protobuf"], 16);
            assert_eq!(summary["field_2_length_if_protobuf"], 4);
            for flag in [
                "license_validated",
                "decryption_tested",
                "playback_tested",
                "tier_restriction_established",
            ] {
                assert_eq!(summary[flag], false);
            }
            let text = summary.to_string();
            assert!(!text.contains("KEY_MATERIAL_123"));
            assert!(!text.contains("LEAK"));
        }
        // Even a syntactically complete retained prefix is not a complete body.
        capture.complete = false;
        capture.failure = Some("body_deadline_exceeded");
        let summary = response_summary(200, &HeaderMap::new(), &capture);
        assert_eq!(summary["body_shape"], "unavailable");
        assert_eq!(summary["field_1_length_if_protobuf"], Value::Null);
        assert_eq!(summary["field_2_length_if_protobuf"], Value::Null);
    }

    #[test]
    fn protobuf_shape_never_reports_keys_and_rejects_malformed_lengths() {
        let mut bytes = vec![0x0a, 16];
        bytes.extend_from_slice(&[0xaa; 16]);
        let capture = Capture {
            observed_bytes: bytes.len(),
            bytes,
            complete: true,
            failure: None,
        };
        let summary = response_summary(200, &HeaderMap::new(), &capture);
        assert_eq!(summary["field_1_length_if_protobuf"], 16);
        assert_eq!(summary["license_validated"], false);
        assert_eq!(summary["outcome"], "http_2xx_unvalidated");
        for input in [
            &[0x0a, 0xff][..],
            &[0x00][..],
            &[0x0a, 1][..],
            &[0xff; 12][..],
            &[0x0a, 0, 0x0a, 0][..],
        ] {
            assert_eq!(protobuf_shape(input), None);
        }
        let mut position = 0;
        assert_eq!(read_varint(&[0xff; 10], &mut position), None);
    }
}
