# Testing Patterns

**Analysis Date:** 2026-07-31

## Test Framework

**Runner:**
- Cargo's built-in Rust test runner, invoked as `cargo test --workspace` in `test.sh`.
- Async tests use Tokio's `#[tokio::test]` attribute, as in `core/tests/connect.rs`.
- No separate test configuration file or third-party test runner is present in the repository.

**Assertion Library:**
- Standard Rust macros such as `assert!`, `assert_eq!`, and `panic!`; examples are in `oauth/src/lib.rs` and `core/tests/connect.rs`.

**Run Commands:**
```bash
cargo test --workspace                         # Run all workspace tests
cargo test -p librespot-core                    # Run one crate's tests
cargo test -p librespot-oauth                   # Run embedded OAuth unit tests
./test.sh                                       # Full CI-style format, lint, build, check, and test sequence
```

There is no watch-mode or coverage command configured. `test.sh` also runs `cargo fmt`, feature-matrix Clippy/checks, locked/frozen builds, and example compilation.

## Test File Organization

**Location:**
- Most tests are co-located in implementation modules under `#[cfg(test)]`, including `core/src/spotify_id.rs`, `core/src/spotify_uri.rs`, `connect/src/shuffle_vec.rs`, `oauth/src/lib.rs`, and `src/main.rs`.
- The repository also uses a separate integration-test directory for crate-level behavior: `core/tests/connect.rs`.

**Naming:**
- Test functions use descriptive `snake_case`, often with a `test_` prefix in module tests and `test_connection` in `core/tests/connect.rs`.
- There are no dedicated fixture filenames, snapshot files, or test utility directories.

**Structure:**
```text
<crate>/src/<module>.rs
  #[cfg(test)]
  mod tests { ... }

<crate>/tests/<integration>.rs
  #[tokio::test]
  async fn test_connection() { ... }
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn rejects_invalid_input() {
        assert_eq!(get_socket_address("https://127.0.0.1/foo"), None);
    }
}
```

This pattern is visible in the test module at the end of `oauth/src/lib.rs`; domain-focused unit modules elsewhere use the same built-in attributes.

**Patterns:**
- Setup is generally inline in each test using concrete values, defaults, and constructors; there is no shared setup/teardown harness.
- Assertions check exact values for pure helpers, while integration tests assert the externally relevant error/result condition.
- Teardown is implicit through Rust ownership and Tokio test completion; no explicit teardown hooks are used.

## Mocking

**Framework:** None detected. The repository has no mock library or mock server dependency in the workspace manifests such as `Cargo.toml` and `core/Cargo.toml`.

**Patterns:**
```rust
let result = Session::new(SessionConfig::default(), None)
    .connect(Credentials::with_password("test", "test"), false)
    .await;
```

`core/tests/connect.rs` exercises the real session/authentication path with deliberately invalid test credentials rather than mocking the network boundary.

**What to Mock:**
- No established mocking guidance exists. New tests should prefer pure/domain-level seams where available and avoid introducing mocks solely to test trivial data transformations.

**What NOT to Mock:**
- Existing tests do not mock protocol, session, or filesystem behavior; the integration test intentionally reaches the real connection path in `core/tests/connect.rs`.

## Fixtures and Factories

**Test Data:**
```rust
SessionConfig::default()
Credentials::with_password("test", "test")
```

Tests mostly construct minimal values inline, use `Default` implementations, and pass literal invalid inputs. No fixture factory module or checked-in fixture data directory was found.

**Location:**
- Inline in test functions and test modules, especially `oauth/src/lib.rs` and `core/tests/connect.rs`.

## Coverage

**Requirements:** None enforced. `test.sh` runs tests and compilation/lint checks but does not invoke `cargo llvm-cov`, `tarpaulin`, or another coverage tool, and no coverage threshold is declared in `Cargo.toml`.

**View Coverage:** No repository-prescribed command. A coverage tool would need to be added separately before reporting coverage as a project metric.

## Test Types

**Unit Tests:**
- Pure parsing, conversion, URI, cache, shuffle, and protocol helper behavior is tested close to implementation in modules such as `src/main.rs`, `core/src/spotify_uri.rs`, `core/src/cache.rs`, and `connect/src/shuffle_vec.rs`.

**Integration Tests:**
- Crate-level behavior is tested under `<crate>/tests`; `core/tests/connect.rs` performs a time-bounded asynchronous session connection attempt and verifies invalid credentials produce a non-empty error.

**E2E Tests:**
- No browser or end-to-end framework is used. The closest system-level coverage is the real network/session test in `core/tests/connect.rs`.

## Common Patterns

**Async Testing:**
```rust
#[tokio::test]
async fn test_connection() {
    timeout(Duration::from_secs(30), async {
        let result = Session::new(SessionConfig::default(), None)
            .connect(Credentials::with_password("test", "test"), false)
            .await;
        assert!(result.is_err());
    }).await.unwrap();
}
```

The concrete implementation in `core/tests/connect.rs` uses a 30-second Tokio timeout and matches the result to ensure authentication does not unexpectedly succeed.

**Error Testing:**
- Invalid inputs are tested by asserting `None`, exact error variants/messages, or non-empty error text; examples include `oauth/src/lib.rs`, `core/tests/connect.rs`, and parser tests in `src/main.rs`.
- Tests use `panic!` when an impossible/successful branch would invalidate the test premise, as in the authentication integration test.

---

*Testing analysis: 2026-07-31*
