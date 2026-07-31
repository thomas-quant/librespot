//! Diagnostic spike for free-tier support.
//!
//! Determines, for whatever account you log in with, how far librespot's *legacy* audio
//! path gets before the server refuses. It deliberately changes nothing about how
//! librespot speaks to Spotify — it walks the normal sequence and reports the outcome of
//! each step, so a refusal can be attributed to a specific stage.
//!
//! Steps: authenticate -> read product info -> resolve metadata -> request an audio key
//! per offered format -> resolve a CDN URL.
//!
//! Usage:
//!   cargo run --example free_tier_spike -- [TRACK_BASE62]
//!   cargo run --example free_tier_spike -- [TRACK_BASE62] --token ACCESS_TOKEN

use std::{collections::BTreeMap, env, time::Duration};

use librespot::{
    core::{
        Error, FileId, SpotifyId, SpotifyUri, authentication::Credentials, cdn_url::CdnUrl,
        config::SessionConfig, session::Session,
    },
    metadata::audio::{AudioFileFormat, AudioItem},
    oauth::OAuthClientBuilder,
};

const CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
const REDIRECT_URI: &str = "http://127.0.0.1:8898/login";

/// Rick Astley - Never Gonna Give You Up. Widely available; override if it isn't in your
/// market, or to test a track you expect to be restricted on the free catalogue.
const DEFAULT_TRACK: &str = "4cOdK2wGLETKBW3PvgPWqT";

/// How long to wait for the `ProductInfo` packet, which arrives asynchronously after
/// the session comes up and carries the account tier.
const PRODUCT_INFO_TIMEOUT: Duration = Duration::from_secs(10);

fn step(n: u8, title: &str) {
    println!(
        "\n─── {n}. {title} {}",
        "─".repeat(56_usize.saturating_sub(title.len()))
    );
}

fn describe(e: &Error) -> String {
    format!("{:?} / {e}", e.kind)
}

#[tokio::main]
async fn main() {
    let mut builder = env_logger::Builder::new();
    builder.parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "librespot=info".into()));
    builder.init();

    let args: Vec<String> = env::args().collect();
    let track_arg = args
        .get(1)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| DEFAULT_TRACK.to_string());
    let token_arg = args
        .iter()
        .position(|a| a == "--token")
        .and_then(|i| args.get(i + 1))
        .cloned();

    // ── 1. Authenticate ──────────────────────────────────────────────────────────
    step(1, "Authenticate");

    let credentials = match token_arg {
        Some(token) => {
            println!("Using access token from --token.");
            Credentials::with_access_token(&token)
        }
        None => {
            let client =
                match OAuthClientBuilder::new(CLIENT_ID, REDIRECT_URI, vec!["streaming"]).build() {
                    Ok(c) => c,
                    Err(e) => {
                        println!("FAIL  could not build OAuth client: {e}");
                        return;
                    }
                };
            match client.get_access_token_async().await {
                Ok(token) => {
                    println!("OK    got an access token, scopes: {:?}", token.scopes);
                    Credentials::with_access_token(&token.access_token)
                }
                Err(e) => {
                    println!("FAIL  OAuth failed: {e}");
                    println!("      An account that cannot even obtain a token is blocked well");
                    println!("      before the audio path, so stop here.");
                    return;
                }
            }
        }
    };

    let session = Session::new(SessionConfig::default(), None);
    if let Err(e) = session.connect(credentials, false).await {
        println!("FAIL  session.connect: {}", describe(&e));
        println!("      If this is PermissionDenied / \"Premium account required\", the access");
        println!("      point rejected the login outright and the tier is gated at layer one.");
        return;
    }
    println!("OK    connected as {:?}", session.username());

    // ── 2. Product info ──────────────────────────────────────────────────────────
    step(2, "Product info (account tier)");

    let deadline = tokio::time::Instant::now() + PRODUCT_INFO_TIMEOUT;
    while session.account_type().is_none() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    match session.account_type() {
        Some(tier) => println!("OK    account type: {tier:?}"),
        None => println!(
            "WARN  no ProductInfo within {PRODUCT_INFO_TIMEOUT:?} — tier unknown, continuing"
        ),
    }
    println!("      catalogue: {:?}", session.catalogue());
    println!("      country:   {:?}", session.country());
    println!("      premium:   {}", session.is_premium());

    let attributes: BTreeMap<_, _> = session.user_data().attributes.into_iter().collect();
    if attributes.is_empty() {
        println!("      (no user attributes received)");
    } else {
        println!("\n      full attribute set:");
        for (k, v) in &attributes {
            println!("        {k:<36} {v}");
        }
    }

    // ── 3. Metadata ──────────────────────────────────────────────────────────────
    step(3, "Resolve track metadata");

    let track_id = match SpotifyId::from_base62(&track_arg) {
        Ok(id) => id,
        Err(e) => {
            println!(
                "FAIL  {track_arg:?} is not a valid base62 ID: {}",
                describe(&e)
            );
            return;
        }
    };
    let track_uri = SpotifyUri::Track { id: track_id };

    let audio_item = match AudioItem::get_file(&session, track_uri).await {
        Ok(item) => {
            println!("OK    {:?}", item.name);
            item
        }
        Err(e) => {
            println!("FAIL  metadata lookup: {}", describe(&e));
            println!("      Metadata is not the DRM path — a failure here means something more");
            println!("      basic is wrong (bad ID, region, or session).");
            return;
        }
    };

    match &audio_item.availability {
        Ok(()) => println!("      availability: available on this catalogue"),
        Err(reason) => println!("      availability: RESTRICTED ({reason:?})"),
    }

    if audio_item.files.is_empty() {
        println!("FAIL  no audio files offered for this track on this account.");
        println!("      This is itself a strong signal: the catalogue withheld every format.");
        return;
    }

    let mut offered: Vec<(AudioFileFormat, FileId)> =
        audio_item.files.iter().map(|(f, id)| (*f, *id)).collect();
    offered.sort_by_key(|(f, _)| format!("{f:?}"));

    println!("\n      formats offered ({}):", offered.len());
    for (format, file_id) in &offered {
        println!("        {format:<18?} {file_id}");
    }

    // ── 4. Audio keys ────────────────────────────────────────────────────────────
    step(4, "Request audio key per format");
    println!("This is the question. AesKey = the legacy path serves this tier.");
    println!("AesKeyError = the server refused, and free support needs a path we won't build.\n");

    let mut granted = 0usize;
    for (format, file_id) in &offered {
        match session.audio_key().request(track_id, *file_id).await {
            Ok(_key) => {
                granted += 1;
                println!("  GRANTED  {format:?}");
            }
            Err(e) => println!("  REFUSED  {format:<18?} {}", describe(&e)),
        }
    }

    // ── 5. CDN URL ───────────────────────────────────────────────────────────────
    step(5, "Resolve a CDN URL");

    let (probe_format, probe_file) = offered[0];
    match CdnUrl::new(probe_file).resolve_audio(&session).await {
        Ok(cdn) => match cdn.try_get_urls() {
            Ok(urls) => {
                println!(
                    "OK    {probe_format:?} resolved to {} CDN URL(s)",
                    urls.len()
                );
                if let Some(first) = urls.first() {
                    let shown: String = first.chars().take(80).collect();
                    println!("      {shown}…");
                }
            }
            Err(e) => println!("WARN  resolved but no usable URL: {}", describe(&e)),
        },
        Err(e) => println!(
            "FAIL  storage-resolve for {probe_format:?}: {}",
            describe(&e)
        ),
    }

    // ── Verdict ──────────────────────────────────────────────────────────────────
    step(6, "Verdict");

    let tier = session.account_type().unwrap_or_else(|| "unknown".into());
    println!("account tier:     {tier}");
    println!("formats offered:  {}", offered.len());
    println!("keys granted:     {granted}");

    println!();
    if granted == offered.len() {
        println!("GREEN  Every key was granted. The legacy path serves this tier, and the");
        println!("       remaining work is the tier-correctness patch, not protocol work.");
    } else if granted > 0 {
        println!("AMBER  Keys were granted for some formats but not others. Playback is");
        println!("       possible if the player is constrained to the granted formats.");
    } else {
        println!("RED    No keys granted. The legacy path does not serve this tier.");
        if tier == "premium" {
            println!("       Note this account is premium, so this is a bug in the spike or a");
            println!("       transient failure — not a tier answer. Re-run before concluding.");
        }
    }
}
