//! Bounded real-content candidate test. No values, signed URLs or audio in reports.
//! The old transform is a hypothesis, not an implementation of verified v5 semantics.
use std::{
    io::{Cursor, Read},
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};

use bytes::Bytes;
use http::{Request, header};
use librespot::core::{FileId, audio_key::AudioKey, cdn_url::CdnUrl, session::Session};
use serde_json::{Value, json};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_VORBIS, DecoderOptions},
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{Failure, Result, deadline, probe};

pub const AUDIO_LIMIT: usize = 512 * 1024;
const OGG_OFFSET: usize = 167;
const REQUIRED_FRAMES: usize = 44_100 * 5;
const TABLE_HASH: &str = "c9f69fe06130ec69d86f31553ce2f06e8ffce4350bf1610a92bdef38dcf3ae09";

pub struct CandidateProfile {
    pub worker: std::path::PathBuf,
    pub table: Vec<u8>,
}

/// Suppress memory-bearing crash files before any authentication or live input.
pub fn disable_core_dumps() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: setrlimit reads a valid rlimit; prctl arguments match Linux ABI.
        if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0
            || unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } != 0
        {
            return Err(Failure::new("preflight", "cannot_disable_core_dumps"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err(Failure::new("preflight", "decode_requires_linux"))
}

pub fn candidate_profile(worker: &Path, table: &Path) -> Result<CandidateProfile> {
    let worker = worker
        .canonicalize()
        .map_err(|_| Failure::new("preflight", "candidate_worker_missing"))?;
    if !worker.is_file() {
        return Err(Failure::new("preflight", "candidate_worker_not_file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(table)
        .map_err(|_| Failure::new("preflight", "candidate_table_unreadable"))?
        .take(3073)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::new("preflight", "candidate_table_unreadable"))?;
    if bytes.len() != 3072 || probe::sha256(&bytes) != TABLE_HASH {
        return Err(Failure::new("preflight", "candidate_table_mismatch"));
    }
    Ok(CandidateProfile {
        worker,
        table: bytes,
    })
}

async fn child_output(
    program: &Path,
    args: &[&str],
    input: &[u8],
    limit: usize,
) -> Result<Vec<u8>> {
    let run = async {
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| Failure::new("candidate", "worker_start_failed"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Failure::new("candidate", "worker_pipe_failed"))?;
        stdin
            .write_all(input)
            .await
            .map_err(|_| Failure::new("candidate", "worker_input_failed"))?;
        drop(stdin);
        let mut output = Vec::new();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Failure::new("candidate", "worker_pipe_failed"))?;
        stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut output)
            .await
            .map_err(|_| Failure::new("candidate", "worker_output_failed"))?;
        if output.len() > limit {
            return Err(Failure::new("candidate", "worker_output_size"));
        }
        if !child
            .wait()
            .await
            .map_err(|_| Failure::new("candidate", "worker_wait_failed"))?
            .success()
        {
            return Err(Failure::new("candidate", "worker_failed"));
        }
        Ok(output)
    };
    tokio::time::timeout(Duration::from_secs(12), run)
        .await
        .map_err(|_| Failure::new("candidate", "worker_deadline"))?
}

async fn candidate_keys(
    profile: &CandidateProfile,
    material: &[u8; 16],
    file: FileId,
) -> Result<[[u8; 16]; 2]> {
    let mut packet = Vec::with_capacity(3116);
    packet.extend_from_slice(b"LPPCAN01");
    packet.extend_from_slice(&profile.table);
    packet.extend_from_slice(material);
    packet.extend_from_slice(&file.0);
    let output = child_output(&profile.worker, &[], &packet, 32).await?;
    if output.len() != 32 {
        return Err(Failure::new("candidate", "worker_output_size"));
    }
    let mut keys = [[0; 16]; 2];
    keys[0].copy_from_slice(&output[..16]);
    keys[1].copy_from_slice(&output[16..]);
    Ok(keys)
}

pub async fn preflight(profile: &CandidateProfile) -> Result<()> {
    // Seconds-long pipe/executable check before login, not a v5 compatibility test.
    candidate_keys(profile, &[0; 16], FileId([0; 20])).await?;
    Ok(())
}

fn trusted_cdn(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "https"
            && u.port_or_known_default() == Some(443)
            && u.username().is_empty()
            && u.password().is_none()
            && u.fragment().is_none()
            && u.host_str().is_some_and(|h| {
                h.ends_with(".spotifycdn.com")
                    || h.ends_with(".scdn.co")
                    || h == "audio-ak-spotify-com.akamaized.net"
            })
    })
}

fn range_size(value: &str) -> Option<usize> {
    let (range, total) = value.strip_prefix("bytes 0-")?.split_once('/')?;
    let end = range.parse::<usize>().ok()?;
    let total = total.parse::<usize>().ok()?;
    let size = end.checked_add(1)?;
    (size == AUDIO_LIMIT.min(total)).then_some(size)
}

pub async fn audio_prefix(session: &Session, file: FileId, report: &mut Value) -> Result<Vec<u8>> {
    let cdn = deadline("cdn_resolve", CdnUrl::new(file).resolve_audio(session)).await?;
    let urls = cdn
        .try_get_urls()
        .map_err(|e| Failure::core("cdn_resolve", e))?;
    let url = urls
        .first()
        .copied()
        .filter(|u| trusted_cdn(u))
        .ok_or_else(|| Failure::new("cdn", "no_trusted_cdn_url"))?;
    let parsed = url::Url::parse(url).map_err(|_| Failure::new("cdn", "invalid_url"))?;
    if parsed.path() != format!("/audio/{}", file.to_base16()) {
        return Err(Failure::new("cdn", "resolved_file_path_mismatch"));
    }
    // Do not attach bearer/client-token headers to the signed CDN URL. No redirects/retry.
    let request = Request::builder()
        .uri(url)
        .header(header::RANGE, format!("bytes=0-{}", AUDIO_LIMIT - 1))
        .header(header::ACCEPT_ENCODING, "identity")
        .body(Bytes::new())
        .map_err(|_| Failure::new("cdn", "request_construction_failed"))?;
    report["cdn_dispatch_calls"] = json!(1);
    let future = session
        .http_client()
        .request_fut(request)
        .map_err(|e| Failure::core("cdn", e))?;
    let response = tokio::time::timeout(Duration::from_secs(30), future)
        .await
        .map_err(|_| Failure::new("cdn", "headers_deadline"))?
        .map_err(|_| Failure::new("cdn", "transport_error"))?;
    report["cdn_status"] = json!(response.status().as_u16());
    if response.status() != http::StatusCode::PARTIAL_CONTENT {
        return Err(Failure::new("cdn", "expected_partial_content"));
    }
    if response
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_some_and(|v| v != "identity")
    {
        return Err(Failure::new("cdn", "unexpected_content_encoding"));
    }
    let size = response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(range_size)
        .ok_or_else(|| Failure::new("cdn", "invalid_zero_based_content_range"))?;
    let capture =
        probe::capture_with_deadline(response.into_body(), AUDIO_LIMIT, Duration::from_secs(20))
            .await;
    report["encrypted_prefix_bytes"] = json!(capture.bytes.len());
    report["cdn_body_complete"] = json!(capture.complete);
    if !capture.complete || capture.bytes.len() != size {
        return Err(Failure::new("cdn", "incomplete_or_mismatched_range"));
    }
    if size < OGG_OFFSET + 27 {
        return Err(Failure::new("cdn", "prefix_too_short_for_ogg"));
    }
    Ok(capture.bytes)
}

fn ogg_crc(page: &[u8]) -> u32 {
    let mut crc = 0u32;
    for (i, &b) in page.iter().enumerate() {
        crc ^= u32::from(if (22..26).contains(&i) { 0 } else { b }) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Validate every complete page in the range. A trailing partial page is not
/// counted or fed to the decoder. This validates a prefix, not whole-file EOS.
fn complete_ogg_prefix(bytes: &[u8]) -> std::result::Result<usize, &'static str> {
    let mut at = 0usize;
    let mut serial = None;
    let mut sequence = 0u32;
    while at < bytes.len() {
        let tail = &bytes[at..];
        if tail.len() < 27 {
            break;
        }
        if &tail[..4] != b"OggS" || tail[4] != 0 {
            return Err("ogg_capture_mismatch");
        }
        let count = tail[26] as usize;
        if tail.len() < 27 + count {
            break;
        }
        let size = 27
            + count
            + tail[27..27 + count]
                .iter()
                .map(|&x| x as usize)
                .sum::<usize>();
        if tail.len() < size {
            break;
        }
        let page = &tail[..size];
        let id = u32::from_le_bytes(page[14..18].try_into().unwrap());
        if serial.is_some_and(|s| s != id)
            || u32::from_le_bytes(page[18..22].try_into().unwrap()) != sequence
        {
            return Err("ogg_sequence_mismatch");
        }
        if sequence == 0
            && (page[5] & 2 == 0
                || page[5] & 1 != 0
                || page.get(27 + count..27 + count + 7) != Some(b"\x01vorbis".as_slice()))
        {
            return Err("vorbis_identification_missing");
        }
        if ogg_crc(page) != u32::from_le_bytes(page[22..26].try_into().unwrap()) {
            return Err("ogg_crc_mismatch");
        }
        serial = Some(id);
        sequence += 1;
        at += size;
    }
    if sequence < 3 {
        return Err("insufficient_complete_ogg_pages");
    }
    Ok(at)
}

fn decode_candidate(encrypted: &[u8], key: [u8; 16]) -> std::result::Result<Value, &'static str> {
    decode_candidate_with_budget(encrypted, key, 2048)
}

fn decode_candidate_with_budget(
    encrypted: &[u8],
    key: [u8; 16],
    max_packets: usize,
) -> std::result::Result<Value, &'static str> {
    let mut plain = Vec::with_capacity(encrypted.len());
    // Always decrypt from absolute file offset zero; only then remove Spotify prefix.
    librespot::audio::AudioDecrypt::new(Some(AudioKey(key)), Cursor::new(encrypted))
        .read_to_end(&mut plain)
        .map_err(|_| "decrypt_read_failed")?;
    let ogg = plain.get(OGG_OFFSET..).ok_or("audio_prefix_too_short")?;
    let valid = complete_ogg_prefix(ogg)?;
    let stream = MediaSourceStream::new(
        Box::new(Cursor::new(ogg[..valid].to_vec())),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut parsed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| "container_probe_failed")?;
    let track = parsed
        .format
        .default_track()
        .ok_or("default_track_missing")?;
    let id = track.id;
    if track.codec_params.codec != CODEC_TYPE_VORBIS
        || track.codec_params.sample_rate != Some(44_100)
        || track.codec_params.channels.is_none_or(|c| c.count() != 2)
    {
        return Err("unsupported_stream_parameters");
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| "decoder_creation_failed")?;
    let started = Instant::now();
    let mut frames = 0usize;
    let mut packets = 0usize;
    let mut nonzero = false;
    while frames < REQUIRED_FRAMES {
        if packets >= max_packets || started.elapsed() > Duration::from_secs(5) {
            return Err("decoder_budget_exceeded");
        }
        let packet = parsed
            .format
            .next_packet()
            .map_err(|_| "prefix_ended_before_required_pcm")?;
        if packet.track_id() != id {
            return Err("unexpected_track");
        }
        // No malformed-packet skipping or EOF-as-success fallback.
        let decoded = decoder
            .decode(&packet)
            .map_err(|_| "packet_decode_failed")?;
        if decoded.spec().rate != 44_100
            || decoded.spec().channels.count() != 2
            || decoded.capacity() > 65_536
        {
            return Err("decoded_parameters_or_capacity");
        }
        let n = decoded.frames();
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        samples.copy_interleaved_ref(decoded);
        if !samples.samples().iter().all(|s| s.is_finite()) {
            return Err("nonfinite_pcm");
        }
        nonzero |= samples.samples().iter().any(|&s| s != 0.0);
        frames += n;
        packets += 1;
    }
    Ok(
        json!({"decoded_frames":frames,"decoded_packets":packets,"sample_rate":44100,"channels":2,
        "decoded_duration_ms":frames*1000/44100,"finite_pcm":true,"nonzero_pcm":nonzero,
        "validated_ogg_prefix_bytes":valid,"audio_decode_validated":true,"whole_track_validated":false}),
    )
}

fn candidate_outcome(name: &str, result: std::result::Result<Value, &'static str>) -> Value {
    match result {
        Ok(mut value) => {
            value["candidate"] = json!(name);
            value["classification"] = json!("decoded_pcm");
            value
        }
        Err(code) => {
            // Resource limits, panics, unsupported decoder setup and short
            // prefixes are not evidence that a candidate key is wrong.
            let rejected = matches!(
                code,
                "ogg_capture_mismatch"
                    | "ogg_sequence_mismatch"
                    | "vorbis_identification_missing"
                    | "ogg_crc_mismatch"
                    | "packet_decode_failed"
                    | "nonfinite_pcm"
            );
            json!({"candidate":name,"audio_decode_validated":false,"failure":code,
                "classification":if rejected {"content_rejected"} else {"inconclusive"}})
        }
    }
}

fn record_outcomes(outcomes: Vec<Value>, report: &mut Value) -> Result<()> {
    if outcomes.len() != 2 {
        return Err(Failure::new("decode", "invalid_child_report"));
    }
    let success = outcomes.iter().any(|r| r["audio_decode_validated"] == true);
    let inconclusive = !success
        && outcomes
            .iter()
            .any(|r| r["classification"] != "content_rejected");
    report["decode_attempt"]["candidates"] = json!(outcomes);
    report["decode_attempt"]["audio_decode_validated"] = json!(success);
    if inconclusive {
        return Err(Failure::new("decode", "inconclusive_candidate_attempt"));
    }
    report["outcome"] = json!(if success {
        "candidate_audio_prefix_decoded"
    } else {
        "tested_candidates_did_not_decode"
    });
    Ok(())
}

pub fn decoder_child() -> u8 {
    #[cfg(target_os = "linux")]
    {
        if disable_core_dumps().is_err() {
            return 7;
        }
        let memory = libc::rlimit {
            rlim_cur: 512 * 1024 * 1024,
            rlim_max: 512 * 1024 * 1024,
        };
        let cpu = libc::rlimit {
            rlim_cur: 8,
            rlim_max: 8,
        };
        // SAFETY: valid limit structures, this disposable process only.
        if unsafe { libc::setrlimit(libc::RLIMIT_AS, &memory) } != 0
            || unsafe { libc::setrlimit(libc::RLIMIT_CPU, &cpu) } != 0
        {
            return 7;
        }
        let mut input = Vec::new();
        if std::io::stdin()
            .take((AUDIO_LIMIT + 41) as u64)
            .read_to_end(&mut input)
            .is_err()
            || input.len() < 40
            || input.len() > AUDIO_LIMIT + 40
            || &input[..8] != b"LPPAUD01"
        {
            return 2;
        }
        let mut outcomes = Vec::new();
        for (n, name) in [
            "historical_v2_transform_and_binding",
            "historical_v2_transform_without_binding",
        ]
        .into_iter()
        .enumerate()
        {
            let mut key = [0; 16];
            key.copy_from_slice(&input[8 + n * 16..24 + n * 16]);
            let result = std::panic::catch_unwind(|| decode_candidate(&input[40..], key))
                .unwrap_or(Err("decoder_panicked"));
            outcomes.push(candidate_outcome(name, result));
        }
        if serde_json::to_writer(std::io::stdout().lock(), &outcomes).is_err() {
            return 3;
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        7
    }
}

pub async fn run(
    file: FileId,
    material: [u8; 16],
    profile: &CandidateProfile,
    encrypted: &[u8],
    report: &mut Value,
) -> Result<()> {
    report["decode_attempt"] = json!({"candidate_compatibility":"unproven","candidates":[],"audio_decode_validated":false});
    let keys = candidate_keys(profile, &material, file).await?;
    let mut packet = Vec::with_capacity(40 + encrypted.len());
    packet.extend_from_slice(b"LPPAUD01");
    for key in keys {
        packet.extend_from_slice(&key);
    }
    packet.extend_from_slice(encrypted);
    let executable =
        std::env::current_exe().map_err(|_| Failure::new("decode", "executable_unavailable"))?;
    report["decryption_tested"] = json!(true);
    let output = child_output(&executable, &["--decode-child"], &packet, 4096).await?;
    let outcomes: Vec<Value> = serde_json::from_slice(&output)
        .map_err(|_| Failure::new("decode", "invalid_child_report"))?;
    // No license/policy validation, audible playback or whole-track claim.
    record_outcomes(outcomes, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_and_cdn_guards() {
        assert_eq!(range_size("bytes 0-524287/3000000"), Some(AUDIO_LIMIT));
        assert_eq!(range_size("bytes 0-99/100"), Some(100));
        for bad in [
            "bytes 0-99/3000000",
            "bytes 1-50/100",
            "bytes 0-524288/900000",
            "bytes 0-99/50",
            "bytes 0-1/*",
        ] {
            assert_eq!(range_size(bad), None);
        }
        assert!(trusted_cdn(
            "https://audio-cf.spotifycdn.com/audio/test?verify=not_logged"
        ));
        assert!(trusted_cdn(
            "https://audio4-fa.scdn.co/audio/test?verify=not_logged"
        ));
        assert!(!trusted_cdn("https://scdn.co.evil.invalid/audio/test"));
        for bad in [
            "http://audio-cf.spotifycdn.com/a",
            "https://spotifycdn.com.evil.invalid/a",
            "https://u:p@audio-cf.spotifycdn.com/a",
        ] {
            assert!(!trusted_cdn(bad));
        }
    }
    #[test]
    fn generated_ogg_fixture_reaches_real_pcm() {
        // CI supplies an ffmpeg-generated lawful tone; no account/media sample.
        let Ok(path) = std::env::var("PROBE_SYNTHETIC_OGG") else {
            return;
        };
        let ogg = std::fs::read(path).unwrap();
        let mut plain = vec![0; OGG_OFFSET];
        plain.extend_from_slice(&ogg);
        assert!(plain.len() < AUDIO_LIMIT);
        let key = [0x5c; 16];
        let mut encrypted = Vec::new();
        librespot::audio::AudioDecrypt::new(Some(AudioKey(key)), Cursor::new(&plain))
            .read_to_end(&mut encrypted)
            .unwrap();
        let result = decode_candidate(&encrypted, key).unwrap();
        assert_eq!(result["audio_decode_validated"], true);
        assert!(result["decoded_frames"].as_u64().unwrap() >= REQUIRED_FRAMES as u64);
        assert_eq!(result["nonzero_pcm"], true);
        let limited = decode_candidate_with_budget(&encrypted, key, 0);
        assert_eq!(limited, Err("decoder_budget_exceeded"));
        let mut report = json!({"decode_attempt":{},"outcome":"not_classified"});
        let outcomes = vec![
            candidate_outcome("first", limited),
            candidate_outcome("second", Err("ogg_capture_mismatch")),
        ];
        let error = record_outcomes(outcomes, &mut report).err().unwrap();
        assert_eq!(error.code, "inconclusive_candidate_attempt");
        assert_ne!(report["outcome"], "tested_candidates_did_not_decode");
        assert!(decode_candidate(&encrypted, [0x5d; 16]).is_err());
        encrypted[OGG_OFFSET + 40] ^= 1;
        assert!(decode_candidate(&encrypted, key).is_err());
    }

    #[test]
    fn panic_and_resource_failures_cannot_reject_candidates() {
        let caught = std::panic::catch_unwind(|| -> std::result::Result<Value, &'static str> {
            panic!("synthetic decoder panic");
        })
        .unwrap_or(Err("decoder_panicked"));
        assert_eq!(caught, Err("decoder_panicked"));
        assert_eq!(
            candidate_outcome("panic", caught)["classification"],
            "inconclusive"
        );
        for code in [
            "decoder_panicked",
            "decoder_budget_exceeded",
            "prefix_ended_before_required_pcm",
        ] {
            let mut report = json!({"decode_attempt":{},"outcome":"not_classified"});
            let outcomes = vec![
                candidate_outcome("first", Err(code)),
                candidate_outcome("second", Err("ogg_crc_mismatch")),
            ];
            assert!(record_outcomes(outcomes, &mut report).is_err());
            assert_ne!(report["outcome"], "tested_candidates_did_not_decode");
            assert_eq!(
                report["decode_attempt"]["candidates"][0]["classification"],
                "inconclusive"
            );
        }
    }

    #[test]
    fn wrong_key_does_not_become_decode_success() {
        assert!(decode_candidate(&vec![0; 4096], [0xaa; 16]).is_err());
        assert!(complete_ogg_prefix(b"OggS").is_err());
    }
}
