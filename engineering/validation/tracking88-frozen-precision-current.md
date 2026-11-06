# Photographed tracking: four-precision frozen PNG output

This checkpoint closes the bounded four-working-precision PNG-output check on
source `60f360c1801e2a9305666746d81edc9e3489e31e`. It changes no product code,
dependency, panel, style, numerical threshold or existing assertion. Issue #88,
continuous tracking quality and the complete 3D route remain open.

## Executed workflow

Three separate processes load the three already accepted photographed board
projects, including their original image bytes and managed COLMAP-format camera
documents. Inputs are read from the retained, hash-verified photographed
acceptance archive; no source image, calibration or camera solve is changed.

For each view, the current native host executes all four project working
precisions: Unorm8, Unorm16, Float16 and Float32. The public
`FrameFreezePlan::capture` captures immutable model/source state on the owning
Session thread. An actual public command batch then deletes all 40 live board
layers. The Session is dropped before a separate default Rust worker invokes
the frozen plan's exporter. Every exported result must retain the captured
revision, frame zero and 640-by-480 dimensions. Original project/asset bytes
remain unchanged; captured and subsequently emptied projects are retained.

All twelve exports succeed. This is the actual full-resolution frozen PNG
exporter, not the legacy MCP `render_preview` capture route, a mocked reply or
a desktop-window screenshot. The external probe uses the current desktop
logger and current workspace path dependencies, with no custom stack size,
backend override, layer exclusion, retry or production-profile change.

## Independent image checks

The reference images are the pinned independent OpenCV ray/plane oracles from
the original photographed qualification. They are fixed goldens; calibration
is not fitted again during this run. The original two-pixel boundary exclusion,
minimum checked-pixel counts and two-byte error bounds remain unchanged.

| View | Checked stable pixels | Stable board-interior pixels | Maximum RGBA/alpha error, each of four precisions |
| --- | ---: | ---: | ---: |
| `left03` | 282,516 | 63,526 | 0 / 0 bytes |
| `left08` | 285,275 | 54,396 | 0 / 0 bytes |
| `left14` | 286,158 | 49,918 | 0 / 0 bytes |

Independent linear composites over black and white also have zero error on
the same stable regions in all twelve cases, within the unchanged two-byte
bound. The input project, image, tracking document, independent image and each
actual PNG have separate hashes in the receipt. All twelve actual PNGs were
visually inspected in two contact sheets. Their checkerboard is a diagnostic
alpha backdrop; the original RGBA PNGs remain untouched. The contact sheets
use complete 640-by-480 images without cropping or resizing and retain the
input identities in `visual-review-layout.json`.

The original independent archive has SHA-256
`ca812bdf506d4808b11564bdaf1eaf55eb2ed5ec978c823e43566914d6a508ce`.
The current native executable has SHA-256
`a8b624cfabf20544a85e61419a18a5ce8ca1e2777c7c56a288bea92a67ebf06d`.
The source closure remains unchanged throughout compilation and the 39.328-second
runtime verification. Product stdout stays empty. Default-Info probe phases
identify Session creation and export after Session closure. Existing Vulkan
loader errors for absent Epic overlay manifests remain visible in the original
file/stderr output; no registry entry, layer setting or diagnostic is suppressed.

## Limits

Four working formats are converted by the existing explicit `SDR_CLIP` output
policy to 8-bit RGBA PNG. The bounded board colors are zero/one. This does not
prove HDR file export, unclipped highlights, 16-bit PNG, arbitrary format
precision equivalence or full-image equality at antialiased boundaries.

The three photographs are separately calibrated poses, not a continuous video
solve. This run does not execute COLMAP's reconstruction algorithm, establish
surveyed metric scale, repeat import/history/reopen or measured-corner fitting,
or repair the retained native full-regression access violation. Earlier import,
history and reopen records keep their original source boundaries. Final-head
cross-platform CI and complete regression remain separate gates. Android is
retired and excluded.

## Evidence

[Commands, probe source, copied inputs, live-state mutations, all twelve PNGs and receipts](data/tracking88-frozen-precision-60f360c1.zip):
2,770,406 bytes, 102 members, SHA-256
`c3aad312c8cfff7e99e515e1698b915c1b01fd90120aaae37aed4135f1cce92c`.
Every archive member was read back and verified. The adjacent JSON contains
the per-member manifest. It includes existing attributed OpenCV photographs
and independently authored board/oracle data, with their original identities;
no runtime binary, executable, model weight or new third-party asset is added.
