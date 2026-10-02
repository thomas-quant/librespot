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
   field 1 before attempting transformation. The private in-memory envelope also
   retains optional field 2 (absence is not a zero value); historical candidates
   explicitly ignore it because its semantics are not established. None of this
   is policy/license validation.
6. Test two predeclared, **unproven** candidates from historical commit
   `1aa229c2f85602b366fd549f0208ed93dbe53693`:
   - historical transformation followed by its file-ID binding;
   - historical intermediate output without that binding.
7. For each candidate, decrypt from absolute offset zero using the existing
   `AudioDecrypt`, then discover a complete CRC-valid Vorbis identification/BOS
   page whose start is within the first **4,096 bytes**. This includes production
   offset **167** and offset zero without restarting CTR at the chosen offset.
   Validate subsequent pages' CRC, serial and sequence continuity from the
   observed initial sequence. Require Vorbis, stereo,44,100Hz and at least
   **five seconds of finite PCM**, without skipping bad packets or treating early
   EOF as success.
8. Inspect the same range under a separately named **undecrypted-container
   control** with the same placement/CRC/PCM criteria. It tests an alternative
   encryption assumption, not an additional request or another transformation.

Container discovery is not a key search and cannot recover a wrong key. The
baseline cipher remains named `production_aes128ctr_iv`; arbitrary IV/counter
variants are not enumerated. Reports add only safe container offset, initial
sequence and setting/classification metadata, never decoded page content.

A content-check failure rejects **only these tested candidates/settings and the
explicit control for this response/file**. It
neither disproves v5 support nor establishes a Free-tier restriction. Changing
request constants does not prove the old transform compatible or incompatible.
The current native protected transformation remains a separate research problem.

Success means a validated **audio prefix**. If only the undecrypted control
succeeds, it validates content decoding—not either transformed key.
Success is not whole-track completion, audible
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
limit. Each child has a 12-second parent wall deadline and is killed on drop. The
three decoder settings share that deadline and the eight-second CPU cap. After
one setting validates PCM, remaining expensive decoding stops and those settings
are reported `not_run_after_success`; later attempts cannot erase that success.
Without success, exhausting the shared budget is an inconclusive diagnostic
failure. Core dumps are disabled before authentication in this mode and within
workers.
These are resource/process boundaries, not a claim of a hardened native-code
sandbox. The installed Spotify DLL is read/hash-checked, **never executed**.

CDN headers have a 30-second deadline and body collection a 20-second deadline.
Authentication/metadata/storage resolution retain their library behavior; the
complete inherited browser OAuth flow remains outside probe deadlines. “One POST”
and “one CDN GET” count this diagnostic's application-level dispatches, not all
login/metadata/storage-resolution traffic.

Exit **5** means all three named settings (two candidates and the undecrypted
control) were rejected by content checks. Preparation,
network, worker failures, decoder panics/budget exhaustion and insufficient
validated prefix are diagnostic/inconclusive outcomes, not candidate rejection.
If one candidate passes, the positive result is retained even if the other is
inconclusive.
Exit **0** with `candidate_audio_prefix_decoded` requires the PCM criteria above;
other modes retain their existing meanings. Always inspect the report's outcome.

CI uses a generated, lawful stereo Ogg tone to check offsets0/167/311, named
undecrypted/decrypted settings and wrong-key/corruption behavior, plus structural
nonzero-sequence fixtures and a small worker protocol/binding smoke test. An
independent OpenSSL-derived AES-CTR known-answer digest and absolute-seek fixtures
check the cipher path; they do not establish the real file's cipher/IV. Envelope
fixtures preserve field-2 presence without assigning semantics. CI never receives the private table, DLL,
credentials or captured media. Executable builds happen on Actions, not locally.
