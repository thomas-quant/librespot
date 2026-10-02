# Historical candidate worker — diagnostic only

`decrypt_main.cc`, `process.cc`, `defs.h`, and `ppdecrypt.h` are retained arithmetic/helpers (with whitespace cleanup only) from this repository's historical commit `1aa229c2f85602b366fd549f0208ed93dbe53693`, under the existing repository license. `main.h` retains required declarations/helpers but replaces the historical static table with an external array. No historical token comment or table values are bundled. No current native DLL data is copied into these sources.

The new worker receives the hash-validated 768-word historical table, one pre-transform response block and a file identifier through a bounded private stdin pipe. It returns only two 16-byte candidate outputs through stdout to its parent; do not run it in a terminal with real material or redirect that pipe to a file.

Candidates are the historical transform with and without its binding step. **Neither is established compatible with v5.** The parent validates candidates against the matching real audio prefix. A worker exit, a 16-byte result or an offline smoke test is not key/license validity.

Build through `.github/workflows/playplay-probe.yml`, on Linux x86_64, with explicit signed wrapping/no strict-aliasing assumptions matching the legacy decompiled arithmetic. No local build is implied. `smoke.py` checks framing and binding using synthetic data; it is not an independently validated deobfuscator oracle.

Original-source SHA-256:

- `decrypt_main.cc`: `ccb5bdea19ee7843f59322828202cb3d12311130c408a06f16218d30779f1e4b`
- `process.cc`: `911e74fd4a1e2c3eda1dab120fdc76dadad67e38f87a441844ed64797f7f2fa9`
- `defs.h`: `90eaca79c3e7849424975a0476220a56c4750296e639e021c338d6583fa7aa78`
- `ppdecrypt.h`: `37e9f749ed741baa81ca152faa30c063b45e311120ee6ce462e15f6f253849d4`

See `docs/PLAYPLAY-REALDATA-PROBE.md` for live scope, resource bounds and evidence limits.
