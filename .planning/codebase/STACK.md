# Technology Stack

**Analysis Date:** 2026-07-31

## Languages

**Primary:**
- Rust 2024 edition, minimum Rust 1.85 - workspace libraries and the `librespot` binary in `src/`, `core/`, `audio/`, `connect/`, `discovery/`, `metadata/`, `oauth/`, `playback/`, and `protocol/`.

**Secondary:**
- Protocol Buffers - Spotify wire/message definitions under `protocol/proto/`, generated during the `protocol` build.
- Bash - developer, release, and cross-build scripts such as `test.sh`, `.github/scripts/bump-versions.sh`, and `contrib/docker-build.sh`.
- YAML and TOML - GitHub Actions, Cargo manifests, toolchain, formatting, and packaging configuration.

## Runtime

**Environment:**
- Native executable/library runtime; asynchronous work uses Tokio.
- Platform audio and discovery services are selected at compile time. The default binary uses system TLS, Rodio/CPAL audio, and pure-Rust libmdns discovery.

**Package Manager:**
- Cargo, with workspace dependency resolution.
- Lockfile: `Cargo.lock` is present and CI uses `--locked`/`--frozen` for reproducible builds.

## Frameworks

**Core:**
- Tokio 1.x - asynchronous networking, timers, synchronization, and process/signal integration.
- Hyper 1.6 and Tokio Tungstenite 0.28 - HTTP and WebSocket transport.
- Protobuf 3.7 - generated Spotify protocol messages and Connect/dealer payloads.

**Testing:**
- Cargo's built-in test harness - unit and integration tests, including `core/tests/connect.rs` and crate-local tests.
- `cargo-hack` - CI feature-matrix checks for packages and TLS/backend combinations.

**Build/Dev:**
- `build.rs` scripts in `core/` and `protocol/` - build-time metadata and protocol generation.
- `vergen-gitcl`/`vergen` - embeds Git/build metadata consumed by `core/src/version.rs`.
- `rustfmt.toml` and workspace Clippy lints - formatting and static-quality policy.
- Cross-compilation support via `Cross.toml`, `contrib/Dockerfile*`, and GitHub Actions.

## Key Dependencies

**Critical:**
- `librespot-core` - session, Spotify transport, login, access-point resolution, caching, encryption, and dealer connectivity.
- `librespot-protocol` - generated and handwritten Spotify protocol types.
- `librespot-audio` - encrypted audio retrieval and stream handling.
- `librespot-connect` - Spotify Connect state, remote commands, queue/context, and device transfer.
- `librespot-metadata` - track, album, artist, playlist, show, episode, image, and lyrics metadata.
- `librespot-oauth` - OAuth authorization-code flow with PKCE.
- `librespot-playback` - decoding, mixer, volume normalization, and selectable audio outputs.

**Infrastructure:**
- `native-tls`/`hyper-tls` or `rustls`/`hyper-rustls` - mutually exclusive HTTPS/WSS TLS choices.
- `libmdns`, `dns-sd`, or `zbus`/Avahi - selectable Spotify Connect discovery implementations.
- `rodio`/`cpal`, GStreamer, ALSA, PulseAudio, JACK, PortAudio, or SDL2 - selectable playback backends.
- `env_logger` and `log` - runtime logging; `sysinfo`, `uuid`, `serde`, and `serde_json` support identity/configuration and wire data.

## Configuration

**Environment:**
- Primary runtime configuration is command-line options parsed in `src/main.rs`; examples include device name, bitrate, cache directory, proxy, backend selection, OAuth enablement, and OAuth callback port.
- A filesystem cache path can contain audio data and an authentication blob; the README recommends restrictive permissions. No checked-in environment file is required or used by the binary.
- Build metadata is supplied through Cargo/build-script environment such as `CARGO_PKG_VERSION` and generated `VERGEN_*` values in `core/src/version.rs`.

**Build:**
- `Cargo.toml` - workspace members, dependency versions, default features, backend feature propagation, Debian packaging, and lints.
- `rust-toolchain.toml` - pinned toolchain configuration.
- `Cross.toml` and `contrib/Dockerfile*` - cross-build/container definitions.
- `rustfmt.toml` - Rust formatting rules.

## Platform Requirements

**Development:**
- Rust toolchain 1.85 or newer plus native development libraries for the chosen audio/TLS/discovery features; `COMPILING.md` documents platform-specific prerequisites.
- Linux may use ALSA/PulseAudio/JACK/GStreamer and Avahi; macOS can use CoreAudio/Bonjour; Windows uses WASAPI through the selected backend.

**Production:**
- Native Linux/macOS/Windows/BSD-style executable or library; Debian packaging is described in `Cargo.toml`.
- Optional systemd service units are provided by `contrib/librespot.service` and `contrib/librespot.user.service`; Docker and Raspberry Pi images/builds are under `contrib/`.

---

*Stack analysis: 2026-07-31*
*Update after major dependency changes*
