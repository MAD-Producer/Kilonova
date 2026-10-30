# Issue 87 mixed-composition acceptance disposition

This checkpoint completes the evidence inventory for the original issue's
mixed 2D/3D, parent/camera/matte, depth-occlusion and existing BSP acceptance
cases. It does not close issue 87, prove the pending desktop import increment,
or claim every combination in the compositor is supported.

## Source and actual execution

Nineteen existing controls are matched to their actual passing execution
lines and current source. The native related gate is the accepted 1041-test
aperture checkpoint. Its `head` records main `e4d6e142` before the uncommitted
implementation was published; its complete source hashes, rather than that
base commit label, are checked byte-for-byte against published head `594e810d`.
The original plane BSP controls additionally ran in the successful Rust/GPU
[job 123355344568](https://github.com/MAD-Producer/Kilonova/actions/runs/40846551127/job/123355344568)
for that exact published head. PR 558 merged as `ed1a84a4`; its complete tree
was independently verified equal to `594e810d`.

This is a source and execution audit, not nineteen newly run tests. No counts
are added to the related gate, independent normative checkpoints or the
separate full local workspace. The raw CI log, job/run identities, native
execution log, recorded source hashes and relevant actual test sources are
retained. An initial auditor incorrectly assumed the related run's base commit
was its final implementation commit and stopped before auditing; the retained
correction binds the recorded implementation hashes to the published source.
No product source, test assertion or acceptance tolerance changed.

## Original acceptance cases

| Case | Implemented and executed behavior | Evidence family |
| --- | --- | --- |
| Plane/mesh depth | Fragment occlusion in both authored orders, independent camera/parent world rays, four precisions at 1x/4x, immutable pending and abandoned frames | `model_layers/project_shared::mixed_planes` |
| Imported camera and parents | Parent rig, child hierarchy, layer clock and observer selection; nested composition FPS and exposure receiver clocks | `model_layers/imported_cameras` |
| Matte and physical depth | Four calibrated matte modes apply coverage before depth; an ordinary receiver reads the independent calibrated source after camera edits; shared sources feed ordinary and calibrated consumers in four precisions | `depth_surface/calibrated_mattes`, `ordinary_calibrated_mattes`, `shared_matte_inputs` |
| Unrelated ordinary 2D layers | Masks, matte inputs and external-input effects keep their independent source/effects rasters; unrelated blends retain authored order in both blend spaces | `depth_surface/composition_boundaries`, `composition_effects`, `composition_blend` |
| Existing BSP plane results | Independent projected-depth/alpha rays across camera/effect/order cases, split perspective UV and unchanged vertex ABI, homogeneous camera-plane clipping, rotated spatial matte projection | `projected_planes`, `layer_compositing` |
| Bounded aperture integration | Saved model aperture with exact disabled-aperture recovery; animated focus on actual calibrated near/far geometry in four precisions | `model_layers/depth_of_field`, `depth_surface/depth_of_field` |

Test paths are relative to `crates/motion-render/tests`; the archive's
`accepted.json` gives the full nineteen test names and their individual source
hashes. The world-ray and old BSP controls compute geometric/coverage expected
values independently; merely comparing one altered implementation against
itself is not the only evidence. The source eligibility branch retains the
ordinary pure-plane BSP path when no model, calibrated geometry or active
aperture requests shared geometry.

## Boundaries retained explicitly

The original acceptance requires the implemented mixed/parent/camera/matte
cases to preserve their semantics and ordinary BSP results. It does not
specify a universal compositor, arbitrary optical model, order-independent
transparency or every combination of nested/exposure/effect inputs. The
[detailed inventory](scene3d-composition-boundaries-current.md) records those
boundaries separately. In particular:

- General intersecting translucent geometry retains the sorted-alpha limit
- Calibrated shared geometry across unproven nesting, adjustment, scene-level
  effects, external inputs or exposure is not claimed implemented
- Positive aperture with blended geometry or an orthographic camera is refused
- Packed/legacy consumers refuse positive aperture before caller-buffer writes
- The supported pointwise material/mask/matte profile is not general material support

These are limits of the delivered profile, not newly implemented combinations.
An explicit refusal is not a rendered result. Any later promise to support one
of these combinations needs a functional change and its own independent pixel
and ownership controls. Android is retired and excluded; scene nodes, ray
tracing and volumes remain later optional work under the original issue.

## Remaining gates and archive

The original mixed-composition evidence inventory is now reconciled. The full
local maintained workspace is still running; desktop model import still needs
actual regression, native UI and standalone MCP proof, latest-main alignment
and its own published-head CI. Background prerender issue 82 remains separate
and incomplete. This document does not turn any of those gates green.

Archive: [source and execution audit](data/scene3d-composition-disposition-v1.zip),
23 entries, 267049 bytes, SHA-256
`7c34dd892cdea2ee65818374d64748250a0c48527fbc1681e1a36c9439bcc146`.
CRC and every member were verified after writing. Original upstream model
assets and binaries are not bundled.
