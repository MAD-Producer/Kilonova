# Original decoded tracking source admission

Issue #88 remains open. This increment extends the
[managed original import](tracking88-managed-import-current.md), on main baseline
`8573d9975cf70d2c8fe05605102ec9d744816070` and import checkpoint
`54ef14bb5a6e879d211ff9a2ec89b13c5c27b08d`. It verifies acquired original media;
it does not apply tracking observations to a camera, layer or parent.
The final Git fetch encountered a connection reset. A subsequent read-only GitHub
API query returned the same main SHA, so no new main-source delta was identified.

## Product boundary

`Prepared::read`, reached by the existing desktop/MCP `import_tracking` operation,
now fully decodes an image from the same open file used for its encoded SHA-256.
The decoded dimensions must match the solve. A correct digest and mutually
consistent project/solve metadata cannot admit invalid pixels or a different
actual size. Decoding is bounded to the declared extent and a 512 MiB image
allocation limit. Sixteen-bit PNG originals are accepted without substituting an
eight-bit image. No decoded pixel conversion is needed for this check.

Video import requires a platform capable of verifying the original source. The
portable default rejects this operation explicitly; it never reports decoded
verification after an unsupported backend returns. The libav adapter opens the
selected original stream, checks its raw dimensions, codec, rotation and square
pixel aspect, and sequentially decodes every presentation frame through EOF.
Missing original PTS or disagreement with a decoder-inferred timestamp is an
error. The adapter neither seeks through the preview index nor converts pixels.

Each actual signed frame PTS and the native rational clock are compared against
the declared source clock using checked `i128` products:

```text
declared_pts * native_denominator
    == native_pts * native_numerator * declared_ticks_per_second
```

There is no source-FPS inference, floating-point comparison, microsecond
rounding, frame sorting, duplicate removal or timestamp repair in this path.
Every decoded frame must match in order, including VFR gaps and nonzero origins;
missing or extra frames reject the import. A failed comparison permanently
invalidates that acquisition, even if a caller attempts to continue feeding it.

The selected managed path is reopened and fully hashed again after decoding.
The held source handle's size and modification metadata are checked around the
operation. Encoded identity, actual decoded dimensions and exact video PTS must
all succeed before a managed tracking original is staged. Existing revision,
generation, saved transaction, rollback and undo/redo protections remain active.
Image decoding, native decoding and source hashes stay outside the session lock.

Successful responses add `decodedSourceVerified: true`. Video responses report
`decodedTimelineVerified: true`; image responses keep that field false because a
still image has no intrinsic video presentation timeline. `poseApplied` remains
false. These are admission results for this acquisition, not persisted promises
that an external file will remain unchanged. The MCP schema states this scope.

## Controls and receipts

Two actual-file negative controls were run before implementing decoded admission.
The old implementation accepted both an 8x6 PNG declared as 7x6 in the project
and solve, and an invalid image payload with a matching recomputed source digest.
The controlled run exited 101 with **0 passed, 2 failed, 0 ignored** and the two
intended assertion markers. Those test bodies remain unchanged by the fix.

The first positive attempt passed desktop check, but the new clock tests did not
compile because two glob imports made `Error` ambiguous. Explicit asset imports
fixed this test-source error without changing any assertion. The next attempt
passed **100 tracking tests** but failed four new native controls in fixture setup:
the product MP4 encoder rejects a movie with no audio samples. The fixture now
authors exactly 0.2 seconds of silence alongside six independently authored 30 fps
frames. Its existing admission and PTS assertions remain intact. Both failed
receipts and exact source snapshots are retained.

The corrected native controls pass **5/5**, including real decoding, managed
publication/save/undo/redo, a one-tick discrepancy at 900,000,000 ticks/second,
frame-count/dimension/stream mismatches and cancellation inside native decoding.
Two existing independently generated video fixtures add B-frame, nonzero-origin
and variable-interval coverage using their raw packet timing tables as the oracle.
Only the test oracle sorts packet decode order into presentation order; production
never sorts imported observations or actual decoded frames.

Before the final diagnostic-context adjustment, V3 passed the complete host
library gate **213/0/0** and all **9 MCP tests**. Those results describe V3 source;
they do not supersede the final-source failures below.

Final V4 puts the original decoded cause before a potentially long path and
bounds the native path context to 256 characters. It adds an actual error-prefix
control so the existing 1,536-character observation limit preserves the cause.
The final desktop check passes, **101 tracking tests** pass and the **5 native
tracking tests** pass, all with zero failures or ignored tests. The two original
negative test bodies are byte-identical to their pre-fix versions.

The final complete host run **fails: 212 passed, 2 failed, 0 ignored**. Failures:

- `session::precision_preview_tests::actual_surface_preview_changes_precision_without_replacing_the_device_and_retains_sdr_fronts`
- `session::registry_observation::tests::actual_recovery_is_quiet_when_healthy_and_emits_bounded_evidence_after_unlock`

Their child outputs stop without a Rust failure message. Matching Windows events
for the exact retained test image report `0xc000001d` and `0xc0000005`. The image
timestamp is `0x6ae6d189`; its SHA-256 is
`25050433f78a07d55c105eb15c6fb6267733001ba2dd79a2c0ed74177a01cada`.
The exact EXE, PDB, dependency record and raw events remain private under
`artifacts/pr-queue/tracking88-decoded-v4/private-native-failure`. Read-only DbgHelp
lookup associates RVAs `0x2970173` and `0x296802f` with core string-search
monomorphizations used by Naga's WGSL lexer. Address association is not a proven
root cause or a repaired fault. These tests and the unchanged full gate were not
rerun.

The next controller's final-source MCP unit target also **fails to build**:
`rustc` exits `0xc0000005` before any test executes. The original command, compiler
diagnostics and failed receipt are retained. No assertion or test selection was
changed to make that target pass, and it was not retried. Final-source complete
host and MCP unit qualification therefore remain failed/pending.

## Actual product executable acceptance

A separate serial controller builds the product executable without FFmpeg and
with FFmpeg; both builds exit zero. These are distinct native product targets,
not reruns or substitutes for the failed test target. Four real stdio processes
then exit zero:

- bootstrap a valid project
- reject video tracking import without an original decoder, preserving project,
  revision and the absence of a staged original
- decode all 72 original B/VFR frames, reject a one-tick submicrosecond mismatch
  without mutation, import the correct original, save, undo exactly once, redo
  after removing the external original, and save again
- reopen read-only and confirm the identical saved project

The successful real video response reports `originalPreserved`,
`encodedSourceVerified`, `decodedSourceVerified` and `decodedTimelineVerified` as
true, with `poseApplied: false`. The original managed JSON is byte-exact.
The default environment has `MOTION_LOG` unset. Actual file logs contain Info
worker start and source decode phase; Error file and stderr records retain request,
source ID, phase, elapsed time and the mismatching frame/clock. Every stdout line
is parsed as JSON-RPC with matching IDs and no trailing nonprotocol output.
Transcripts retain each line before parsing, with OS newline normalization; they
are not claimed to be byte-exact process stdout.

Archive [source snapshots, negative controls and raw receipts](data/tracking88-decoded-source-v4.zip):
**132 read-back-verified members**, SHA-256
`4c6333e654a9b20f651a4ae8b4234d6afa6084a2a38ea68b46fc4c39591cd8da`.
It includes failed V1/V2 controls, historical V3 success, final V4 failures,
the compiler failure, both actual product builds, protocol/console/file logs,
generated originals, the unchanged independently generated repository video
fixture and a sanitized native failure summary. Executables, PDBs, raw system
events, transient project locks and regenerated bootstrap plugin packages are
excluded. The archive manifest identifies exact final code hashes and unchanged
negative test bodies. [Static closeout](data/tracking88-decoded-source-closeout.json)
passes whitespace, existing panel style-consumer checks, the provenance guard
(5,135 paths at that staging snapshot) and the 57-tool inventory with zero Android
selectors/tests. Final code hashes still match the actual admission gate. No panel,
renderer, dependency, lockfile, project schema or Android source was changed.

## Remaining boundaries

This does not persist a decoded proof or bind observations into evaluated scene
poses. Runtime camera/object/parent bindings, original-asset relink and cache
invalidation, #85/#86 project-clock integration, explicit gap/manual policies and
rendered synthetic plus calibrated real-footage/lens-replacement acceptance remain
required. Current full workspace/desktop and exact-head cross-platform CI are not
established by this increment. Android remains excluded by user instruction.

The source checks are acquisition evidence, not a filesystem lease against an
external A-to-B-to-A replacement between decoder opens. Cooperative checks cannot
interrupt a blocked native read or an image decoder inside its library call. This
cold operation still occupies the existing serial engine worker; Info start and
in-flight state precede it. No per-command restart or thread timeout was added.

The canonical clock comparator supports signed origins, but the existing managed
video model/index still uses unsigned microseconds. Files whose negative preroll
was excluded from the managed frame count are explicitly rejected when the full
original decode disagrees; they are not silently repaired. Clock-model integration
is still needed before advertising arbitrary negative-preroll import.

The final-source child faults and compiler access violation also require diagnosis
before full qualification. The earlier original-source host gate's native
`0xc0000005` remains unexplained;
later changed-source successes do not fix it. Its exact private binary/PDB/events
remain retained. Original #82 strict P95 failures also remain failed. No unchanged
performance gate was rerun, assertion weakened, sample reduced or cached GPU
coverage presented as complete by this increment.
