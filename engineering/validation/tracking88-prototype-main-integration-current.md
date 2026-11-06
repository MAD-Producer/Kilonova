# Tracking prototype: current-main integration

The optional prototype is integrated with maintained main
`26c1d2d2bc83c8073d85a200a7cc78cd6284d4f4` and the imported-camera/professional
workspace delivery `b25fa736bf9bfef169776a3151fef12ddfb894ae`.
Source `81ea22aaf15464f9491e3dd24e709c28a5c848de` adds the frozen prototype
tools and native Surface controls to that combined base. It keeps the current
rational-clock implementation and both branches' provenance. The new Surface
fixture initially passed `FrameRate::integer(125)` to the integer-only
`Project::new` constructor. Complete native compilation exposed this type error;
the fixture now passes `125`, which the constructor validates and converts to a
rational clock internally. Its assertions, sample times, pixel tolerances and
original media/solve bytes are unchanged.
No panel, product layout or interaction changes are included.

## Fresh tools and actual CV execution

All six phases completed with exit zero on the fixed source above, in 42.187
seconds including the actual CLI video cases. Recorded source hashes remained
unchanged throughout the execution.

| Complete phase | Actual result |
| --- | --- |
| Provenance guard | 6,042 tracked paths passed |
| Validation inventory | All tools classified; original runtime checks retained |
| Full `tools/test_*.py` discovery | 123 passed, one existing environment-dependent skip |
| Complete optional `tracking/test_flow.py` | All eight actual CV controls passed |
| Actual encoded synthetic-video CLI | Both tracks retain 84 automatic observations; point maximum error 0.164875 px against 0.75 px, plane maximum 0.021855 px against 1 px |
| Actual manual-absence/failure-recovery CLI | Authored absence stays quiet; sustained automatic failure escalates once at occurrence 60 and resets after manual recovery; file/Error stderr/stdout isolation passed |

The skipped tools case requires `MOTION_EFFECT_VALIDATOR` to run the Rust
schema/WGSL loader. Its unchanged environment condition was retained rather than
reported as a passing execution. Native Rust coverage remains a separate gate.

The fresh synthetic video has its own container identity,
`ee8596fa7bee0aa58aa7fa99e249885bf38184c3e1aca41c286e8c3eb1c85754`.
It does not replace the original frozen native test fixture. Both the new
encoded inputs and measured solves are retained in the [tools archive](data/tracking88-prototype-integration-tools-v1.zip)
and [manifest](data/tracking88-prototype-integration-tools-v1.json), together with
all tools sources, controllers, original outputs and the native harness delta.
Every member was read back: 3,231,743 bytes, SHA-256
`60432fb33be7574ccbad6d908eb2cd5beab767cbef6ec22c6bd675c4ec2444ab`.

## Combined depth route and remaining gates

The later integration `be8cde26e296b6b452456ef5bc0cc68205ff9966` also includes
the exact reference-depth branch
`e64c63f12e54088b9ff393d7b422101c50eaae26`. Its unchanged model/native source
provides explicit registered depth mixing and the independently checked
masked moving-occlusion controls. The earlier tools execution above is not a
new full execution on this later combined source.

The complete maintained workspace, with FFmpeg and optional AI-depth features,
still requires a successful full regression on this combined source. The
original 10 GiB free-space start gate remains in force. After the filesystem
confirmed sufficient space, attempt v1 stopped before tests because its shared
temporary directory had disappeared during external cleanup. Attempt v2 used
a dedicated temporary directory within the protected build target. It completed
compilation far enough to expose the fixture type error above, then exited 101
after 665.687 seconds with no test invocations. Its recorded source remained
unchanged at `e010a034503c821ee441753bbae778f0f2d841d4`. Both original logs and
controller receipts remain under `artifacts/pr-queue/tracking88-prototype-workspace-integration-v1`
and `artifacts/pr-queue/tracking88-prototype-workspace-integration-v2`; neither
attempt counts as passing test coverage. The fixture correction changes no
assertions or product code and requires a new complete run.

Current combined native compilation, complete workspace execution, final-head
cross-platform CI and photographed prototype quality remain pending. Previous
source-bound successful runs and the original heap-corruption/identity failure
records are retained; integration alone does not diagnose or repair them.

The separate [support and known-motion diagnostics](tracking88-support-diagnostics-current.md)
retain the original real-video failure. Neither these tools passes nor the
unchanged reference-depth results close issue 88 or complete the 3D route.
Android is outside the maintained acceptance scope.
