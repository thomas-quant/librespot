# One-response, real-audio candidate test

This is an **experimental compatibility test**, not a verified PlayPlay-v5 key
implementation. It tests the historical algorithm against real content rather
than treating HTTP 200 or sixteen returned bytes as success.

## Exactly what it does

With both `--send` and `--try-legacy-decode`:

1. Validate the pinned DLL, local historical table and worker before login.
2. Authenticate and require the explicitly expected account tier (normally Free).
3. Select one available **OGG_VORBIS_96** file, including bounded relinking.
4. Resolve that file's signed CDN URL and fetch **one range starting at zero**,
   at most **512 KiB**. Require HTTPS, an allowed CDN host, the matching `/audio/`
   file-ID path, HTTP 206 and the entire requested range (or the entire shorter
   asset). Short range fulfillment is an acquisition error before the POST.
   No redirect/retry or
   second CDN request is made by this diagnostic.
5. Make **one application-level v5 PlayPlay POST** for that same file. Retain its
   complete response only in memory. Require HTTP 200 and an unambiguous 16-byte
   field 1 before attempting transformation; this is not policy/license validation.
6. Test two predeclared, **unproven** candidates from historical commit
   `1aa229c2f85602b366fd549f0208ed93dbe53693`:
   - historical transformation followed by its file-ID binding;
   - historical intermediate output without that binding.
7. For each candidate, decrypt from absolute offset zero using the existing
   `AudioDecrypt`, then expose Ogg at plaintext offset **167**. Validate complete
   Ogg pages, sequence/serial and CRC before decoding. Require Vorbis, stereo,
   44,100 Hz and at least **five seconds of finite PCM**, without skipping bad
   packets or treating early EOF as success.

A failure rejects **only these tested candidates for this response/file**. It
neither disproves v5 support nor establishes a Free-tier restriction. Changing
request constants does not prove the old transform compatible or incompatible.
The current native protected transformation remains a separate research problem.

Success means a validated **audio prefix**, not whole-track completion, audible
playback, production readiness, license-policy/expiry validation or general
support across accounts/formats. No audio sink is opened.

## Run the verified CI artifact

The artifact now contains `playplay_probe`, `legacy_candidate`, guides,
`SOURCE_COMMIT` and `SHA256SUMS`. Verify checksums before running either executable.
Only run the trusted CI-built worker: it receives sensitive material over stdin.

In the owner's existing clone, the hash-checked historical table has been prepared
locally at `local-test-data/playplay-probe/legacy-table-v2.bin`. It is **not** in
Git or CI artifacts and is not a content key. Its 768 little-endian uint32 words
are extracted from that historical commit's `main.h`; required SHA-256:

`c9f69fe06130ec69d86f31553ce2f06e8ffce4350bf1610a92bdef38dcf3ae09`

```sh
# All paths are local. ARTIFACT points to the downloaded, checksum-verified bundle.
"$ARTIFACT/playplay_probe" --spotify-dll "$DLL" \
  --try-legacy-decode --legacy-worker "$ARTIFACT/legacy_candidate" \
  --legacy-table local-test-data/playplay-probe/legacy-table-v2.bin

# Once offline preparation passes, enable exactly this live experiment:
"$ARTIFACT/playplay_probe" --spotify-dll "$DLL" --send --expected-tier free \
  --format ogg96 --try-legacy-decode \
  --legacy-worker "$ARTIFACT/legacy_candidate" \
  --legacy-table local-test-data/playplay-probe/legacy-table-v2.bin
```

Without `--send`, no account/network work occurs. The worker startup check uses a
zero input block merely to detect an unusable executable before wasting login;
it is not a separate algorithm-validation milestone.

## Privacy and bounds

The parent keeps credentials, response, candidate keys, signed URLs and encrypted
prefix in memory. Nothing saves those values, plaintext audio or PCM. Reports
contain only fixed candidate names/failure codes, HTTP metadata, byte/frame counts
and explicit validation flags. Library logging is not enabled; child stderr is
not forwarded. OAuth still prints its browser authorization URL: share the JSON,
not a terminal transcript. Windows ACL caveats in the main guide still apply.

The C++ candidate runs in a separate process with cleared environment, bounded
binary stdin/stdout, 5-second CPU and 256-MiB address-space limits. Decoder work
runs in a separate copy of the probe with an 8-second CPU / 512-MiB address-space
limit. Each child has a 12-second parent wall deadline and is killed on drop;
core dumps are disabled before authentication in this mode and within workers.
These are resource/process boundaries, not a claim of a hardened native-code
sandbox. The installed Spotify DLL is read/hash-checked, **never executed**.

CDN headers have a 30-second deadline and body collection a 20-second deadline.
Authentication/metadata/storage resolution retain their library behavior; the
complete inherited browser OAuth flow remains outside probe deadlines. “One POST”
and “one CDN GET” count this diagnostic's application-level dispatches, not all
login/metadata/storage-resolution traffic.

Exit **5** means both candidates were rejected by content checks. Preparation,
network, worker failures, decoder panics/budget exhaustion and insufficient
validated prefix are diagnostic/inconclusive outcomes, not candidate rejection.
If one candidate passes, the positive result is retained even if the other is
inconclusive.
Exit **0** with `candidate_audio_prefix_decoded` requires the PCM criteria above;
other modes retain their existing meanings. Always inspect the report's outcome.

CI uses a generated, lawful stereo Ogg tone to check the decoder path and a small
worker protocol/binding smoke test. CI never receives the private table, DLL,
credentials or captured media. Executable builds happen on Actions, not locally.
