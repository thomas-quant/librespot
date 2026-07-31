# Coding Conventions

**Analysis Date:** 2026-07-31

## Naming Patterns

**Files:**
- Rust modules use lowercase `snake_case`, with `mod.rs` used for directory-backed modules such as `playback/src/decoder/mod.rs` and `audio/src/fetch/mod.rs`.
- Workspace crates use descriptive lowercase names and the `librespot-*` package convention in files such as `core/Cargo.toml` and `metadata/Cargo.toml`.

**Functions:**
- Functions and methods use `snake_case`, for example `parse_file_size` and `setup_logging` in `src/main.rs`.
- Constructors and lookups commonly use semantic names such as `new`, `find`, `get`, and `from_*`, visible across `core/src/` and `metadata/src/`.

**Variables:**
- Local variables and fields use `snake_case`; short domain-specific names such as `e`, `id`, and `uri` are common in narrow scopes.
- Constants use `SCREAMING_SNAKE_CASE`, for example `OAUTH_SCOPES` in `src/main.rs` and poison-message constants in `audio/src/fetch/receive.rs`.

**Types:**
- Structs, enums, traits, and type aliases use `UpperCamelCase`, for example `MetadataError` in `metadata/src/error.rs` and `PlayerConfig` in `playback/src/config.rs`.
- Enum variants use `UpperCamelCase`; error enums are typically named `<Domain>Error` and derive `thiserror::Error`, as in `core/src/error.rs` and `metadata/src/error.rs`.

## Code Style

**Formatting:**
- `rustfmt` is the formatter, configured for Rust 2024 and style edition 2024 in `rustfmt.toml`.
- Formatting is checked for the entire workspace with `cargo fmt --all -- --check` in `test.sh`.
- Code favors Rust's trailing-comma/multiline formatting and modern inline format arguments, e.g. `{e}` and `{track_uri}` in `src/main.rs` and `playback/src/player.rs`.

**Linting:**
- Clippy is run through `cargo hack` across feature combinations in `test.sh`, including `librespot-protocol` and the root `librespot` package.
- Workspace lint policy is declared in `Cargo.toml`; `clippy::redundant_closure_for_method_calls` is warned on, with crate lints inheriting `workspace = true`.

## Import Organization

**Order:**
1. External and workspace-crate imports, generally grouped by crate and alphabetized or semantically ordered, as in `src/main.rs`.
2. Standard-library imports in a separate `std::{...}` block.
3. Local module declarations and `use` statements after the external imports when needed, as in `src/main.rs`.

**Path Aliases:**
- No custom path aliases are used. Workspace crates are referenced by their package/library names, such as `librespot_core` and `librespot_playback`; the root crate re-exports them in `src/lib.rs`.
- Conditional imports use `#[cfg(feature = ...)]`, notably the backend-specific import in `src/main.rs`.

## Error Handling

**Patterns:**
- Public/domain errors are typed enums with `#[derive(Debug, Error)]` and user-facing `#[error(...)]` messages, for example `MetadataError` in `metadata/src/error.rs` and `ParseFileSizeError` in `src/main.rs`.
- Fallible operations generally return `Result` and propagate errors with `?`; lower-level errors are wrapped or converted with `#[from]` where appropriate, as in `src/main.rs`.
- Recoverable runtime failures are logged with context and handled through fallback, retry, or shutdown paths, especially in `src/main.rs`, `discovery/src/server.rs`, and `playback/src/player.rs`.
- `unwrap`/`expect` are used for invariants or setup that is considered impossible to fail, often with an explanatory message, such as lock access in `connect/src/state.rs` and `audio/src/fetch/receive.rs`; tests and examples also use `unwrap` for concise setup.

## Logging

**Framework:** `log` facade with `env_logger` initialization in `src/main.rs`.

**Patterns:**
- `error!` reports failed operations or invalid state, `warn!` reports recoverable degradation/configuration issues, `info!` reports lifecycle milestones, and `debug!`/`trace!` provide diagnostics.
- Messages include relevant values using structured Rust format arguments, for example in `src/player_event_handler.rs`, `oauth/src/lib.rs`, and `playback/src/player.rs`.
- Logging is preferred for operational failures that can be handled; errors are still returned when callers need to decide control flow.
- `RUST_LOG`, `--verbose`, and `--quiet` are reconciled in `setup_logging` in `src/main.rs`.

## Comments

**When to Comment:**
- Comments explain non-obvious protocol, platform, feature, or signal-handling behavior, such as the cleanup trap in `test.sh` and feature rationale in `Cargo.toml`.
- Inline comments are occasional and targeted; most code communicates intent through names, typed errors, and control flow.

**JSDoc/TSDoc:**
- Not applicable. Rust documentation comments are present selectively for public APIs; there is no repository-wide requirement for exhaustive API docs.

## Function Design

**Size:** Functions are organized around one operation, but some application orchestration functions are necessarily large, notably the CLI/runtime setup in `src/main.rs` and player state handling in `playback/src/player.rs`.

**Parameters:** Prefer typed domain/configuration structs and enums over untyped option bags; optional behavior is represented with `Option`, feature flags, and configuration types across `core/src/config.rs` and `playback/src/config.rs`.

**Return Values:** Return `Result`/`Option` for expected absence or failure; use domain error types for parse, authentication, metadata, and transport boundaries. Async operations use Tokio futures and return awaited results, as in `core/src/session.rs`.

## Module Design

**Exports:** Crates expose focused public modules from each crate root, while implementation details remain in private submodules. The root facade re-exports workspace crates from `src/lib.rs`.

**Barrel Files:** Rust crate roots and `mod.rs` files serve as explicit module declaration/re-export points, such as `core/src/lib.rs`, `playback/src/lib.rs`, and `playback/src/decoder/mod.rs`; there is no generated barrel system.

---

*Convention analysis: 2026-07-31*
