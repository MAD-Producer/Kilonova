# Depth resolution export candidates

Issue #75 remains open. `tools/export_ai_depth.py --side 266|392|518` exports
an explicit square ONNX candidate from the already attributed, externally
installed Small weights. The default remains 518. A resolution is not a
fast/standard/high quality label or a claim about interactive performance.

The export procedure checks the pinned source revision, unchanged reference
checkout and original weight size/hash before loading the model. Publication
requires finite reference and ONNX results, exact names/shapes/Float32 types,
CPU-only execution and the original normalized maximum 2e-4 / mean 2e-5 bounds.
Each output has a distinct filename, processing identity, shape and SHA-256.
Existing destinations are refused. No download, dependency or asset is added.

## Source-bound qualification

The qualified implementation is commit `d9443b566595eb6bf5314f4f4bf46744e8a7d214`.
The [evidence manifest](data/depth75-resolution-qualification-v1.json) binds the
[116-member archive](data/depth75-resolution-qualification-v1.zip), source
hashes and original numerical limits. The archive contains controllers, raw
outputs, runtime profiles, model manifests and the native probe source/lock.
It excludes model weights, ONNX graphs, source images, raw tensors and binaries.

All three real exports passed their official PyTorch reference probe. The 518
export reproduced the existing complete model hash; 266 and 392 have independent
hashes. Full digests and external package versions are in the archived manifests.

| Side | ONNX SHA-256 prefix | Model bytes |
| --- | --- | ---: |
| 266 | `07eb7aeb46a7` | 98,940,310 |
| 392 | `f78598e32582` | 98,940,310 |
| 518 | `274368034c0f` | 98,941,070 |

For each side, actual inference was compared on 153x101, 101x153 and 1x53 crops
from the existing attributed Destiny laboratory photograph. The independent
reference uses official normalization/model execution and PyTorch aligned-endpoint
resampling; it does not call the owned geometry helper or scalar resampler.
All nine Python worker comparisons and all nine actual native Jobs comparisons
passed. The worst normalized maximum was 0.0000114882229929 and the worst mean
was 0.000000766092366, below the unchanged 0.0002 / 0.00002 bounds. No fitting,
affine adjustment or per-frame output normalization was applied.

## Native ownership, failures and logging

Two new Rust profile/report tests passed on the qualified source. They cover
exact processing identity, cross-profile substitution, schema/hash/dimensions,
source-resource bytes and independent RGB/scalar Float32 transfer byte counts.
Nine focused Python admission tests passed, including real invalid requests in
normal and optimized Python, crop orientation and odd padding. The complete
Python tool suite passed 119 tests with one original skip (120 total); its
qualified Python implementation hashes match this source.

Actual Jobs published in-flight state before inference, preserved source,
geometry/model identity and exact tensor transfer sizes, and cleared flight,
queue and temporary files on completion. Each profile also executed a real
known-but-mismatched processing request against its model bytes. All three were
refused with the original session/shape cause. At the default Info level, start
records and Error causes reached the file, Errors reached stderr, and stdout
contained only the probe protocol. No product assertion or return value changed.

The first external probe build failed because it called `same_sample` on an
`Option<DepthSource>`. Its original compiler output and source hashes remain
archived. Accessing the published sample through `as_ref().expect(...)` fixed
the probe; product source and all original assertions stayed unchanged. The
subsequent locked/offline probe build and actual requests passed. These native
controls qualify the new paths, not the complete workspace or physical depth.

## Observed costs and quality limits

The native probe measured three cold CPU calls per profile on this machine.
These ranges are samples, not percentiles, sustained limits or capacity claims.

| Side | Inference range (ms) | Full Jobs wall range (ms) |
| --- | ---: | ---: |
| 266 | 258.449-269.743 | 1,203.346-1,796.842 |
| 392 | 617.824-655.382 | 1,706.479-1,771.775 |
| 518 | 1,247.178-1,346.790 | 2,266.952-2,298.448 |

Raw reports retain preprocessing, host transport, loading, input/output copies,
inference and postprocessing separately. Jobs input was already decoded RGB;
these are not end-to-end original-media decode or export measurements. Peak
process/device memory, warm execution and thermal stability remain unqualified.

Unadjusted predictions from different resolutions differ on the same crops:
the mean difference relative to the 518 output amplitude ranged from about
0.059 to 0.166. This observation has no pass/fail threshold or quality ranking.
It is not a backend-equivalence error, temporal experiment or ground truth.
It does show that interchangeable processing identities would conceal materially
different predictions. Automatic quality switching still requires explicit
alignment, cache/version policy and video qualification.

The separate `reference-depth76` full regression used source `0646083d` and did
not cover this follow-up. It started 430 targets, recorded 3,141 passing tests
and four original skips, but failed because 16 native targets exited before
tests with `STATUS_DLL_NOT_FOUND`. It is not a full pass. Its failures remain
retained independently; the new profile checks do not waive them.

Optional native WGSL admission still accepts only its explicitly implemented
518 plan/processing identity. Additional WGSL plans, measured capability and
quality selection, generation/cache checks, hysteresis, frozen export policy,
real-video transitions and sustained/cross-GPU qualification remain required.
No quality scheduler, UI integration or default model distribution is claimed.
