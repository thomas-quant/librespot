# Architecture

**Analysis Date:** 2026-07-31

## Pattern Overview

**Overall:** Multi-crate Rust library workspace with a thin headless executable, asynchronous actor/event components, and compile-time backend adapters.

**Key Characteristics:**
- `Cargo.toml` defines the public `librespot` library and `librespot` binary while re-exporting the workspace crates through `src/lib.rs`.
- The dependency direction is mostly layered: protocol types at the bottom, core Spotify transport/session services above them, then metadata/audio/playback and Connect/discovery integration.
- Runtime coordination uses Tokio tasks and `mpsc`/`oneshot` channels rather than a central mutable application object; shared state is commonly held behind `Arc`, mutexes, or weak session handles.
- Cargo features select mutually exclusive TLS, discovery, audio-output, decoder, and passthrough implementations at compile time.

## Layers

**Protocol and generated model layer:**
- Purpose: Represent Spotify protobuf messages and expose small hand-written trait extensions.
- Contains: Generated code compiled from `protocol/proto/*.proto`, build generation in `protocol/build.rs`, and adapters in `protocol/src/impl_trait/*.rs`.
- Depends on: `protobuf` only.
- Used by: `core`, `metadata`, and `connect`.

**Core session and transport layer:**
- Purpose: Authenticate, maintain the Spotify session, speak the access-point/dealer/Mercury protocols, resolve metadata/audio endpoints, and provide common IDs, configuration, cache, and errors.
- Contains: `core/src/session.rs`, `core/src/connection/`, `core/src/dealer/`, `core/src/mercury/`, `core/src/spclient.rs`, `core/src/audio_key.rs`, and `core/src/cache.rs`.
- Depends on: Protocol types, OAuth, Tokio networking, HTTP/WebSocket/TLS, cryptography, and serialization.
- Used by: Audio, metadata, playback, discovery, Connect, and the root executable.

**Content and media acquisition layer:**
- Purpose: Turn Spotify metadata and file identifiers into usable domain objects and encrypted audio streams.
- Contains: Metadata entities and request abstractions in `metadata/src/`, plus range-based fetching and decryption in `audio/src/fetch/` and `audio/src/decrypt.rs`.
- Depends on: Core session services and protocol models.
- Used by: Playback and applications embedding the library.

**Playback abstraction layer:**
- Purpose: Decode fetched audio, apply format/dither/volume behavior, and send samples to a selectable output sink.
- Contains: `playback/src/player.rs`, `playback/src/decoder/`, `playback/src/audio_backend/`, `playback/src/mixer/`, and `playback/src/config.rs`.
- Depends on: Audio, core, metadata, and optional platform/backend crates.
- Used by: Connect control and the root binary.

**Control and integration layer:**
- Purpose: Expose Spotify Connect state/control and advertise or discover devices.
- Contains: Connect state machine and SPIRC handling in `connect/src/`, zeroconf server/backends in `discovery/src/`, and OAuth authorization in `oauth/src/lib.rs`.
- Depends on: Core, protocol, playback, and optional platform discovery libraries.
- Used by: The root executable or external applications.

**Application composition layer:**
- Purpose: Parse command-line options, select features/configuration, construct services, connect event channels, and own process shutdown.
- Contains: `src/main.rs` and the event bridge in `src/player_event_handler.rs`.
- Depends on: All workspace crates and CLI/runtime dependencies.
- Used by: The `librespot` process entry point.

## Data Flow

**Headless Spotify Connect playback:**

1. `src/main.rs::main` parses options and `get_setup` builds session, cache, credentials/OAuth, playback, mixer, and discovery configuration.
2. A `librespot_core::Session` authenticates and maintains the encrypted access-point connection; `core/src/session.rs` dispatches packets to channel, dealer, and key-management components.
3. `librespot_discovery` advertises the device and emits Connect events through Tokio channels.
4. `connect/src/spirc.rs` receives Spotify Connect commands, updates state in `connect/src/state.rs`, and sends player/mixer commands.
5. `playback/src/player.rs` requests the selected track, uses metadata/audio services to obtain and decrypt the stream, decodes it, and writes frames to a `Sink` selected by `playback/src/audio_backend/mod.rs`.
6. Player events flow back through `PlayerEventChannel` to Connect and optionally `src/player_event_handler.rs`; shutdown propagates through command channels and task handles.

**Core request/response or subscription flow:**

1. A caller constructs a request or subscription through a public core/metadata API.
2. `core/src/mercury/` or `core/src/dealer/` allocates a request ID/channel and registers a callback or reply sender.
3. A Tokio task serializes the protocol message and sends it over the session connection.
4. The session dispatch loop routes the response by packet/topic/request key to the registered handler.
5. The handler resolves a `oneshot` result or pushes events to an `mpsc` receiver; domain crates deserialize into metadata/audio/control types.

**State Management:**
- Session lifetime is shared through `Session(Arc<SessionInternal>)`; background tasks own weak or cloned handles and communicate through channels.
- Playback, discovery, dealer, and audio fetching each encapsulate a long-lived task or worker with explicit command/event channels.
- Optional filesystem cache state is implemented in `core/src/cache.rs`; no database layer is present.

## Key Abstractions

**Session:**
- Purpose: Stable handle for authentication, encrypted transport, packet dispatch, cache access, and shared Spotify services.
- Examples: `core/src/session.rs`, re-exported as `librespot_core::Session`.
- Pattern: `Arc`-backed façade with internal asynchronous tasks and lazy components.

**Channel/dealer/Mercury transports:**
- Purpose: Multiplex streaming, request/reply, and subscription traffic over the Spotify session.
- Examples: `core/src/channel.rs`, `core/src/dealer/mod.rs`, `core/src/mercury/mod.rs`.
- Pattern: ID-routed message dispatch backed by Tokio channels and futures/streams.

**Backend traits:**
- Purpose: Keep playback independent of a concrete audio device or decoder.
- Examples: `Sink`, `Open`, and `SinkAsBytes` in `playback/src/audio_backend/mod.rs`; `AudioDecoder` in `playback/src/decoder/mod.rs`; `Mixer` in `playback/src/mixer/mod.rs`.
- Pattern: Trait objects (`Arc<dyn Mixer>`) plus feature-gated concrete adapters and builder functions.

**Domain metadata and request traits:**
- Purpose: Provide typed track, album, artist, playlist, show, episode, image, and audio-file models without exposing raw protocol handling to callers.
- Examples: `metadata/src/lib.rs`, `metadata/src/request.rs`, and `metadata/src/track.rs`.
- Pattern: Public typed structs/enums, protocol re-exports, and `Metadata`/`MercuryRequest` traits.

**Configuration structs:**
- Purpose: Make composition and platform choices explicit at boundaries.
- Examples: `core/src/config.rs`, `playback/src/config.rs`, `connect/src/state.rs`, and `discovery/src/server.rs`.
- Pattern: Builder/default-oriented values passed into constructors rather than global configuration.

## Entry Points

**Library entry:**
- Location: `src/lib.rs`.
- Triggers: Rust applications depend on the `librespot` crate.
- Responsibilities: Re-export `audio`, `connect`, `core`, `discovery`, `metadata`, `oauth`, `playback`, and `protocol` crates.

**Binary entry:**
- Location: `src/main.rs::main`.
- Triggers: CLI invocation or installed service definitions in `contrib/librespot.service` and `contrib/librespot.user.service`.
- Responsibilities: Parse options, configure logging, choose credentials/backends, construct services, run the Tokio runtime, and coordinate shutdown.

**Reusable examples:**
- Locations: `examples/play.rs`, `examples/play_connect.rs`, `examples/get_token.rs`, `examples/playlist_tracks.rs`, and `oauth/examples/`.
- Triggers: `cargo run --example ...`.
- Responsibilities: Demonstrate embedding individual library layers without the root CLI composition.

## Constraints and Anti-Patterns

- The workspace targets Rust 1.85 and edition 2024 as declared in `Cargo.toml`; new APIs should preserve the public crate boundaries and feature forwarding convention.
- TLS features are intended to be mutually exclusive and are validated in `oauth/src/lib.rs`; adding a new network dependency without propagating all TLS variants can produce confusing feature builds.
- Audio and discovery implementations must remain feature-gated: platform-specific code belongs under the corresponding modules in `playback/src/audio_backend/` or `discovery/src/`, not in generic composition code.
- Protocol sources in `protocol/proto/` are generated inputs. Do not hand-edit generated output; update `.proto` files and `protocol/build.rs` instead.
- Avoid blocking work inside Tokio tasks. The architecture deliberately separates asynchronous network/control tasks from decoder/output work, with synchronous platform APIs isolated in backend adapters.
- Avoid bypassing the session routing abstractions with ad-hoc sockets or global state; request correlation and task shutdown depend on `core/src/channel.rs`, `core/src/dealer/`, and `core/src/mercury/`.
- Unbounded channels are used in several high-throughput paths (`core/src/session.rs`, `core/src/dealer/`, `audio/src/fetch/`, and `playback/src/player.rs`); new producers should consider lifecycle, cancellation, and backpressure explicitly.

## Error Handling

**Strategy:** Typed, crate-local error enums bubble through `Result`; cross-crate boundaries commonly convert into `core::Error` or a domain-specific error, while the executable logs fatal setup/runtime failures and exits.

**Patterns:**
- `thiserror` derives descriptive errors across `core/src/error.rs`, `audio/src/fetch/mod.rs`, `playback/src/decoder/mod.rs`, `connect/src/spirc.rs`, and sibling modules.
- Channel closure, task cancellation, and failed request receivers are represented as explicit error variants rather than silently ignored in transport/media code.
- Network/protocol errors are classified at the layer that understands them, then propagated through session, playback, or Connect operations.
- User-facing parsing errors such as `ParseFileSizeError` are defined near the CLI boundary in `src/main.rs`.

## Cross-Cutting Concerns

**Concurrency:** Tokio tasks, `mpsc` command/event channels, `oneshot` request completion, `Arc` shared ownership, and explicit shutdown senders are pervasive (`core/src/session.rs`, `playback/src/player.rs`, `discovery/src/lib.rs`).

**Logging:** The `log` facade is used throughout crates; `src/main.rs::setup_logging` initializes `env_logger` and honors `RUST_LOG`.

**Authentication and security:** Core session authentication is in `core/src/authentication.rs`/`core/src/login5.rs`; browser authorization with PKCE is in `oauth/src/lib.rs`; encrypted audio key and stream handling are in `core/src/audio_key.rs` and `audio/src/decrypt.rs`.

**Serialization and compatibility:** Protobuf generation and JSON/serde conversions bridge Spotify wire formats to typed Rust models; protocol compatibility is concentrated in `protocol/` and core transport modules.

**Observability and operations:** Version/build metadata is generated by `core/build.rs` and exposed through `core/src/version.rs`; service/container packaging lives under `contrib/`.

*Architecture analysis: 2026-07-31*
*Update when major patterns change*
