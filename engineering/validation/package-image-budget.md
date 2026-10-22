# Configured effect-package PNG capacity (issue #61 R6)

Status: implemented candidate, not compiled or executed yet. The separate
`5a2dfaa4` integrated admission matrix does not accept this later source. Its
frozen validation continues in the original worktree; this candidate does not
modify that checkout or start a competing compiler in its build directory.

## Functional change

Package validation previously checked three independent hard-coded 128 MiB PNG
limits. `PackageReadLimits` now accepts an explicit 4-byte..2-GiB decoded image
allowance. Editor and effect images share a checked cumulative total of unique
resource paths. Refusal occurs after metadata inspection and before decoding,
with a native error retaining package ID, resource path and required/budget bytes.
Invalid metadata, malformed PNGs and shader failures keep separate identities.

Session installation passes the current `decodedImageBudgetBytes` policy through
the input loader and the install-time directory refresh. Standalone APIs retain
their conservative default and offer explicit-limit variants. The default and
maximum constants now have one definition shared by effects and rendering.
`effect_tool pack`, `check` and `glsl` accept an optional
`--decoded-image-budget-bytes N` argument; failed packing does not publish output.

Runtime package texture uploads and host `plugin_pixels` also check the current
per-image CPU policy before decoding. A prior successful import is not authority
to allocate under an older, larger policy. Resident native textures retain their
existing lifetime semantics. Frozen renderers consume their own captured policy;
this requires actual PNG/movie regression evidence below before acceptance.

The package-validation total is conservative RGBA8 capacity across unique PNGs
in one package, including editor images. Runtime reads apply the same configured
allowance per decoded image. Neither scope is total codec/process memory or
physical VRAM. Encoded archive/file, shader, device and dimension limits remain
separate; this change removes the unrelated fixed decoded-PNG cutoff.

## Required source-bound acceptance

| ID | Actual evidence required | State |
| --- | --- | --- |
| R6-P1 | Editor/effect PNG aggregate, exact cutoff and shared-path deduplication | Pending |
| R6-P2 | Generated 8192x4097 PNG: default refusal, configured pack/load above 128 MiB, actual complete pixels | Pending |
| R6-P3 | Actual registry install refusal preserves input/registry/publication; configured reload recovers | Pending |
| R6-P4 | Invalid policies and corrupt PNGs retain non-capacity failure identity | Pending |
| R6-P5 | Actual pack/check subprocesses consume the option; rejection preserves prior publication and stdout | Pending |
| R6-P6 | Runtime in-memory decode checks the current policy and preserves native codec causes | Pending |
| R6-P7 | Actual native Dither resource refusal reports CPU scope/package/path; restoration matches independent reference pixels | Pending |
| R6-P8 | Actual registered Session install/pixel APIs obey policy changes without authored/revision/undo or source-byte changes | Pending |
| R6-P9 | Actual Engine install command refusal/recovery reaches default Info start, Error file/stderr, original reply and stdout isolation | Pending |
| R6-P10 | Frozen full-quality PNG/movie package-resource policy, source-bound broad regressions and provenance | Pending |

No new dependency or third-party asset is introduced. Test images, HTML and
manifests are independently generated; existing attributed packaged resources
are reused only through the repository's normal public loader. Existing test
assertions, product layout/styles/interactions and protocol responders stay
unchanged. Android restoration and new Linux/Metal execution remain unaccepted.
