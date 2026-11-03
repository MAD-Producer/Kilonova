# External tracking JSON round-trip boundary

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This increment adds `motion_host::tracking::encode::to_vec` to the
[strict JSON decoder](tracking88-json-decoder-current.md). It validates the typed
asset before serializing and returns independently owned JSON bytes, with no
project, file, source, layer or camera mutation.

## Behavior

The encoder emits the same explicit version/axis/source/time/observation contract
accepted by the decoder. It preserves sparse indices, visibility, confidence,
manual corrections, plane reference/scale, camera pose, declared world units and
per-observation lens calibration. Source content hashes use canonical lowercase
hex. Optional values are explicit nulls. Track/sample order is preserved.

Tracks and samples are borrowed and serialized incrementally, without building a
second owned observation graph or an intermediate JSON value tree. Output is
limited to the decoder's 16 MiB transport boundary. A valid typed asset can be too
large for this transport: encoding returns `OutputLimit` rather than dropping
samples, truncating bytes, changing timing or returning incomplete JSON. The
bounded output buffer is private and discarded on error. This is a byte limit,
not an exact heap-capacity promise.

Validation happens before serialization, so non-finite values cannot silently
become JSON nulls. Invalid assets retain the original structured cause and
track/sample location through `Error::source`. Internal serialization failures
also preserve their cause. No failure is logged here; the pending import command
still needs the repository's operation/request/source start and error records.

The host now explicitly enables the existing serde_json `float_roundtrip` feature.
Previously that feature was requested by the desktop crate only; a standalone
host build must not depend on downstream feature unification for preserving
imported binary64 calibration/pose values. No dependency/version, external asset
or compatibility layer was added. Existing provenance/attribution/notice policy
was checked. No migration or legacy-format fallback was introduced.

## Actual verification

The direct CPU source harness now runs **35 tests: 35 passed, zero failed, zero
ignored**. The original 31 geometry/asset/decoder assertions remain intact; four
new serialization tests cover:

- Actual encode/decode of all three observation kinds, source PTS beyond the
  exact integer range of a binary64 JSON intermediary, negative origins/VFR,
  sparse/lost/occluded/manual state, Unicode IDs and all content-hash bytes
- Bit-for-bit preservation of finite binary64 values, including signed zero,
  extremes, subnormals and a deterministic corpus generated from 2048 bit patterns;
  camera poses, quaternions, calibration and confidence also compare their bits
- Refusal of invalid/non-finite assets before serialization, with unchanged
  input and structured original causes
- Exact output-boundary behavior and a real validated one-million-sample asset
  that exceeds the JSON limit, returning an error without partial bytes

Evidence is retained at `E:/Dev/aem/artifacts/pr-queue/tracking88-roundtrip-v1`:
the exact source harness, compiler/test logs and a SHA-256 receipt. The existing
serde_json artifact used by this direct harness has `float_roundtrip` enabled,
matching the new explicit host feature declaration. Cargo dependency resolution,
the actual host module tree and full regression still need their own execution;
direct source compilation does not substitute for them.

#82's frozen full workspace test and its queued native admission/phase controls
continue independently. No second Cargo/GPU/native-window validator was started.
No panels, UI, Android or another agent's source files changed. A fresh main fetch
was attempted again and failed because the configured local proxy on port 7897
was unavailable. The current cached `8573d997` base is not claimed to be latest.

## Still pending

This closes the standalone byte serialization/decoding increment, not project
persistence or the full issue. Source-content verification; explicit external
axis/world/project-time conversion; pose/camera/object/parent bindings; editing,
undo/redo/save/reopen/relink/cache invalidation; actual rendered reprojection and
real calibrated footage/lens replacement remain pending. #85/#86 prerequisite
integration, latest-main reconciliation, host Cargo and exact-head CI also remain
required. No tracker or camera-solver capability is advertised.
