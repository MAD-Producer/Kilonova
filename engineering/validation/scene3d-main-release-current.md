# Maintained desktop model-pipeline release acceptance

Issue: [87](https://github.com/MAD-Producer/Kilonova/issues/87).
The issue was closed as completed on `2026-10-30T15:36:10Z`, after its updated
desktop-scope completion body and remote state were read back and verified.
Accepted main source: `49890ce0da7d7701eaec5e521d749b562d3ffd47`.
Completed release run: [41065552795](https://github.com/MAD-Producer/Kilonova/actions/runs/41065552795).

The source-bound implementation checkpoints remain listed in the
[issue acceptance map](scene3d-issue87-acceptance-current.md). This checkpoint
completes its later maintained-desktop release gate, including the merged
desktop model import, project library and MCP catalogue correction. The four
platforms below all passed the actual native unit-test step, desktop check,
pinned shared LGPL FFmpeg preparation, video-enabled release build, runtime
decode verification, release packaging and decode verification from the
extracted archive.

| Maintained platform | Actual job | Result |
| --- | --- | --- |
| Linux x86_64 | 124095828745 | All required native and release steps passed |
| Windows x86_64 | 124095828980 | All required native and release steps passed |
| macOS Intel | 124095829156 | All required native and release steps passed |
| macOS arm64 | 124095829270 | All required native and release steps passed |

The same run's Rust/GPU, dependency audit, libav and final release-publication
jobs passed. All eight jobs completed successfully. The raw logs, exact head,
per-step conclusions and ignored test rows are retained. Counts from overlapping
steps, platforms and historical checkpoints are deliberately kept separate;
ignored manual controls are not counted as passes.

## Accepted scope and retained limits

The maintained desktop scope covers owned glTF/GLB/OBJ assets, bounded material
and light rendering, instancing, skin/morph/imported pose animation, authored
and imported cameras, explicitly calibrated relative depth, and the documented
mixed plane/model/mask/matte/depth-of-field profiles. Independent normative
sample comparisons, negative controls and exact saved/frozen restoration are
source-bound in the existing acceptance map.

Android is retired and excluded at the requester's direction. Scene nodes,
ray tracing and volumes remain later optional work. General intersecting
transparency, unsupported nested/exposure/effect combinations, physical device
input qualification and arbitrary DPI remain subject to the existing documented
limits. The default 900x600 Project viewport limitation is retained; the accepted
narrow library controls use supported tab docking. No layout or hit geometry
was changed for this gate.

The sustained playback proof retains its controlled 32x32, 60-second scope.
It does not establish trained depth-model quality, GPU-provider inference,
zero-copy inference or high-resolution throughput.

Background prerendering is tracked separately in issue82 and draft PR575. Its
unmerged runtime, foreground P95 failures, native abnormal exits and current
local regression are outside this main-source acceptance. Earlier main-run
failures also remain failures: this later successful gate demonstrates the
identified source, without establishing the causes of intermittent prior exits.

## Reproducible record

Archive: [main release evidence](data/scene3d-main-release-49890ce0-v2.zip).
SHA-256: `9ba2834429db5346bf893e241892f27f17e55814d63faaccef3aec8b78536df3`.
All ten members were read back byte for byte. `assessment-v2.json` is the final
derived assessment. The initial ARM64 log transfer failed with an HTTP/2 stream
cancellation; only that read-only transfer was repeated through the job-logs
endpoint. No CI job or test was rerun. The initial derived record inaccurately
described an empty transfer file; v2 records that no first file was created and
retains the initial assessment and archive receipt. Actual job results and raw
logs are unchanged.

This record introduces documentation and CI evidence only. Product sources,
tests, assertions, UI, dependencies and third-party assets are unchanged.
