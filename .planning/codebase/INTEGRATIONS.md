# External Integrations

**Analysis Date:** 2026-07-31

## APIs & External Services

**Spotify platform services:**
- Spotify access-point resolution - discovers `accesspoint`, `dealer`, and `spclient` hosts through `https://apresolve.spotify.com/`; fallback hosts are defined in `core/src/apresolve.rs`.
  - Integration method: HTTPS via Hyper, with configurable proxy support from `core/src/config.rs`.
  - Auth: Session credentials/access token managed by the core session and login flow.
- Spotify login and client token services - `https://login5.spotify.com/v3/login` and `https://clienttoken.spotify.com/v1/clienttoken`, implemented in `core/src/login5.rs` and `core/src/spclient.rs`.
  - Auth: Device/client identity and session credentials; token values are runtime data and are not documented here.
- Spotify spclient - resolved HTTPS access point used for metadata, playback state, Connect state, playlists, lyrics, audio storage URLs, and context requests in `core/src/spclient.rs` and callers under `metadata/` and `connect/`.
- Spotify Dealer - resolved secure WebSocket endpoint (`wss://...`) used for real-time Connect commands/events by `core/src/dealer/manager.rs` and `connect/src/spirc.rs`.
- Spotify CDN - signed audio URLs returned by spclient, consumed by `core/src/cdn_url.rs` and `audio/src/fetch/`; hosts include `spotifycdn.com`, `scdn.co`, and Akamai-backed Spotify audio domains.
- Spotify image CDN - metadata image URLs are formed in `metadata/src/audio/item.rs`.

**OAuth:**
- Spotify Accounts - authorization and token endpoints `https://accounts.spotify.com/authorize` and `https://accounts.spotify.com/api/token`, configured in `oauth/src/lib.rs`.
  - SDK/Client: `oauth2` 5.x with `reqwest` 0.12.
  - Auth: Authorization-code flow with PKCE; the binary can open a browser and run a local callback listener, or accept a redirect URL manually.
  - Callback: local HTTP listener, normally exposed by the `--oauth-port` option and assembled in `src/main.rs`.

## Data Storage

**Databases:**
- None. The client communicates with Spotify services and does not contain SQL/NoSQL drivers or migrations.

**File Storage:**
- Local filesystem cache - optional cache directory configured by CLI, managed in `core/src/cache.rs` and `core/src/session.rs`.
  - Stores cached audio and authentication material; cache pruning uses a size limiter and filesystem timestamps.
  - No cloud object-storage integration is present.

**Caching:**
- Local disk only; no Redis or remote cache service is present.

## Authentication & Identity

**Auth Provider:**
- Spotify login5/device session flow - implemented by `core/src/login5.rs` and session construction in `core/src/session.rs`.
  - Token storage: in-memory session state, with optional persisted authentication blob in the local cache.
  - Session management: access credentials are refreshed/used by the core transport; logout/credential lifecycle is exposed through the session APIs.

**OAuth Integrations:**
- Spotify Accounts OAuth - PKCE authorization flow in `oauth/src/lib.rs`, used by the CLI in `src/main.rs` and examples such as `examples/play_connect.rs`.

## Monitoring & Observability

**Error Tracking:**
- None detected. There is no Sentry or hosted error-tracking SDK.

**Analytics:**
- None detected.

**Logs:**
- Local stderr/stdout logging through `log` and `env_logger`, initialized by `src/main.rs`; service deployments can collect these through systemd or container logging.

## CI/CD & Deployment

**Hosting:**
- No application hosting service; this is distributed as native binaries/libraries and Rust crates.
- Debian/systemd packaging is configured in `Cargo.toml` and `contrib/`; container/cross-build definitions are in `contrib/`.

**CI Pipeline:**
- GitHub Actions - `.github/workflows/build.yml` runs locked workspace/example builds, tests, and feature checks across toolchains/OSes.
- `.github/workflows/quality.yml` runs `cargo fmt --check` and Clippy feature matrices.
- `.github/workflows/cross-compile.yml` builds target platforms and uploads artifacts.
- `.github/workflows/release.yml` publishes the workspace to crates.io on release creation; `.github/workflows/prepare-release.yml` automates version/changelog pull-request preparation.
- `.github/dependabot.yml` updates GitHub Actions dependencies weekly.

## Environment Configuration

**Development:**
- Required runtime values are supplied through CLI arguments; optional proxy and OAuth callback settings are represented in `SessionConfig` and `src/main.rs`.
- Secrets are not read from checked-in environment files. Credentials are supplied interactively, through OAuth, or via the configured local cache.

**Staging:**
- No repository-defined staging environment or staging service configuration detected.

**Production:**
- Deployment-specific service managers/containers provide command-line arguments, filesystem permissions, proxy settings, and logging capture. `contrib/librespot.service` and `contrib/librespot.user.service` are the reference systemd units.

## Webhooks & Callbacks

**Incoming:**
- Local OAuth callback - HTTP redirect listener on loopback, implemented in `oauth/src/lib.rs`; it parses the authorization code and returns a short HTTP response.
- Spotify Connect discovery - local HTTP/zeroconf interaction implemented in `discovery/src/server.rs`; this is device discovery/control traffic, not a public webhook.

**Outgoing:**
- Dealer WebSocket messages and spclient HTTPS requests - initiated by session/Connect operations; no generic webhook delivery or retry service is present.

---

*Integration audit: 2026-07-31*
*Update when adding/removing external services*
