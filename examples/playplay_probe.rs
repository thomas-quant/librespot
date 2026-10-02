//! One-request PlayPlay diagnostic, NOT a production key manager or player.
//! Default is offline. `--send` makes at most one application-level PlayPlay POST.
//! Optional `--try-legacy-decode` tests two unproven historical candidates against
//! one matching encrypted audio prefix. Nothing sensitive is retained in reports.

#[path = "support/playplay_decode.rs"]
mod decode;
#[path = "support/playplay_probe.rs"]
mod probe;

use std::{
    collections::{HashSet, VecDeque},
    env,
    fs::{self, File, OpenOptions},
    future::Future,
    io::{Read, Write},
    path::PathBuf,
    process::ExitCode,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use http::{Method, Request, header};
use librespot::{
    core::{
        Error, FileId, SpotifyId, SpotifyUri, authentication::Credentials, config::SessionConfig,
        error::ErrorKind, session::Session, version,
    },
    metadata::audio::{AudioFileFormat, AudioItem},
    oauth::OAuthClientBuilder,
};
use serde_json::{Value, json};

const CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const DEFAULT_TRACK: &str = "4cOdK2wGLETKBW3PvgPWqT";
const STEP_TIMEOUT: Duration = Duration::from_secs(30);
const BODY_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_METADATA_ITEMS: usize = 8;

struct Options {
    dll: PathBuf,
    track: String,
    format: AudioFileFormat,
    expected_tier: &'static str,
    send: bool,
    access_token_file: Option<PathBuf>,
    decode: Option<(PathBuf, PathBuf)>,
    report: PathBuf,
}

struct Failure {
    stage: &'static str,
    code: &'static str,
    kind: Option<ErrorKind>,
}

impl Failure {
    fn new(stage: &'static str, code: &'static str) -> Self {
        Self {
            stage,
            code,
            kind: None,
        }
    }

    fn core(stage: &'static str, error: Error) -> Self {
        // Never format the underlying error: it may contain a URL or server text.
        Self {
            stage,
            code: "librespot_error",
            kind: Some(error.kind),
        }
    }

    fn report(&self) -> Value {
        json!({"stage": self.stage, "code": self.code,
               "kind": self.kind.map(|kind| format!("{kind:?}"))})
    }
}

type Result<T> = std::result::Result<T, Failure>;

fn options(args: &[String]) -> Result<Option<Options>> {
    let mut cli = getopts::Options::new();
    cli.optflag("h", "help", "show help; no network activity");
    cli.optflag(
        "",
        "send",
        "enable authentication/metadata and one PlayPlay POST",
    );
    cli.optopt(
        "",
        "spotify-dll",
        "path to the exact researched Spotify.dll",
        "PATH",
    );
    cli.optopt(
        "",
        "track",
        "public base62 track ID (default: known test track)",
        "ID",
    );
    cli.optopt(
        "",
        "format",
        "ogg96 (default), ogg160, ogg320, or aac24; no fallback",
        "FORMAT",
    );
    cli.optopt(
        "",
        "expected-tier",
        "free (default) or premium; mismatch prevents POST",
        "TIER",
    );
    cli.optopt(
        "",
        "access-token-file",
        "explicit token file instead of browser OAuth; never cached",
        "PATH",
    );
    cli.optopt(
        "",
        "report",
        "new JSON file; never overwrite an existing file",
        "PATH",
    );
    cli.optflag(
        "",
        "try-legacy-decode",
        "test unproven legacy candidates on one real OGG96 prefix",
    );
    cli.optopt(
        "",
        "legacy-worker",
        "CI-built bounded legacy candidate worker",
        "PATH",
    );
    cli.optopt(
        "",
        "legacy-table",
        "local hash-pinned legacy table; never uploaded",
        "PATH",
    );
    let parsed = cli
        .parse(args)
        .map_err(|_| Failure::new("arguments", "invalid_options"))?;
    if parsed.opt_present("help") {
        println!("{}", cli.usage("Usage: playplay_probe --spotify-dll PATH [--send] [options]\n\nWithout --send: offline profile validation only. A 2xx is not proof of a valid license or playback."));
        return Ok(None);
    }
    if !parsed.free.is_empty() {
        return Err(Failure::new("arguments", "unexpected_positional_argument"));
    }
    let dll = PathBuf::from(
        parsed
            .opt_str("spotify-dll")
            .ok_or_else(|| Failure::new("arguments", "spotify_dll_required"))?,
    );
    let format = match parsed.opt_str("format").as_deref().unwrap_or("ogg96") {
        "ogg96" => AudioFileFormat::OGG_VORBIS_96,
        "ogg160" => AudioFileFormat::OGG_VORBIS_160,
        "ogg320" => AudioFileFormat::OGG_VORBIS_320,
        "aac24" => AudioFileFormat::AAC_24,
        _ => return Err(Failure::new("arguments", "unsupported_format")),
    };
    let decode = if parsed.opt_present("try-legacy-decode") {
        if format != AudioFileFormat::OGG_VORBIS_96 {
            return Err(Failure::new("arguments", "decode_candidate_requires_ogg96"));
        }
        Some((
            PathBuf::from(
                parsed
                    .opt_str("legacy-worker")
                    .ok_or_else(|| Failure::new("arguments", "legacy_worker_required"))?,
            ),
            PathBuf::from(
                parsed
                    .opt_str("legacy-table")
                    .ok_or_else(|| Failure::new("arguments", "legacy_table_required"))?,
            ),
        ))
    } else {
        if parsed.opt_present("legacy-worker") || parsed.opt_present("legacy-table") {
            return Err(Failure::new("arguments", "decode_flag_required"));
        }
        None
    };
    let expected_tier = match parsed.opt_str("expected-tier").as_deref().unwrap_or("free") {
        "free" => "free",
        "premium" => "premium",
        _ => return Err(Failure::new("arguments", "unsupported_expected_tier")),
    };
    let track = parsed
        .opt_str("track")
        .unwrap_or_else(|| DEFAULT_TRACK.to_owned());
    SpotifyId::from_base62(&track).map_err(|_| Failure::new("arguments", "invalid_track_id"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Failure::new("clock", "clock_before_unix_epoch"))?;
    let report = parsed
        .opt_str("report")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(format!(
                "local-test-data/playplay-probe/result-{}.json",
                now.as_nanos()
            ))
        });
    Ok(Some(Options {
        dll,
        track,
        format,
        expected_tier,
        send: parsed.opt_present("send"),
        access_token_file: parsed.opt_str("access-token-file").map(PathBuf::from),
        decode,
        report,
    }))
}

fn profile(options: &Options) -> Result<[u8; 16]> {
    let metadata =
        fs::metadata(&options.dll).map_err(|_| Failure::new("profile", "dll_unreadable"))?;
    if !metadata.is_file() || metadata.len() != probe::DLL_SIZE as u64 {
        return Err(Failure::new("profile", "unsupported_dll_size_or_type"));
    }
    let file = File::open(&options.dll).map_err(|_| Failure::new("profile", "dll_unreadable"))?;
    let token = probe::checked_token(
        file,
        probe::DLL_SIZE,
        probe::TOKEN_OFFSET,
        probe::DLL_SHA256,
    )
    .map_err(|code| Failure::new("profile", code))?;
    if probe::sha256(&token) != probe::TOKEN_SHA256 {
        return Err(Failure::new(
            "profile",
            "profile_token_fingerprint_mismatch",
        ));
    }
    Ok(token)
}

fn reserve_report(path: &PathBuf) -> Result<File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|_| Failure::new("report", "cannot_create_report_directory"))?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|_| Failure::new("report", "report_exists_or_cannot_create"))
}

async fn deadline<T, F>(stage: &'static str, future: F) -> Result<T>
where
    F: Future<Output = std::result::Result<T, Error>>,
{
    tokio::time::timeout(STEP_TIMEOUT, future)
        .await
        .map_err(|_| Failure::new(stage, "deadline_exceeded"))?
        .map_err(|error| Failure::core(stage, error))
}

async fn credentials(options: &Options) -> Result<Credentials> {
    if let Some(path) = &options.access_token_file {
        let metadata = fs::metadata(path)
            .map_err(|_| Failure::new("authentication", "token_file_unreadable"))?;
        if !metadata.is_file() || metadata.len() > 8192 {
            return Err(Failure::new("authentication", "invalid_token_file"));
        }
        let mut token = String::new();
        File::open(path)
            .map_err(|_| Failure::new("authentication", "token_file_unreadable"))?
            .take(8193)
            .read_to_string(&mut token)
            .map_err(|_| Failure::new("authentication", "invalid_token_file"))?;
        let token = token.trim();
        if token.is_empty()
            || token.len() > 8192
            || !token.is_ascii()
            || token.chars().any(char::is_whitespace)
        {
            return Err(Failure::new("authentication", "invalid_token_file"));
        }
        return Ok(Credentials::with_access_token(token));
    }
    // Existing OAuth library prints a browser authorization URL. No logger is
    // installed by this executable: RUST_LOG cannot enable token/URI trace logs.
    // The inherited OAuth flow (including token exchange) has no probe deadline.
    // Only post-OAuth service stages and the PlayPlay body capture are bounded.
    let client = OAuthClientBuilder::new(CLIENT_ID, REDIRECT_URI, vec!["streaming"])
        .build()
        .map_err(|_| Failure::new("authentication", "oauth_configuration_failed"))?;
    let token = client
        .get_access_token_async()
        .await
        .map_err(|_| Failure::new("authentication", "oauth_failed"))?;
    Ok(Credentials::with_access_token(&token.access_token))
}

#[derive(Clone)]
struct MetadataCandidate {
    uri: SpotifyUri,
    available: bool,
    requested_file: Option<FileId>,
    alternatives: Vec<SpotifyUri>,
}

async fn select_file(session: &Session, options: &Options, report: &mut Value) -> Result<FileId> {
    let id = SpotifyId::from_base62(&options.track)
        .map_err(|_| Failure::new("metadata", "invalid_track_id"))?;
    select_with(SpotifyUri::Track { id }, report, |uri| async move {
        let item = deadline("metadata", AudioItem::get_file(session, uri)).await?;
        Ok(MetadataCandidate {
            requested_file: item.files.get(&options.format).copied(),
            available: item.availability.is_ok(),
            uri: item.track_id,
            alternatives: item.alternatives.map(|a| a.0).unwrap_or_default(),
        })
    })
    .await
}

async fn select_with<F, Fut>(start: SpotifyUri, report: &mut Value, mut lookup: F) -> Result<FileId>
where
    F: FnMut(SpotifyUri) -> Fut,
    Fut: Future<Output = Result<MetadataCandidate>>,
{
    let mut scheduled = HashSet::from([start.to_string()]);
    let mut queue = VecDeque::from([start]);
    let mut inspected = 0;
    let mut failures = 0;
    while inspected < MAX_METADATA_ITEMS {
        let Some(uri) = queue.pop_front() else { break };
        inspected += 1;
        report["metadata_items_checked"] = json!(inspected);
        let item = match lookup(uri).await {
            Ok(item) => item,
            Err(_) => {
                failures += 1;
                report["metadata_lookup_failures"] = json!(failures);
                continue;
            }
        };
        if item.available {
            if let Some(file) = item.requested_file {
                if file.0 != [0; 20] {
                    report["selected_track"] = json!(item.uri.to_string());
                    report["file_id_sha256"] = json!(probe::sha256(&file.0));
                    return Ok(file);
                }
            }
        }
        for alternative in item.alternatives {
            if inspected + queue.len() >= MAX_METADATA_ITEMS {
                break;
            }
            // This set covers BOTH fetched and queued IDs. Duplicates must not
            // consume the limited number of remaining unique lookup slots.
            if scheduled.insert(alternative.to_string()) {
                queue.push_back(alternative);
            }
        }
    }
    Err(Failure::new(
        "metadata",
        "no_available_file_for_requested_format_within_budget",
    ))
}

fn endpoint(base: &str, file: FileId) -> Result<url::Url> {
    let mut url =
        url::Url::parse(base).map_err(|_| Failure::new("endpoint", "invalid_base_url"))?;
    let trusted_host = url
        .host_str()
        .is_some_and(|host| host.ends_with(".spotify.com"));
    if url.scheme() != "https"
        || !trusted_host
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(Failure::new("endpoint", "untrusted_base_url"));
    }
    url.set_path(&format!("/playplay/v1/key/{}", file.to_base16()));
    Ok(url)
}

async fn live(
    options: &Options,
    token: &[u8; 16],
    candidate: Option<&decode::CandidateProfile>,
    report: &mut Value,
) -> Result<()> {
    let credentials = credentials(options).await?;
    let session = Session::new(SessionConfig::default(), None);
    deadline("session", session.connect(credentials, false)).await?;
    let until = tokio::time::Instant::now() + Duration::from_secs(10);
    while session.account_type().is_none() && tokio::time::Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let tier = match session.account_type().as_deref() {
        Some("free") => "free",
        Some("premium") => "premium",
        Some(_) => "other",
        None => "unknown",
    };
    report["observed_tier"] = json!(tier);
    if tier != options.expected_tier {
        return Err(Failure::new("tier", "expected_tier_not_confirmed"));
    }
    let file = select_file(&session, options, report).await?;
    // Resolve/fetch the matching encrypted prefix before spending the one POST.
    let audio = if candidate.is_some() {
        Some(decode::audio_prefix(&session, file, report).await?)
    } else {
        None
    };
    let base = deadline("endpoint", session.spclient().base_url()).await?;
    let url = endpoint(&base, file)?;
    report["request_host"] = json!(url.host_str());
    let auth = deadline("login5", session.login5().auth_token()).await?;
    let client_token = deadline("client_token", session.spclient().client_token()).await?;
    let timestamp = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Failure::new("clock", "clock_before_unix_epoch"))?
            .as_secs(),
    )
    .map_err(|_| Failure::new("clock", "timestamp_overflow"))?;
    let body = probe::encode(
        &probe::Fields {
            version: 5,
            token,
            cache_id: Some(b""),
            interactivity: 1,
            content_type: 1,
            timestamp,
            field7: None,
        },
        true,
    );
    report["request_body_bytes"] = json!(body.len());
    report["request_timestamp_seconds"] = json!(timestamp);
    let request = Request::builder()
        .method(Method::POST)
        .uri(url.as_str())
        .header(
            header::AUTHORIZATION,
            format!("{} {}", auth.token_type, auth.access_token),
        )
        .header("client-token", client_token)
        .header(header::CONTENT_TYPE, "application/x-protobuf")
        .header(header::CONTENT_LENGTH, body.len())
        .body(Bytes::from(body))
        .map_err(|_| Failure::new("request", "request_construction_failed"))?;

    // Single application-level dispatch. Do NOT use request()/request_body():
    // those discard non-2xx bodies or retry 429. Never call a second diagnostic POST.
    report["playplay_dispatch_calls"] = json!(1);
    let future = session
        .http_client()
        .request_fut(request)
        .map_err(|error| Failure::core("request", error))?;
    let response = tokio::time::timeout(STEP_TIMEOUT, future)
        .await
        .map_err(|_| Failure::new("request", "deadline_exceeded"))?
        .map_err(|_| Failure::new("request", "transport_error"))?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let capture =
        probe::capture_with_deadline(response.into_body(), probe::BODY_LIMIT, BODY_TIMEOUT).await;
    report["response"] = probe::response_summary(status, &headers, &capture);
    report["outcome"] = report["response"]["outcome"].clone();
    if let Some(candidate) = candidate {
        if status != 200 && (200..300).contains(&status) {
            return Err(Failure::new("response_contract", "expected_http_200"));
        }
        if status == 200 && capture.complete {
            let envelope = probe::response_envelope(&capture.bytes)
                .ok_or_else(|| Failure::new("response_contract", "expected_single_16byte_field"))?;
            decode::run(
                file,
                &envelope,
                candidate,
                audio
                    .as_deref()
                    .ok_or_else(|| Failure::new("cdn", "prefix_missing"))?,
                report,
            )
            .await?;
        }
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    // Deliberately no logger, even with RUST_LOG; redact panic payloads as well.
    std::panic::set_hook(Box::new(|_| {
        let _ = std::io::stderr().write_all(b"probe_internal_error\n");
    }));
    let args: Vec<String> = env::args().skip(1).collect();
    if args == ["--decode-child"] {
        return ExitCode::from(decode::decoder_child());
    }
    let options = match options(&args) {
        Ok(Some(options)) => options,
        Ok(None) => return ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.report());
            return ExitCode::from(2);
        }
    };
    // Reserve the file before authentication so a bad/duplicate output path
    // cannot waste a live request. It remains an empty checkpoint on interruption.
    let mut output = match reserve_report(&options.report) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("{}", error.report());
            return ExitCode::from(2);
        }
    };
    let mut report = json!({
        "schema_version": 1,
        "mode": if options.send && options.decode.is_some() { "live_legacy_candidate_attempt" }
                else if options.send { "live_request_only" } else { "offline_profile_check" },
        "decode_requested": options.decode.is_some(),
        "cdn_dispatch_calls": 0,
        "outcome": "not_sent",
        "profile": {"version": 5, "source_dll_version": "1.3.1.234",
                    "dll_sha256": probe::DLL_SHA256, "token_sha256": probe::TOKEN_SHA256,
                    "validated": false},
        "requested_track": options.track,
        "requested_format": format!("{:?}", options.format),
        "expected_tier": options.expected_tier,
        "observed_tier": "not_checked",
        "authentication": {"source": if options.access_token_file.is_some() { "explicit_token_file" } else { "browser_pkce" },
            "client_id": CLIENT_ID, "librespot_client_version": version::SPOTIFY_SEMANTIC_VERSION,
            "librespot_protocol_version": version::SPOTIFY_VERSION, "credentials_cached": false,
            "windows_context_equivalence": "unverified", "client_token_required_by_probe": true},
        "request_route_template": "/playplay/v1/key/{file_id}",
        "playplay_dispatch_calls": 0,
        "response": null,
        "license_validated": false, "decryption_tested": false, "playback_tested": false,
        "tier_restriction_established": false
    });
    let result = async {
        let token = profile(&options)?;
        report["profile"]["validated"] = json!(true);
        let candidate = if let Some((worker, table)) = &options.decode {
            decode::disable_core_dumps()?;
            let candidate = decode::candidate_profile(worker, table)?;
            decode::preflight(&candidate).await?;
            report["candidate_worker_preflight"] = json!("passed_not_compatibility_proof");
            Some(candidate)
        } else {
            None
        };
        if options.send {
            live(&options, &token, candidate.as_ref(), &mut report).await
        } else {
            report["outcome"] = json!("offline_profile_validated_no_network");
            Ok(())
        }
    }
    .await;
    let exit = if let Err(error) = result {
        report["outcome"] = json!("diagnostic_error");
        report["error"] = error.report();
        1
    } else if report["outcome"] == "tested_candidates_did_not_decode" {
        5
    } else if report["response"]["body_complete"] == false {
        4
    } else if report["response"]["status"]
        .as_u64()
        .is_some_and(|s| !(200..300).contains(&s))
    {
        3
    } else {
        0
    };
    match probe::persist_report(&report, &mut output, &mut std::io::stdout().lock()) {
        Ok(true) => {}
        Ok(false) => {
            let _ = std::io::stderr().write_all(b"console_write_failed_report_saved\n");
        }
        Err(code) => {
            let _ = writeln!(std::io::stderr(), "{code}");
            return ExitCode::from(1);
        }
    }
    ExitCode::from(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, future::ready};

    fn test_uri(number: u8) -> SpotifyUri {
        SpotifyUri::Track {
            id: SpotifyId::from_base62(&format!("{number:022}")).unwrap(),
        }
    }

    #[tokio::test]
    async fn cyclic_and_queued_duplicates_do_not_waste_unique_lookup_budget() {
        let ids: Vec<_> = (1..=8).map(test_uri).collect();
        let mut graph: HashMap<_, _> = ids
            .iter()
            .map(|uri| {
                (
                    uri.to_string(),
                    MetadataCandidate {
                        uri: uri.clone(),
                        available: true,
                        requested_file: None,
                        alternatives: vec![],
                    },
                )
            })
            .collect();
        graph.get_mut(&ids[0].to_string()).unwrap().alternatives =
            vec![ids[1].clone(), ids[2].clone(), ids[1].clone()];
        graph.get_mut(&ids[1].to_string()).unwrap().alternatives = vec![
            ids[0].clone(),
            ids[2].clone(),
            ids[3].clone(),
            ids[4].clone(),
            ids[5].clone(),
            ids[6].clone(),
            ids[7].clone(),
        ];
        graph.get_mut(&ids[7].to_string()).unwrap().requested_file = Some(FileId([8; 20]));
        let mut calls = Vec::new();
        let mut report = json!({});
        let selected = select_with(ids[0].clone(), &mut report, |uri| {
            calls.push(uri.to_string());
            ready(Ok(graph.get(&uri.to_string()).unwrap().clone()))
        })
        .await
        .ok()
        .unwrap();
        assert_eq!(selected, FileId([8; 20]));
        assert_eq!(calls.len(), 8);
        assert_eq!(calls.iter().collect::<HashSet<_>>().len(), 8);
        assert_eq!(report["metadata_items_checked"], 8);
    }

    #[tokio::test]
    async fn restricted_and_missing_requested_format_items_are_skipped() {
        let mut report = json!({});
        let selected = select_with(test_uri(1), &mut report, |uri| {
            let number = if uri.to_string() == test_uri(1).to_string() {
                1
            } else if uri.to_string() == test_uri(2).to_string() {
                2
            } else {
                3
            };
            ready(Ok(MetadataCandidate {
                uri,
                available: number != 1,
                requested_file: if number == 2 {
                    None
                } else {
                    Some(FileId([number; 20]))
                },
                alternatives: if number < 3 {
                    vec![test_uri(number + 1)]
                } else {
                    vec![]
                },
            }))
        })
        .await
        .ok()
        .unwrap();
        assert_eq!(selected, FileId([3; 20]));
        assert_eq!(report["metadata_items_checked"], 3);
    }

    #[test]
    fn endpoint_is_https_spotify_only_without_decoration() {
        let file = FileId([0xab; 20]);
        let good = endpoint("https://gew1-spclient.spotify.com:443", file)
            .ok()
            .unwrap();
        assert_eq!(good.path(), format!("/playplay/v1/key/{}", "ab".repeat(20)));
        assert!(good.query().is_none());
        for bad in [
            "http://gew1-spclient.spotify.com",
            "https://spotify.com.evil.invalid",
            "https://user:pass@gew1-spclient.spotify.com",
            "https://gew1-spclient.spotify.com:8443",
            "https://gew1-spclient.spotify.com?product=9",
            "https://gew1-spclient.spotify.com/base",
        ] {
            assert!(endpoint(bad, file).is_err());
        }
    }

    #[test]
    fn cli_requires_explicit_send_and_rejects_unknown_parameters() {
        let args = ["--spotify-dll", "fixture.dll"].map(str::to_owned);
        let parsed = options(&args).ok().flatten().unwrap();
        assert!(!parsed.send);
        assert_eq!(parsed.expected_tier, "free");
        assert!(options(&["--token".into(), "DO_NOT_LEAK".into()]).is_err());
        assert!(
            options(&[
                "--spotify-dll".into(),
                "fixture.dll".into(),
                "--expected-tier".into(),
                "any".into()
            ])
            .is_err()
        );
    }
}
