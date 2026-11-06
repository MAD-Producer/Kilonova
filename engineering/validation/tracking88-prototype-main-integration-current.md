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
assertions or product code.

## Complete frozen-source run: failed, not waived

Attempt v3 executed the complete command below on unchanged
`270840919743570dad144ec3a7e609eac91fe1db`:

```text
cargo test --workspace --exclude motion-android --features motion-desktop/ffmpeg,motion-host/ai-depth,motion-desktop-media/ai-depth --no-fail-fast --locked -- --test-threads=1
```

It terminated with Cargo exit 101 after 8,415.297 seconds. Independent accounting
matched all 421 expected target invocations (411 native and ten documentation
targets), including duplicate target names in different packages: 4,934 passed,
one failed, 78 originally ignored, none filtered or measured. The previous
417-target result belongs to an earlier source tree and is not this run.

The sole failure was
`motion-core::format_version_boundaries::unsupported_versions_cannot_load_or_replace_an_existing_project`.
Its valid-project save inside the version loop failed at line 36, after the
unsupported-version load checks and before the later save/reload assertions:
Windows returned OS error 5, `PermissionDenied`, from the publication phase for
`project.json`, with `published: false`. The original test executable, PDB,
sources, cause and output were retained. No assertion, ACL, return value,
backend, retry policy or execution budget was changed.

One subsequent diagnostic executed the exact retained test binary with the
same temporary-directory configuration. That single test passed in 1.359
seconds including the controller. The cause remains unresolved: this does not
establish an antivirus diagnosis, a product repair or a passing full regression.

The [terminal evidence archive](data/tracking88-terminal-public-v1.zip) and
[manifest](data/tracking88-terminal-public-v1.json) retain the original three
attempts, complete outputs, source identities, exact target accounting, original
failure sources and same-binary diagnostic. Every member was read back:
716,242 bytes, SHA-256
`d45951c200762dbbdae075f435aaa73381fcb8d88a9f81ef1585d1a3ad50faa2`.
Retained executable/PDB and the separate complete private source archive are
identified by hashes rather than duplicated in this compact public archive.

Later main adds `video_retirement_export.rs` and `video_retirement_pressure.rs`;
neither exists in frozen v3. The actual merged source still needs its own
complete regression and final-head cross-platform CI. Photographed tracking
quality also remains pending. Previous source-bound successful runs and the
original heap-corruption/identity failure records are retained; integration or
a passing isolated diagnostic does not diagnose or repair them.

The separate [support and known-motion diagnostics](tracking88-support-diagnostics-current.md)
retain the original real-video failure. Neither these tools passes nor the
unchanged reference-depth results close issue 88 or complete the 3D route.
Android is outside the maintained acceptance scope.
