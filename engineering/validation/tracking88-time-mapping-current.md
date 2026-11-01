# Exact tracking source-time mapping

Issue [#88](https://github.com/MAD-Producer/Kilonova/issues/88) remains open.
This increment supplies exact temporal lookup alongside the tracking asset,
calibration and coordinate-conversion boundaries. It does not replace the project
fps representation or complete the open #85/#86 prerequisites.

## Contract

`tracking::time::Time` accepts explicitly declared signed ticks or a frame number
on a caller-supplied rational clock. Reusing the existing validated `asset::Rate`
value for that input does not activate rational project fps. A `Mapping` declares
both a project-time anchor and an exact signed source-PTS anchor, with a bounded
signed rational speed. Negative speed reverses time; zero explicitly freezes it.
No clip origin, speed, source rate or project rate is inferred.

`Timeline::new` validates the asset once and borrows it immutably. Subsequent
`locate` calls allocate nothing and binary-search its actual VFR timestamps.
The nominal source rate is never used to regenerate PTS. Arithmetic uses reduced,
checked `i128` fractions, retaining signed origins and adjacent source timestamps
above the binary64 integer-precision limit. Cross-cancellation reduces avoidable
overflow; any arithmetic beyond the supported exact range returns `NumericRange`
instead of rounding, wrapping or saturating.

Lookup returns an exact source index, a pair of source indices with an exact
fractional weight, or an explicit before/after-range disposition. It never clamps
to the first/last sample. The temporal bracket does not permit interpolating
across an individual track's missing, lost, occluded or manually discontinuous
observations. No observations, manual edits, calibration, PTS or metadata are
rewritten. Pose interpolation, gap policy and project binding remain separate
unfinished work. Normal outside-range results have no logging/file side effects;
typed asset failures retain their original `Error::source` cause. An eventual
import command must add request/source context and default-level error logging.

## Actual evidence

The isolated direct-source CPU harness passed **56 tests, zero failed, zero
ignored**, including all 47 preceding geometry/asset/codec/pose/plane controls
without changing their assertions. Nine added tests cover:

- Independent 24/25/30/60 and 24000/1001, 30000/1001, 60000/1001 clock controls
- Original VFR timestamps overriding nominal source fps
- Negative clock origins and adjacent signed PTS above `2^53`
- Explicit range boundaries and a negative fractional-time golden
- Reverse speed, nonzero anchors and explicit freeze at extreme clock values
- Full signed-i64 PTS span and exact interpolation weights
- Invalid timebase/speed, exact-arithmetic overflow and preserved asset causes
- Unchanged sparse/manual/occluded asset bytes after repeated lookups
- 3360 mixed clock/speed/frame cases checked against an independent integer oracle

Evidence and SHA-256 receipts are retained under
`E:/Dev/aem/artifacts/pr-queue/tracking88-time-mapping-v1`. The first compilation
caught ambiguous test imports and a missing fixture unwrap; the second shell
lacked the MSVC linker environment. Both original compiler logs are retained.
No test ran before those fixture/environment corrections, and no assertion was
weakened. The actual successful compile/test logs are retained separately.

The [fresh host integration](tracking88-host-integration-current.md) completed
actual check plus all 56 tracking tests, after separately proving the unchanged
44-test pose and 47-test plane stages. All 169 host unit tests also passed.
The real terminal receipt is `tracking88-host-integration-v2/exit.json`.
Direct-source controls alone were not used as host acceptance. The earlier
queued plane/time controllers executed no tests after their preceding coverage
failure; those receipts remain retained.

The initial queue was stopped before any source edit, build or test because its
proposal applicability check found Windows CRLF patch line endings. The corrected
LF patch passed the read-only applicability check before the replacement queue.
The zero-step original receipt is retained in `tracking88-time-host-v1`.

Project source identity verification, persisted binding/conversion provenance,
history/save/reopen/relink/invalidation, interpolation with manual corrections,
actual rendered synthetic and calibrated real footage, and lens replacement
remain pending. This is neither a tracker nor a 3D camera solver. No previous #82
P95 failure is rerun or reclassified by these CPU results.
