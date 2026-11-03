# #88 photographed calibration and native lens replacement

This increment qualifies a real photographed calibration example and its native
GPU replacement raster. It does not complete #88 or the broader 3D route.
The exact commands, source identities, terminal statuses and retained failures
are recorded in the accompanying closeout JSON and archive.

## Data and independent measurements

**PHOTO-1 / external acceptance assets.** Thirteen original 640x480 checkerboard
photographs from the official OpenCV 4.10.0 repository, pinned at commit
`71d3237a093b60a27601c20e9ee6c3e52154e8b1`, with unchanged upstream LICENSE and
COPYRIGHT. Authors, revision, license and modification scope are recorded in
ATTRIBUTION.md. The source manifest records every byte length and SHA-256.
No photograph or external runtime enters the shipped product or effect package.

**PHOTO-2 / disjoint validation.** The independently authored
`tools/tracking/calibrate_photographs.py` uses separately installed
opencv-python-headless 5.0.0.93 and NumPy 2.4.6. Ten views fit all five
Brown-Conrady coefficients and K. `left03`, `left08`, `left14` are excluded from
that calibration. In each held-out view, 27 alternating inner corners fit its
pose; the other 27 measured corners validate projection. A full-data or pose-fit
residual is not presented as independent validation. Training RMS is 0.4524 px.

| Held-out photo | Native projected RMS (px) | Native maximum (px) | Stable GPU pixels | Maximum byte error |
| --- | ---: | ---: | ---: | ---: |
| left03 | 0.183292 | 0.329759 | 282523 | 0 |
| left08 | 0.263480 | 0.564816 | 285253 | 0 |
| left14 | 0.213070 | 0.442629 | 286176 | 0 |

The board's physical square size is unpublished. A square is assigned one project
metre solely as a scale gauge, not as a measured physical dimension. These are
separate board poses in photographs, not continuous photographed camera movement
or a full camera-motion solver. Corner detection uncertainty, printed-target
imperfection and calibration error remain part of the measured residual.

## Functional change and guards

**PHOTO-LENS-1 / functional admission improvement.** The fitted lens has
`k1=-0.2530`, `k2=-0.1242`, `k3=0.4109` and nonzero tangential coefficients.
The previous absolute coefficient triangle bound is approximately 47.81 and
rejects this lens. The fast weak-lens proof is retained. A bounded fallback
encloses the signed radial Jacobian eigenvalues and displacement using outward
rounded interval arithmetic on the entire radial domain. It proves both a
contraction and that its disk maps into itself. This is a continuum enclosure,
not testing a finite grid for folds. The admitted derivative bound is 0.392989
and normalized radius is 0.880100. The 0.45 limit and 48 inverse iterations are
unchanged; no fitted coefficient is removed, reduced or clamped.

**PHOTO-LENS-2 / inverse and failure controls.** CPU tests compare 221 externally
inverted pixels including image corners and boundaries across all four rotations,
check conservative overscan, preserve exact distortion, and roundtrip the camera
project. A negative control has a locally small derivative but no enclosing disk;
it must fail. A valid neighboring lens then succeeds. Existing folding, numeric
range, crop, focal correction, incomplete raster and recovery controls remain.
The accepted inverse input radius is stored separately from the solution disk;
interaction does not assume the solution radius is always twice the input radius.

**PHOTO-LENS-3 / actual native rendering.** Forty authored spatial quads replace
the photographed board plane through the normal Scene/Renderer camera, depth,
offscreen and nonlinear output path. Actual readback is compared to separately
implemented analytic-Jacobian Newton rays using the externally fitted R/t and a
world-plane equation, without the production inverse or Scene camera matrices.
A separate OpenCV dense-ray check and annotated photo overlays are retained.
Both pixel checks keep the existing fixed two-pixel edge exclusion and maximum
two-byte tolerance. Measured-corner RMS <=1 px and maximum <=2.5 px are unchanged.
Projection relative to external projected coordinates remains within 0.0003 px.

The GPU test's video descriptor is a model fixture with placeholder source bytes.
The separate native executable gate imports each of the three original JPEGs
through MCP, verifies original SHA-256 and decoded dimensions, preserves the
tracking document bytes, binds its calibrated camera, and renders forty quads.
All 120 projected anchors agree with the external coordinates within 0.000064 px.
Invalid binding leaves the project unchanged, and binding undo/redo, save and
read-only reopen preserve the project. Default Info command starts, decoded
source identity, Error file and stderr, and JSON-RPC-only stdout are checked.
This verifies original image acquisition, not continuous original-video decode
or a source relink transaction. Earlier video/PTS evidence remains distinct.

**PHOTO-PNG-1 / native capture oracle correction.** Library readback and the
native SDR capture have different alpha conversion paths. The first native
gate incorrectly required hidden RGB in zero-alpha PNG pixels to match. It
failed on three pixels in left08; its report, controller and images remain
unchanged. PNG uses straight alpha, and zero alpha is fully transparent
([W3C PNG, sections 6.2 and 12.3](https://www.w3.org/TR/png-3/)).
The separately versioned gate independently checks alpha and keeps the same
two-byte raw RGB limit for every pixel with nonzero alpha. Only pixels where
both alphas are exactly zero have RGB canonicalized for the visible comparison.
It also compares linear composites over both black and white. No production
capture code, edge exclusion or numerical tolerance changed. All three views
have zero stable-pixel alpha, visible RGB and black/white composite error.
Hidden RGB differences remain explicitly reported: 0, 3 and 8 pixels;
left08 and left14 still have raw RGBA byte errors of 255. This is visible-output
qualification, not bit-for-bit PNG equality or a claim of an encoder repair.

## Failure history and acceptance

The exploratory download first used an annotated tag object rather than its
commit and failed before downloading assets. A Python shape-broadcasting mistake
and an OpenCV 5 API-name difference were corrected; their failed reports remain.
The first native GPU gate failed because the new test supplied engine world
coordinates where authored composition coordinates were required. The fixture
now explicitly converts the board's RDF axes to engine RUB, then to authored
composition coordinates. No production coordinate convention or assertion
tolerance changed. The original failure and passing source identities are kept.

The complete terminal gate table and archive identity are in
`data/tracking88-photographed-calibration-closeout.json`. Compilation alone is
not acceptance. The source-bound local run passes 245 model, 169 selected
renderer (16 lens and 153 regression), 485 core and 50 host-tracking tests:
949 passed, zero failed and one existing manual performance test ignored.
The FFmpeg desktop check, provenance check, separate external dense-ray oracle
and native executable MCP gate pass. This is not the full desktop/workspace
suite or exact-head cross-platform qualification. There are no panel,
interaction, layout, style or dependency
changes, and no cache, foreign worktree or previous evidence was deleted.

## Remaining #88 scope

- Actual source relink/replacement, undo/redo/reopen and warmed render-cache
  consistency with a bound camera, including equal-size wrong-source rejection
- Native solve/lens asset replacement transactions on an unchanged plate
- Built-in 2D/planar tracker prototype after the imported-data qualification
- Full maintained desktop/workspace regression and exact-head cross-platform CI

Vendor-specific adapters, full 3D camera solving, lens plus aperture, temporal
lens accumulation and the explicitly refused spatial effects/mask/matte paths
are separate follow-ups; this document does not automatically add them all to
#88's acceptance scope. Android remains excluded by user instruction. Earlier
compiler/native access violations and #82 strict P95/GPU-coverage failures remain
unresolved; successful changed-source gates do not repair those failures.
