# Codebase Structure

**Analysis Date:** 2026-07-31

## Directory Layout

```
librespot/
├── src/                 # Root library facade, CLI binary, and CLI event handler
├── core/                # Session, authentication, transport, cache, and Spotify services
├── protocol/            # Protobuf sources, generated bindings, and protocol traits
├── metadata/            # Typed Spotify metadata and playlist models
├── audio/               # Audio file fetching, range loading, and decryption
├── playback/            # Player, decoders, mixers, and output sink adapters
├── connect/             # Spotify Connect state and SPIRC control
├── discovery/           # Zeroconf/mDNS device advertisement and discovery
├── oauth/               # OAuth authorization-code/PKCE client
├── examples/            # Root crate embedding examples
├── docs/                # Protocol and user/developer notes
├── contrib/             # Containers, service units, cross-build scripts, and hooks
├── cache/               # Runtime cache directory placeholder and ignore rules
├── .devcontainer/       # Development container configuration
├── Cargo.toml           # Workspace/root package, features, dependencies, packaging
├── Cargo.lock           # Locked dependency graph
├── build.rs files       # Build metadata/protobuf generation in crate directories
└── README.md            # Project overview and user-facing configuration
```

## Directory Purposes

**`src/`:**
- Purpose: Root package facade and the production headless executable.
- Contains: `src/lib.rs`, `src/main.rs`, and `src/player_event_handler.rs`.
- Key files: `src/main.rs` composes all crates; `src/lib.rs` re-exports the public workspace crates.
- Subdirectories: None.

**`core/`:**
- Purpose: Shared Spotify protocol/session foundation.
- Contains: Authentication, HTTP/TLS, access-point connection, dealer and Mercury messaging, cache, IDs, tokens, config, and errors.
- Key files: `core/src/lib.rs`, `core/src/session.rs`, `core/src/config.rs`, `core/src/connection/`, `core/src/dealer/`, `core/src/mercury/`, `core/src/cache.rs`.
- Subdirectories: `core/src/connection/`, `core/src/dealer/`, `core/src/mercury/`, and `core/tests/`.

**`protocol/`:**
- Purpose: Wire-format definitions and generated protobuf bindings.
- Contains: `.proto` inputs under `protocol/proto/`, generation in `protocol/build.rs`, and Rust exports in `protocol/src/`.
- Key files: `protocol/Cargo.toml`, `protocol/build.rs`, `protocol/src/lib.rs`, `protocol/src/impl_trait.rs`.
- Subdirectories: `protocol/proto/` and `protocol/src/impl_trait/`.

**`metadata/`:**
- Purpose: Typed metadata API over core/protocol requests.
- Contains: Track, album, artist, playlist, show, episode, lyrics, image, availability, restriction, and audio-file models.
- Key files: `metadata/src/lib.rs`, `metadata/src/request.rs`, `metadata/src/track.rs`, `metadata/src/playlist/`.
- Subdirectories: `metadata/src/audio/` and `metadata/src/playlist/`.

**`audio/`:**
- Purpose: Retrieve encrypted Spotify audio and expose decrypted stream/file abstractions.
- Contains: AES/CTR decryption, byte range tracking, asynchronous fetch/receive workers, temporary-file handling, and error types.
- Key files: `audio/src/lib.rs`, `audio/src/decrypt.rs`, `audio/src/fetch/mod.rs`, `audio/src/fetch/receive.rs`, `audio/src/range_set.rs`.
- Subdirectories: `audio/src/fetch/`.

**`playback/`:**
- Purpose: Decode audio and send it to an output device or process.
- Contains: Player orchestration, format/configuration, Symphonia decoder, optional passthrough decoder, dither, mixer, local-file support, and backend adapters.
- Key files: `playback/src/lib.rs`, `playback/src/player.rs`, `playback/src/decoder/mod.rs`, `playback/src/audio_backend/mod.rs`, `playback/src/mixer/mod.rs`.
- Subdirectories: `playback/src/audio_backend/`, `playback/src/decoder/`, and `playback/src/mixer/`.

**`connect/`:**
- Purpose: Spotify Connect command/state integration.
- Contains: SPIRC protocol handling, Connect configuration/state, track/context/transfer/shuffle helpers, and metadata resolution.
- Key files: `connect/src/lib.rs`, `connect/src/spirc.rs`, `connect/src/state.rs`, `connect/src/model.rs`.
- Subdirectories: `connect/src/state/`.

**`discovery/`:**
- Purpose: Advertise the player and handle device discovery using selectable zeroconf implementations.
- Contains: Common server/event API plus feature-gated libmdns, Avahi, or DNS-SD support.
- Key files: `discovery/src/lib.rs`, `discovery/src/server.rs`, `discovery/src/avahi.rs`.
- Subdirectories: `discovery/examples/`.

**`oauth/`:**
- Purpose: Obtain Spotify access tokens through OAuth authorization code with PKCE.
- Contains: Sync/async client support, redirect listener/stdin flows, TLS feature checks, and examples.
- Key files: `oauth/src/lib.rs`, `oauth/examples/oauth_async.rs`, `oauth/examples/oauth_sync.rs`.
- Subdirectories: `oauth/examples/`.

**`examples/`, `docs/`, and `contrib/`:**
- `examples/` demonstrates library embedding (`examples/play.rs`, `examples/play_connect.rs`, `examples/get_token.rs`, `examples/playlist_tracks.rs`).
- `docs/` documents authentication, dealer, and connection behavior (`docs/authentication.md`, `docs/dealer.md`, `docs/connection.md`).
- `contrib/` contains packaging/deployment artifacts, Dockerfiles, cross-compilation scripts, service units, and event hooks.

## Key File Locations

**Entry Points:**
- `src/main.rs` - root CLI/binary entry and runtime composition.
- `src/lib.rs` - root library facade and crate re-exports.
- `examples/*.rs` - standalone embedding examples.
- `discovery/examples/*.rs` and `oauth/examples/*.rs` - crate-specific examples.

**Configuration:**
- `Cargo.toml` - workspace members/dependencies, public features, package metadata, and backend selection.
- `*/Cargo.toml` - per-crate dependencies and feature forwarding.
- `rust-toolchain.toml` - toolchain selection.
- `rustfmt.toml` - formatting configuration.
- `Cross.toml` - cross-compilation configuration.
- `contrib/librespot.service` and `contrib/librespot.user.service` - service execution defaults.

**Core Logic:**
- `core/src/session.rs` - shared authenticated session and packet dispatch.
- `core/src/connection/` - access-point handshake and codec.
- `core/src/dealer/` and `core/src/mercury/` - request/event transport abstractions.
- `metadata/src/` - domain models and typed metadata requests.
- `audio/src/` - encrypted audio acquisition/decryption.
- `playback/src/player.rs` - playback lifecycle and commands.
- `connect/src/spirc.rs` and `connect/src/state.rs` - Connect control/state.
- `discovery/src/lib.rs` and `discovery/src/server.rs` - zeroconf service lifecycle.

**Testing:**
- `core/tests/connect.rs` - integration coverage for core connection behavior.
- Inline `#[cfg(test)]` modules in files such as `core/src/cache.rs`, `core/src/spotify_id.rs`, `core/src/spotify_uri.rs`, `connect/src/shuffle_vec.rs`, and `oauth/src/lib.rs`.
- `test.sh` - repository test helper.

**Documentation:**
- `README.md` - primary usage/build/configuration guide.
- `COMPILING.md` - build and platform/backend guidance.
- `CONTRIBUTING.md` - contributor workflow.
- `SECURITY.md` - security reporting guidance.
- `docs/*.md` - protocol and integration notes.

## Naming Conventions

**Files:**
- Rust modules use lowercase snake case, e.g. `core/src/http_client.rs`, `playback/src/audio_backend.rs`, and `connect/src/context_resolver.rs`.
- Directory modules use `mod.rs` when the module has children, e.g. `core/src/dealer/mod.rs` and `metadata/src/playlist/mod.rs`.
- Important repository documents use uppercase names such as `README.md`, `Cargo.toml`, `COMPILING.md`, and `SECURITY.md`.
- Tests are usually inline `#[cfg(test)]` modules or under a crate `tests/` directory; examples are named by scenario.

**Directories:**
- Workspace crates are short lowercase domain names: `core`, `audio`, `playback`, `connect`, `discovery`, `metadata`, `oauth`, and `protocol`.
- Child directories group a module's implementation variants or domain subtypes, such as `playback/src/audio_backend/` and `metadata/src/playlist/`.

**Special Patterns:**
- `lib.rs` is each crate's public module/export boundary.
- Feature-specific implementations are named after the backend and conditionally compiled, e.g. `playback/src/audio_backend/alsa.rs` and `discovery/src/avahi.rs`.
- Generated protocol material is derived from `protocol/proto/*.proto`; build scripts are named `build.rs`.

## Where to Add New Code

**New Feature:**
- Primary code: the narrowest existing workspace crate (`core/src/`, `metadata/src/`, `audio/src/`, `playback/src/`, `connect/src/`, `discovery/src/`, or `oauth/src/`).
- Public exports: the relevant crate `src/lib.rs`, then `src/lib.rs` at the root only if the root facade should expose it.
- Tests: inline beside the implementation for focused logic, or the crate's `tests/` directory for integration behavior.
- Config/features: the owning crate's `Cargo.toml` and workspace feature forwarding in root `Cargo.toml` when the feature crosses crate boundaries.

**New Component/Module:**
- Implementation: add a snake_case module under the owning crate's `src/`, with a `mod.rs` only when it owns child modules.
- Types: keep domain types near their module; reuse protocol definitions through `protocol/src/` rather than duplicating wire structs.
- Tests: colocate unit tests or add `crate/tests/` integration tests.

**New Backend:**
- Implementation: add a feature-gated adapter under `playback/src/audio_backend/` or `discovery/src/`.
- Registration: update the corresponding `mod.rs`, builder/selector, crate feature list, and root feature forwarding in `Cargo.toml`.
- Documentation/build dependencies: update `README.md`, `COMPILING.md`, and platform packaging only as needed.

**New Protocol Message:**
- Definition: add or update the appropriate file under `protocol/proto/`.
- Generated exposure: adjust `protocol/build.rs` or `protocol/src/lib.rs` only when the generated namespace/export requires it.
- Domain conversion: add typed wrappers in `metadata/src/` or another consuming crate rather than embedding application logic in generated code.

## Special Directories

**`protocol/proto/`:**
- Purpose: Source schemas for generated protobuf bindings.
- Source: Maintained `.proto` files compiled by `protocol/build.rs`.
- Committed: Yes; generated build output is produced during compilation.

**`cache/`:**
- Purpose: Runtime cache location placeholder.
- Source: Runtime behavior in `core/src/cache.rs`.
- Committed: Only its ignore rule is intended to be tracked; runtime contents should not be committed.

**`target/` (when present):**
- Purpose: Cargo build artifacts and generated output.
- Source: Cargo/build scripts such as `core/build.rs` and `protocol/build.rs`.
- Committed: No; treat as disposable build output.

**`.planning/codebase/`:**
- Purpose: Repository analysis documents used by planning workflows.
- Source: Mapper workflow/templates.
- Committed: Workflow-dependent; this task writes `ARCHITECTURE.md` and `STRUCTURE.md` only and does not commit changes.

*Structure analysis: 2026-07-31*
*Update when directory structure changes*
