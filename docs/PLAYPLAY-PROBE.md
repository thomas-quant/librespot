# Request-only PlayPlay compatibility probe

By default, `examples/playplay_probe.rs` tests a **candidate request body**, not
audio playback. Its normal `--send` mode does not decrypt responses or fetch CDN
audio. A separate, explicit `--try-legacy-decode` mode tests two unproven historical
candidates against one real OGG96 prefix; see [the real-data guide](PLAYPLAY-REALDATA-PROBE.md).
Neither mode integrates a production key manager or uses Widevine device files.

## What is being tested

Static analysis of one installed Windows `Spotify.dll` (1.3.1.234) recovered a
PlayPlay constructor using version **5** and a different 16-byte token from the
historical version-2 spike. The first six wire-field positions are compatible,
but native omission/enum behavior differs. Detailed local analysis is retained at
`.planning/research/DESKTOP-RE-PASS2-2026-10-02.md`; those local notes and private
inputs are not required or uploaded by the CI build.

This probe combines that **body candidate** with the existing librespot
OAuth/AP/login5/client-token flow. It does **not** claim to reproduce the full
Windows authentication context. The report records librespot's advertised client
versions and marks Windows-context equivalence unverified.

- `version=5`, recovered token, omitted empty cache ID.
- Audio/raw-file family only: field 4 = 1, field 5 = 1.
- Whole local Unix seconds; zero/empty omission for fields 1–6.
- Unknown field 7 omitted. No invented native enum or field-7 names.
- The requested metadata format must actually be offered on an available track.
- No fallback to a different format, account tier, DRM, token, or request version.

**An HTTP success is not a validated license, usable key, or successful playback.**
A 403 also does not establish a universal Free-account restriction. A matching
version-5 response transformation/deobfuscator remains untested.

## GitHub Actions

The `playplay-probe` workflow tests and builds only this diagnostic on Ubuntu
24.04 with Rust 1.97.1 and one Cargo job. It runs synthetic offline tests, Clippy,
formatting, a generated lawful Ogg/PCM fixture and an offline `--help` smoke test.
It also builds the resource-bounded historical candidate worker without a table.
It never receives Spotify credentials, an installed DLL, WVD samples, the local
historical table or captured media/reports, and never enables `--send`.

Its `playplay-probe-linux-x86_64` artifact contains `playplay_probe`,
`legacy_candidate`, this guide, `REALDATA.md`, `SOURCE_COMMIT`, and `SHA256SUMS`. After download, run `sha256sum -c SHA256SUMS`
inside the artifact directory. Use that directory's `./playplay_probe` in place
of `target/debug/examples/playplay_probe` in the commands below. The Linux
executable requires compatible glibc/OpenSSL runtime libraries; it is not a
native Windows executable.

## Build/check commands

Respect the repository owner's local-build approval rule. The following build
and test commands are instructions, not a claim they have been executed. Use the
configured job caps; on the current memory-limited machine, one Cargo job is
appropriate when no other cap is set.

```sh
# Type-check without linking an executable:
CARGO_BUILD_JOBS=1 cargo +1.97.1 check --example playplay_probe --tests \
  --no-default-features --features native-tls

# Execute the focused offline tests (requires test-executable compilation):
CARGO_BUILD_JOBS=1 cargo +1.97.1 test --example playplay_probe \
  --no-default-features --features native-tls

# Build the executable, after approval or through an authorized CI branch:
CARGO_BUILD_JOBS=1 cargo +1.97.1 build --example playplay_probe \
  --no-default-features --features native-tls
```

No audio backend or discovery feature is needed for this request-only example.
The tests use synthetic token bytes and in-memory bodies; no account, DLL,
credential sample, or network service is needed to run them.

## Offline validation first

Point at your own installed DLL. The path below is a placeholder:

```sh
DLL='/mnt/c/Users/<WindowsUser>/AppData/Roaming/Spotify/Spotify.dll'
target/debug/examples/playplay_probe --spotify-dll "$DLL"
```

Without `--send`, the executable does **no network or authentication work**. It
checks the DLL's exact size and SHA-256, extracts 16 bytes from the same stream it
hashes, and checks their fingerprint. It never executes or loads the DLL.

Supported DLL SHA-256:

```text
0731eca3ec438395815907c04653c63a917b55bf0ebdd83c96f041424a92b54b
```

A different/updated DLL fails closed; there is no force flag or offset override.
Do not distribute the DLL or extracted token with this project. A matching hash
identifies the researched artifact, not a clean vendor signature or server
acceptance. The researched installation contains BlockTheSpot-related files;
their behavior has not been attributed or changed.

## One live request

```sh
target/debug/examples/playplay_probe --spotify-dll "$DLL" --send \
  --expected-tier free --format ogg96
```

This explicitly enables:

1. Browser PKCE OAuth (`streaming` scope), then AP session authentication.
2. Account-tier confirmation. Default expectation is `free`; mismatch or unknown
   tier stops before the PlayPlay request. `--expected-tier premium` permits a
   deliberately separate Premium control, not an automatic fallback.
3. Metadata lookup for the known test track, with bounded alternative/relinking
   lookup (at most eight items). Restricted items are not selected.
4. Login5 bearer and client-token acquisition. Both must succeed; no silent
   omission of the client-token header.
5. **At most one application-level POST** to the resolved Spotify HTTPS host's
   `/playplay/v1/key/{file_id}`. No metrics/salt query decoration, diagnostic
   repost, application retry, redirect following, or 429 retry loop is used.

Authentication/metadata can make their own requests through existing library
code. “One POST” refers to this diagnostic's PlayPlay dispatch, not the entire
login exchange. The complete inherited OAuth flow—including callback waiting
and the subsequent token exchange—is outside this probe's deadlines and may
wait indefinitely. Post-OAuth service stages have 30-second deadlines; PlayPlay
body capture has a separate 15-second deadline. Interrupt a stalled OAuth flow
rather than assuming its token exchange is bounded.

Options:

- `--track BASE62_ID`: override the public test-track ID.
- `--format ogg96|ogg160|ogg320|aac24`: select exactly one offered format.
- `--access-token-file PATH`: explicitly supplied token file instead of browser
  OAuth. No command-line bearer-token option, automatic cache search, or persisted
  credentials. Its original client/scope context may differ; browser OAuth is
  the preferred initial experiment.
- `--report PATH`: choose a new JSON report file. Existing files are never
  overwritten. The destination is reserved before authentication, so an invalid
  output path does not waste a live request. An interrupted process can leave
  an empty checkpoint file. Completed JSON is written/flushed to the file before
  a fallible console copy, so a closed stdout pipe cannot discard saved evidence.

## Original-response evidence and privacy

The probe uses `request_fut` directly, preserving the original non-2xx response
instead of letting the wrapper discard it and issuing another POST.

Response capture is limited to **64 KiB**, with explicit timeout, stream-error,
and truncation states. Headers/status survive incomplete-body collection. The
saved report contains an allowlist:

- HTTP status, approved MIME category, numeric Content-Length/Retry-After.
- Observed/retained byte counts and whether the body completed.
- JSON/protobuf-syntax/unknown shape; lengths of protobuf fields 1 and 2 if safely
  inspected, with absent fields reported as null.
- Only recognized, fixed symbolic error codes. No arbitrary server messages.

**Never saved:** raw request/response bodies, body previews, token bytes, cookies,
authorization headers, device IDs, usernames, account attributes, signed URLs,
licenses, or content keys. A 16-byte protobuf field is only shape evidence.
Unknown server messages are deliberately not persisted; investigate any need for
additional evidence explicitly rather than enabling unrestricted logging.

Static response-path inspection of the pinned DLL found a 16-byte field-1 gate
before transformation, and a separate field 2 copied only when its length is
four. Field 2's meaning and missing/wrong-length fallback remain unknown. The
probe reports lengths without enforcing those sizes, interpreting field 2, or
claiming native acceptance. An absent field is not a zero-valued field.

The shape inspector is deliberately conservative, not a replica of the native
protobuf parser. Duplicate or wrong-wire fields 1/2, groups and messages over its
1,024-field inspection budget are unclassified; that does not necessarily mean
invalid protobuf. Unknown fields using supported wire types remain opaque.
Nonminimal varints are accepted without inferring native canonicality rules.
Synthetic fixtures exercise these boundaries and incomplete captures. Even a
24-byte message with field lengths 16 and 4 is only one possible envelope—not a
reconstruction of an earlier response whose bytes were discarded. Older reports
without the field-2 length cannot supply that observation retrospectively.

The executable installs no library logger, so `RUST_LOG` cannot enable the known
credential/URL logging paths. The OAuth library prints a browser authorization
URL to the console; this is not included in the JSON report. Share the JSON,
not an unrestricted terminal/network transcript. The older `free_tier_spike`
example is a separate diagnostic and its unredacted output is not safe to share.

Default reports go to `local-test-data/playplay-probe/`, locally excluded through
this clone's `.git/info/exclude`. The HAR is also locally excluded. These excludes
do not travel to other clones; add them before running elsewhere. Files request
Unix mode 0600, but the Windows-mounted filesystem may report/allow broader
access; Windows ACLs govern Windows-side access.

## Exit statuses

- **0:** offline profile check completed, or a complete HTTP 2xx response was
  collected. Neither means license/decryption/playback is validated.
- **1:** preparation/authentication/transport/report error; inspect safe stage/code.
- **2:** invalid CLI or report destination; no live request made.
- **3:** complete non-2xx response, with original-response summary saved.
- **4:** response headers arrived but body capture was incomplete/truncated.
- **5:** explicit real-data mode ran, but all named candidate/settings and the
  undecrypted control failed content checks. This is not a verdict on all v5
  transformations or Free playback. Resource/setup/short-prefix failures are
  inconclusive diagnostic errors, not this negative outcome.

Request-only reports leave licensing, decryption, playback and universal tier
restriction claims unestablished. Real-data mode may validate only a decoded
prefix; it still makes no license-policy, audible-playback or whole-track claim.
Never turn an incomplete capture, refusal or candidate failure into a categorical
verdict about Spotify Free.
