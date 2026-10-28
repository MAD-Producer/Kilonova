# Real-codec and frozen temporal depth — issue #87

This increment carries admitted indexed video depth into independent PNG and
persistent MP4 tasks. It does not complete issue #87. Android, panel/style changes,
trained-model quality and later optional scene nodes are outside this increment.

## Ownership and source checks

- Capture is metadata-only on the Session owner. Each authored interval requires
  an already admitted result for that object, document, source, model, geometry,
  actual indexed PTS/end and prepared RGB fingerprint. Metadata alone is refused.
- Static entries and temporal entries share the existing 192 MiB scalar-plane
  budget. The combined snapshot admits at most `MAX_LAYERS` samples. Deduplicated
  scalar planes retain shared ownership; full-movie RGBA is not copied into it.
- Each temporal entry records the original registered VideoAsset, calibration,
  key, fingerprint, decoder budget and originating request. Unique object/PTS,
  interval order, source range and the exact project mapping are checked.
- The strict current scalar wire format is `MSDPTH2`. It adds mandatory temporal
  metadata, retains bounded 8 MiB metadata and chunked plane IO, and rejects older
  headers, unknown fields, bad indices, unreachable resources and oversized arrays.
  No legacy migration is provided during active iteration.
- Freeze copies the actual original and managed PTS cache. The originating
  platform factory is retained without retaining a decoder or inference worker.
  Persistent restore uses the maintained desktop factory.
- Verification decodes each frozen sample without inference. Exact PTS/end,
  display geometry, color interpretation and the protocol RGB fingerprint must
  agree. Per-frame attachment decodes only the selected sample, checks it again,
  and pairs those owned pixels with its admitted scalar plane on a scene clone.
  The ordinary video-color decoder cannot supply a second frame for that surface.
- Cooperative export cancellation reaches decoder packet checks. Attachment is
  committed only after all required layers succeed. Existing output publication
  remains transactional; failed/canceled work cannot replace a complete output.
- Info start records precede frozen video decoding and include source request,
  object, asset, source PTS, phase and bounded path. Routine completion is Debug.
  This adds no Present logging, inference retries, UI state or worker restarts.

## Verification status

The source-bound `frozen-temporal-depth-native-v5/accepted.json` is the acceptance
source. It incorporates the full media-library execution from v4 only after
checking that every source hash is identical, and explicitly runs the existing
manual key-timing test that the default library command excludes.

| Selection | Executed and passed | Failed / unexecuted |
| --- | ---: | ---: |
| Full FFmpeg/GPU/AI-depth media library | 59 | 0 / 0 |
| Host library | 113 | 0 / 0 |
| Video admission/native rendering | 19 | 0 / 0 |
| Depth-session workflows | 17 | 0 / 0 |
| Indexed-track model | 4 | 0 / 0 |
| Core calibration and layer clipboard | 16 | 0 / 0 |
| **Total distinct selected tests** | **228** | **0 / 0** |

The raw media-library command reports 58 passed and one ignored. The separate
`--ignored --exact` command executes that named test successfully; there is no
remaining skipped test in this selection. All other commands report zero ignored.
Host compilation without depth, desktop compilation with FFmpeg/depth, style and
provenance checks also pass. These are affected-path results, not final umbrella
full-workspace or remote cross-platform acceptance.

Actual codec tests use independently generated 32×32 H.264 source frames and
libav decode. A controlled CPU estimator derives its scalar field from the actual
decoded red channel. Its entire scalar field and the protocol fingerprint are
checked against an independently opened decoder, not a mocked RGB buffer.

For Unorm8, Unorm16, Float16 and Float32, the tests check independent blue-plane
occlusion, each selected source frame's near color, all re-encoded output frames
and their pixel differences from native GPU output. Persistent tasks are restored
after both Session closure and deletion of the original project directory. PNG
must reproduce native pixels exactly; every MP4 frame must meet the mean absolute
channel-difference limit of eight. Payload corruption and output-preserving
cancellation are exercised. Existing static PNG/MP4 tests remain unchanged.

The worst mean absolute channel difference across the 12 decoded frozen output
frames is 4.71875, below the limit of eight. `contact-sheet.png` from the final
media run was visually inspected: blue independent occlusion and the three
source-specific red regions remain present in every precision mode.

These small synthetic fixtures prove routing and ownership, not long-duration
throughput, trained inference quality, HDR delivery or photographic quality.

## Attempts

- Real-codec v1: the test oracle used a bare RGB digest. Production correctly
  hashes the RGB contract prefix and little-endian dimensions before the bytes.
  Correcting the oracle and adding an independent per-scalar comparison produced
  the accepted real-codec v2 run. This was not a production fingerprint failure.
- Frozen gate v1: compilation stopped because the new cancellation test called
  `unwrap_err` on an Ok type without Debug. No product test ran.
- Frozen gate v2: compilation stopped because the new test reused an export task
  after its consuming export call. The fixture now independently restores the
  task used by the cancellation test. No product test ran.
- Frozen gate v3: 173 selected tests passed with zero failures or ignored tests;
  no-depth/desktop compilation, style and provenance checks also passed. This
  predates the decoder-cancellation regression identified below.
- Mid-decoder cancellation red v1: a wrapper cancels only after entering an
  actual H.264 decoder's frame call. The exact-reason assertion failed because
  source context prefixed `movie export cancelled`. The source checker now
  preserves the first original control failure across the private callback
  boundary; native decoder faults still retain source/request context.
- Frozen gate v4: all 58 default media-library tests passed, including the
  mid-decoder cancellation guard. Its strict controller stopped at the inherited
  ignored manual test, so this attempt alone is not accepted.
- Frozen gate v5: verifies source equality with v4, explicitly passes the manual
  test, and completes the host/model/core and compilation/style/provenance gates.
  The combined selection above is accepted without changing any old assertion.

## Evidence and main integration

[Archived evidence](data/frozen-temporal-depth-native-v1.zip), SHA-256:
`5cf64a3206341aa90aaeabd823388838f7623fbdd6c3107d8e3936492d06e884`

The 383 archive entries include raw attempts, commands, source hashes/snapshots,
comparison reports, actual movies, native PNGs, scalar payloads and the inspected
contact sheet. Every entry was read back and checked by SHA-256. The archive is
2,066,222 bytes and includes unsuccessful attempts with their original disposition.

PR #544 was merged only after all five exact-head required maintained-desktop
checks passed. Its actual squash title/body were verified as Chinese. Fetched
main `79cddb38ff18893b6309e396c300bfb2a098f705` has source tree
`2e8b8f15c9fa26b67cc22f723db272c0d5f56789`, exactly equal to tested parent
`dca2f7f666844ffaa6e1506c9be4d93b73dbaf9c`. The publication verifier requires
that equality, an unchanged final commit tree across rebase and source hashes
identical to the final gate. No panels, UI layout, styles or dependencies change.

## Remaining scope

This closes bounded temporal snapshot ownership, not unbounded movie storage.
The live cache still retains at most `MAX_LAYERS` samples and 192 MiB combined
scalar/RGBA. Freezing a track whose samples have been evicted fails explicitly;
there is no file-backed depth archive for longer tracks yet. Sustained real-codec
playback and long-duration temporal ownership need separate acceptance.

Nested calibrated depth, complex effect/mask/matte combinations, exposure depth,
intersecting transparency, DOF, normative asset comparisons and final current-main
full-workspace/maintained-desktop acceptance remain open. Missing calibration
intervals and unsupported combinations continue to fail explicitly. Existing
Unorm/float precision behavior is preserved; no Android validation is claimed.
