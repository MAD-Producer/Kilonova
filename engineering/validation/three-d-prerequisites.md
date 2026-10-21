# 3D pipeline prerequisite review

Review baseline: `159e9810207b61cfe2d59e98c23f16cf8ca36bde` (2026-10-20 UTC).
The first increment was integrated with main
`a14b4b518f507e7dc9a2758e14bea0d8bd46e076`; source-bound results and remaining
limits are in [preview-target-budget-completed.md](preview-target-budget-completed.md).
This record describes source coverage and staged work, not completion of
[#87](https://github.com/MAD-Producer/Kilonova/issues/87).

## Dependency order

Complete the relevant resource-budget foundation in
[#61](https://github.com/MAD-Producer/Kilonova/issues/61), then semantic formats in
[#72](https://github.com/MAD-Producer/Kilonova/issues/72). Build the execution DAG
in [#78](https://github.com/MAD-Producer/Kilonova/issues/78) and the independent
depth-resource contract in [#74](https://github.com/MAD-Producer/Kilonova/issues/74)
on those foundations. Review the additional linked prerequisites (#60, #67 and
#73) before claiming that any of these issues is complete.

Static model import, scene rendering and 3D nodes follow this work. Do not add
placeholder nodes or mark #87 complete merely because a file can be parsed.
ONNX inference is a later optional host capability; metric camera depth must not
depend on an AI model or treat an uncalibrated relative map as physical distance.

## Verified existing coverage and remaining gates

| Issue | Existing main coverage | Work still required before closure |
| --- | --- | --- |
| #61 | `resource_policy.rs` has configurable source/scratch allowances; `resource_error.rs` distinguishes refusals; `preview_memory.rs` has four bounded levels, cooldown and recovery; host preview retains the last submitted target; texture/submission ledgers expose logical allocations | Account for the combined frame peak, including caller-owned targets and overlapping submissions; support trustworthy platform pressure inputs with safe fallbacks; complete cross-platform pressure, seek and export acceptance. Existing counters are not driver residency measurements |
| #72 | Masks use R8; effect scratch has selected RGBA16Float accumulators; existing working-space/alpha conversions are explicit | Resource semantics and format/usage/filter/blend negotiation; end-to-end optional float across nested, blend, matte, adjustment and output; versioned portable consumers and legacy-pixel equivalence |
| #78 | `motion-graph` is now merged: persistent typed authoring documents, checked DAGs, groups, shader generation and real msfx export. `EffectFramePlan` and native preview caching are already used | A separate typed execution IR and frame resource/task DAG; dependency and lifetime planning; precise fusion boundaries; diamond sharing, format/capability diagnostics and grouped/ungrouped GPU equivalence. Authoring DAG validation alone does not complete this issue |
| #74 | No `DepthResource` or inference runtime was found in the reviewed main sources | Define relative/metric/inverse-depth semantics, direction, calibration, validity/confidence, UV mapping and source identity; then separately implement optional, cancellable ONNX inference with licensed weights and stale-result rejection |

The old issue descriptions use the October 11 baseline. Their statement that
`motion-graph` is absent no longer describes this main revision. Existing work
must be retained and tested rather than reimplemented.

## First #61 increment: preview target admission

Before this change, `PreviewFrameTargets::prepare` checked a fixed 64 MiB per
target, while retaining a separate front target. Source/scratch policy settings
could not limit their combined capacity. Reusing an allocated candidate also
needed to respect a subsequently lowered runtime allowance.

The new `previewTargetBudgetBytes` policy field bounds front plus candidate
capacity before allocation or replacement. Its default is 128 MiB, preserving
the previous two-target maximum; omitted fields in older requests use that
default. The existing 16-megapixel per-target contract remains in place.
`memoryPolicyVersion` becomes 3; the packed render-plan ABI is unchanged.

An unrendered host placeholder starts at one pixel so the minimum eight-byte
two-target policy can attach without exhausting its budget before preview
degradation begins. A rejected request preserves the submitted front. The host
uses its existing bounded retry/cooldown policy, and formal capture does not
inherit the preview-only allowance or resolution.

This increment is **not a total GPU memory cap**. It covers the logical capacity
of one preview target pair. Buffers, decoder allocations, swapchain images,
effects, exposure tasks, other renderers and driver-retired allocations remain
separate. Dropping an old application target does not prove immediate physical
GPU retirement. Admission and retained-target accounting use the actual color
format capacity, including eight-byte RGBA16Float targets. This does not enable
an end-to-end float composition/capture chain; that remains part of #72.

## Coordination and acceptance boundaries

Open branches were reviewed before starting: #373 owns camera-lens settings;
#379 owns path-node tools; #380 owns model editing invariants; #382 owns frame
bundle allocations; #383 owns vector upload reuse/GPU diagnostics; #384 owns
frozen-reader isolation. This increment changes the shared policy and preview
target admission only. No camera, geometry, panel, node or material code changes
are included, and none of these open branches is overwritten.

Use native GPU pixel tests for retained-frame preservation, real refusal,
smaller-frame recovery, bounded exhaustion and unchanged formal capture. Test
legacy JSON defaults, invalid-policy atomicity and policy propagation through
host/editor and frozen export. Record actual commands/results below after they
complete; compilation alone does not satisfy acceptance.
