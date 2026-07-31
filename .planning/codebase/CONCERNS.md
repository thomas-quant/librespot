# Codebase Concerns

**Analysis Date:** 2026-07-31

## Tech Debt

**Large cross-crate feature matrix:**
- Issue: The root package fans TLS, decoder, audio-backend, and discovery choices into several workspace crates through feature forwarding in `Cargo.toml` and the individual `*/Cargo.toml` manifests.
- Why: Librespot supports many deployment targets and system audio stacks from one workspace.
- Impact: Build and test behavior varies substantially by feature combination; the default CI checks do not visibly cover every backend combination, so regressions can remain hidden until downstream packaging.
- Fix approach: Maintain an explicit supported-feature matrix, compile-test each supported backend in CI, and keep feature propagation documented beside the owning crate.

**Generated protocol surface and legacy compatibility:**
- Issue: `protocol/build.rs` generates a large protobuf API at build time and still contains a TODO to remove legacy protobufs once the newer API is complete.
- Why: Spotify protocol compatibility has evolved incrementally.
- Impact: Generated-code changes can be difficult to review, and obsolete message definitions increase compile and maintenance cost.
- Fix approach: Track generated inputs and legacy call sites separately, remove legacy schemas only after runtime coverage demonstrates they are unused.

**Process-wide mutable environment setup:**
- Issue: `src/main.rs` sets `RUST_BACKTRACE` at runtime using a global semaphore and an unsafe environment mutation.
- Why: The application wants full backtraces by default while avoiding concurrent environment writes.
- Impact: Library consumers embedding the binary logic may get surprising process-wide environment changes; this also increases complexity around initialization order.
- Fix approach: Configure backtraces at process launch or make the behavior an explicit CLI/configuration choice rather than mutating global process state.

## Known Bugs

**No automatic recovery after keepalive loss:**
- Symptoms: A session is shut down and returns a timeout error when the keepalive sequence is missed; playback does not automatically reconnect.
- Trigger: Spotify connectivity loss or a missed Ping/PongAck deadline in `core/src/session.rs`.
- Workaround: Restart or externally reconnect the process.
- Root cause: The code explicitly leaves reconnect as a TODO in `core/src/session.rs`.

**Unsupported local-file sample rates:**
- Symptoms: Local files whose sample rate is not 44,100 Hz are rejected instead of being resampled or played at a matching sink rate.
- Trigger: A local file decoded by `playback/src/decoder/symphonia_decoder.rs` with any other sample rate.
- Workaround: Convert the file to 44.1 kHz before playback.
- Root cause: The decoder checks against the fixed `SAMPLE_RATE` and documents resampling/sink re-opening as unfinished work.

**Unsupported format paths terminate through `unimplemented!`:**
- Symptoms: Selecting an unsupported output format can panic rather than return a recoverable sink error.
- Trigger: Unsupported format selection in `playback/src/audio_backend/sdl.rs` or `playback/src/audio_backend/portaudio.rs`.
- Workaround: Use one of the formats handled by the selected backend.
- Root cause: Both backend implementations call `unimplemented!` in their format fallback branches.

## Security Considerations

**Credentials persisted in the cache directory:**
- Risk: Spotify credentials are stored as `credentials.json`; a permissive filesystem or backup can expose account material.
- Current mitigation: `core/src/cache.rs` warns on Unix when the credential file is world-readable, and `src/main.rs` masks common credential command-line/environment values in trace logging.
- Recommendations: Enforce restrictive permissions when creating/writing the file, avoid storing more credential material than required, and document secure cache-directory ownership for service deployments.

**Sensitive values can still enter diagnostic output through configuration expansion:**
- Risk: The masking in `src/main.rs` is name-based. New sensitive options or aliases can be added without being included in the masking lists.
- Current mitigation: Known username, password, and access-token option/environment names are replaced with `XXXXXXXX`.
- Recommendations: Centralize secret classification and redaction, add tests for every credential input path, and treat proxy/auth headers and future token fields as sensitive by default.

**Network trust and proxy configuration are broad attack surfaces:**
- Risk: `core/src/http_client.rs` builds HTTPS clients with either native TLS or rustls and optionally intercepts all traffic through a configured proxy; incorrect deployment trust stores or a compromised proxy can expose traffic.
- Current mitigation: TLS backends and CA-root choices are explicit Cargo features, and HTTP status failures are surfaced.
- Recommendations: Document proxy trust assumptions, test both native and rustls roots, and ensure callers cannot silently disable certificate verification when adding transport options.

## Performance Bottlenecks

**Audio cache directory scan and eviction are synchronous:**
- Problem: Cache initialization recursively scans the audio directory and eviction removes files synchronously.
- Measurement: No repository benchmark or latency target was found.
- Cause: `core/src/cache.rs` performs recursive filesystem traversal and `remove_file` operations while constructing/pruning the cache; the in-memory priority queue also keeps metadata for every discovered file.
- Improvement path: Bound scan/eviction work, move maintenance off latency-sensitive startup paths, handle concurrent writers robustly, and add benchmarks for large caches.

**Streaming fetch holds coordination locks across scheduling work:**
- Problem: Audio reads and range scheduling contend on a shared download-status mutex and condition variable.
- Measurement: No throughput or lock-contention measurements were found.
- Cause: `audio/src/fetch/mod.rs` computes missing ranges while holding `download_status`, then coordinates spawned CDN streamers; `audio/src/fetch/receive.rs` has multiple poisoned-lock `expect` paths.
- Improvement path: Minimize lock scope around range-set operations, measure concurrent seek/playback behavior, and add stress tests for overlapping ranges and cancellation.

**HTTP rate limiter can delay bursts globally per key:**
- Problem: Requests may wait up to the configured rate-limit behavior and retry indefinitely when the service returns a retryable 429 with `Retry-After`.
- Measurement: Constants in `core/src/http_client.rs` allow 300 calls per 30 seconds and a nominal maximum wait of 10 seconds; no production latency telemetry is present.
- Cause: The keyed limiter and retry loop are centralized but have no attempt/deadline budget in `HttpClient::request`.
- Improvement path: Add bounded retries/deadlines, expose metrics, and distinguish idempotent requests from operations that should not be replayed.

## Fragile Areas

**Session and Connect state machines:**
- Why fragile: `core/src/session.rs` and `connect/src/state.rs` rely on invariants enforced by `expect`, shared locks, asynchronous channels, and ordering between keepalive, transfer, and player-state events.
- Common failures: A poisoned lock or violated precondition becomes a process panic; a missed event can leave the session disconnected without reconnection.
- Safe modification: Preserve state-transition ordering, test disconnect/reconnect and transfer races, and replace externally reachable invariant failures with typed errors where practical.
- Test coverage: The repository has a small integration test in `core/tests/connect.rs`; no comprehensive session/Connect state-machine suite was found.

**Optional audio backends:**
- Why fragile: `playback/src/audio_backend/` contains backend-specific FFI/system-library assumptions and many direct `unwrap`/`expect` calls.
- Common failures: Missing devices/libraries, unsupported formats, or backend initialization failures panic instead of producing a normal `SinkError`.
- Safe modification: Compile and run backend-specific smoke tests on representative systems, return errors from `Open`, and keep feature guards synchronized with the root manifest.
- Test coverage: No backend-specific tests were found in the repository.

**Cache size accounting:**
- Why fragile: `core/src/cache.rs` maintains separate priority-queue and size-map state while files can change externally; symlink traversal is treated as a directory case during initialization.
- Common failures: Stale sizes, failed deletion, or external file changes can make accounting diverge from actual disk usage; unusual filesystem links can cause unexpected traversal behavior.
- Safe modification: Define symlink policy, rescan or reconcile metadata, make eviction tolerant of concurrent deletion, and test corrupted/incomplete cache contents.
- Test coverage: Unit tests cover `SizeLimiter` basics, but filesystem failure, concurrent mutation, and large-directory behavior are not covered.

## Scaling Limits

**Per-process cache metadata:**
- Current capacity: Limited by memory and filesystem traversal; every discovered cache file is represented in `SizeLimiter.queue` and `SizeLimiter.sizes`.
- Limit: Very large audio caches increase startup time and resident metadata before playback begins.
- Symptoms at limit: Slow startup, eviction churn, and possible memory pressure.
- Scaling path: Use bounded metadata/indexing, shard cache directories, and perform incremental/background reconciliation.

**Single-process playback and connection ownership:**
- Current capacity: The binary is designed around one process-wide session/player and a current-thread Tokio main runtime in `src/main.rs`.
- Limit: Multiple independent Spotify sessions or high concurrency require embedding/architectural work rather than simply adding requests.
- Symptoms at limit: Shared global configuration, locks, and event loops become contention and lifecycle coupling points.
- Scaling path: Separate session/player instances behind explicit runtime ownership and isolate per-session caches, rate limits, and event dispatch.

## Dependencies at Risk

**System audio and discovery dependencies:**
- Risk: Optional crates such as `alsa`, `jack`, `gstreamer`, `portaudio-rs`, `sdl2`, `zbus`, and `dns-sd` depend on platform libraries and daemon versions outside Cargo's lockfile.
- Impact: A locked Rust dependency graph does not guarantee reproducible builds or runtime behavior for all feature combinations.
- Migration plan: Maintain container/VM build fixtures per backend, pin/document minimum system-library versions, and prefer graceful runtime errors for unavailable devices.

**Spotify private/proprietary protocol endpoints:**
- Risk: `core/src/spclient.rs` and the generated schemas in `protocol/proto/` depend on undocumented or evolving Spotify behavior; the source contains a TODO for seen-in-the-wild but unimplemented endpoints.
- Impact: Server-side changes can break login, metadata, playback, or Connect behavior without a stable upstream contract.
- Migration plan: Add protocol compatibility fixtures and observability around unknown messages/endpoints, then isolate endpoint-specific parsing from core state transitions.

## Missing Critical Features

**Robust reconnect and credential/session recovery:**
- Problem: Keepalive loss currently shuts down the session, while reconnect with cached/last credentials remains a TODO in `core/src/session.rs`.
- Current workaround: Supervisors restart or reconnect the application externally.
- Blocks: Reliable unattended receiver operation through transient network/server outages.
- Implementation complexity: High; requires lifecycle, backoff, token validity, playback resumption, and Connect-state coordination.

**Sample-rate conversion for local playback:**
- Problem: `playback/src/decoder/symphonia_decoder.rs` only accepts 44.1 kHz input.
- Current workaround: Pre-convert local files.
- Blocks: Transparent playback of common local media at 48 kHz and other rates.
- Implementation complexity: Medium to high; requires resampling quality/latency decisions and sink reconfiguration or a fixed output conversion stage.

## Test Coverage Gaps

**Transport, retry, and authentication failure paths:**
- What's not tested: HTTP retry-after behavior, proxy/TLS feature variants, CDN token expiry, and the TODO path for refreshing `cdn_url` in `audio/src/fetch/receive.rs`.
- Risk: Transient service failures can cause hangs, stale URLs, or silent playback interruptions.
- Priority: High
- Difficulty to test: Requires deterministic mock HTTP/CDN servers and feature-matrix test jobs.

**Playback and backend integration:**
- What's not tested: Decoder-to-sink behavior, local-file edge cases, device loss, unsupported formats, and backend initialization across `playback/src/audio_backend/`.
- Risk: Audio regressions compile successfully but fail only on user hardware.
- Priority: High
- Difficulty to test: Requires representative audio fixtures and platform/system audio environments; virtual/null sinks would reduce the burden.

**Connect/session concurrency:**
- What's not tested: Transfer races, shutdown ordering, keepalive timeouts, reconnect, and poisoned/shared-state recovery in `core/src/session.rs` and `connect/src/`.
- Risk: Rare event-order bugs can strand playback or panic the process.
- Priority: High
- Difficulty to test: Needs deterministic async scheduling, protocol fixtures, and simulated connection failures.

**Feature-combination coverage:**
- What's not tested: Every supported pairing of TLS roots, discovery backend, decoder, and output backend; `test.sh` performs selected `cargo hack` checks but does not provide runtime tests for all combinations.
- Risk: Feature forwarding or conditional compilation breaks downstream builds unnoticed.
- Priority: Medium
- Difficulty to test: Matrix size and platform-native dependencies make CI expensive; a representative tiered matrix is needed.

---

*Concerns audit: 2026-07-31*
*Update as issues are fixed or new ones discovered*
