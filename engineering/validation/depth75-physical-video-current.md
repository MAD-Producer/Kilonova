# Physical indexed video placement and independent rendering (#75)

Status: desktop physical-unit authoring and ownership integration verified with
authored samples. This does not qualify a learned model, automatic quality tier,
continuous cross-quality inference or the complete 3D roadmap.

## Functional change

`Session::physical_video_depth_placement_command` authors one staged original
video interval. The decoder supplies PTS and interval end; the actual prepared
RGB and independently current source/object guards remain mandatory. Metric and
inverse-metric camera-Z samples retain their declared units. This route creates
neither relative anchors nor interpolation across missing source intervals.

The existing image-only physical placement command keeps its behavior. Both
commands share physical placement preparation and authored reference-camera
capture. The video command follows the existing relative video author's ability
to author an uncovered interval before rendering that interval can succeed.
Ordinary indexed-track history, quotas, source verification, scalar spill and
frozen rendering are reused. No renderer or model-format change is required.

## Source-bound controls and retained failures

One owned serial controller ran at a time, with locked/offline Cargo commands and
unchanged source until each original terminal result was consumed. Existing
assertions, ignores, budgets, deadlines and thresholds remain unchanged.

| Controller | Complete target results | Disposition |
| --- | --- | --- |
| `depth75-physical-video-controls-v1` | Session:36 passed; video:26 passed,2 failed | The new fixture configured an absent storage directory. Both original failures remain recorded. |
| `depth75-physical-video-controls-v2` | Session:36 passed; video:27 passed,1 failed | After creating storage, the authoring test passed. The frozen test attempted to remove the source root while its anonymous scalar files still lived there; Windows returned file-in-use error32. This is retained as a fixture ownership failure. |
| `depth75-physical-video-controls-v3` | Video:28 passed; host library:295 passed,1 original ignore | The fixture now owns a separate existing scratch directory until its frozen scalar planes are released. All original assertions remain. Session is not rerun. |

The production diff and all tracked test files are identical across these three
controllers. Only the new video test fixture changed. The complete Session
target's36 passes from v2 therefore remain applicable; the enclosing v2 Cargo
command remains failed. Together the three applicable complete targets establish
359 distinct passes and one original ignore, not a whole-workspace pass.

V3 source is base `519bd5ae5454259d1919545c58623c7f96807bbf`, tracked diff SHA-256
`86c1cb5a670c3ce1ac231fac34a3129969e27f5998a36a75e79b8f7c00eebc31`,
and new video test SHA-256
`690c755942bf164ce815dab0de3a7077db9a044f0bc031e0df3c443af69697f1`.
Original session45061 exited0; terminal chunk`efd69f` was consumed before source
freeze release. Raw controllers, source diffs, logs, receipts and product-file
hashes remain under private `artifacts/pr-queue/depth75-physical-video-controls-v*/`.
The v1 and v2 original failed terminals were also consumed and preserved.

## Actual exercised boundaries

Both physical representations exercise absent/relative/stale result and wrong
object refusal, decoder-owned VFR boundaries, nonmutating command preparation,
ordinary undo/redo and preservation of the other indexed samples. An independent
GPU renderer then uses a serialized/restored frozen snapshot after Session
closure and removal of the original source directory. Three original PTS
intervals retain their respective decoded colors, actual physical occlusion and
identical pixels within each VFR interval. A distinct frozen source copy and its
index remain required; the snapshot does not claim to own an entire movie's RGB.
Primary cancellation is preserved, and a decoder reporting the wrong actual PTS
is still refused despite valid physical units.

The backend is an explicitly authored analytic control. No actual learned-model
quality, requested/resolved production profile, automatic transition admission,
thermal stability or complete-device peak measurement follows from these tests.
The [original failure and remaining acceptance record](depth75-status-current.md)
continues to apply. No model/runtime/dependency acquisition, Android validation,
panel, layout, interaction, compiler/backend workaround or inference retry is
included. Published-source CI for `519bd5ae` predates this video increment and
cannot cover it.
