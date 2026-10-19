# Issue 129: glyph atlas lifetime repair

Baseline: main `452f659d6c92ebfe60554b21bd26f45e3829f5da`.
Scope: issue #129, section 2. This is a functional text-rendering fix, not an observability-only change or closure of the umbrella issue.

## Failure and implementation

Previously, a full atlas called `clear()` in the middle of command generation. Earlier glyph commands retained normalized UVs pointing at pixels that had been cleared or reused.

The atlas now grows geometrically without moving existing texels. Cached entries are renormalized against the new dimensions. Positioned glyphs record the atlas dimensions used to produce their UVs; the painter converts these UVs against the uploaded texture dimensions. This also preserves cropped glyph UVs and commands appended from another PaintList. Hand-authored glyph commands can use `atlas_size: None` for UVs relative to the current uploaded texture.

Shelf allocation checks bitmap width and height, including padding, before writing pixels. Invalid and extreme rasterization sizes are rejected before calling the outline/platform rasterizer. Failed allocations preserve existing entries, texels and cursor position.

## Capacity and observation boundary

Production growth is bounded to 2048 x 2048 RGBA texels (16 MiB of coverage data on each of CPU and GPU). This fits downlevel desktop texture dimension limits. Growth does not evict existing glyphs or clear in-progress commands. Transient CPU allocation during growth also includes the old bitmap; GPU resource retirement follows wgpu submission lifetime.

At the hard capacity or for an oversized glyph, the new glyph keeps its advance but has no bitmap. Invalid nonpositive/nonfinite sizes produce no geometry or advance. Refused glyphs are not inserted into the cache. Existing text remains intact. This is a bounded, observable refusal, not a claim that arbitrary glyph workloads always fit or that eviction/paging has been implemented.

`text.atlas_full` now describes actual capacity/rasterization refusals instead of routine growth. It retains the existing atlas-owner lifetime counter: first refusal logs Warn, occurrence 60 logs Error, subsequent occurrences are suppressed. Replacing the atlas resets the counter; successful glyphs and `clear()` do not reset it. The existing default-level observation test retains every assertion; its fixture now imposes a tiny capacity ceiling to exercise real refusals rather than the removed clear/repack policy.

## Regression evidence

- Real installed-font tests force a small atlas to grow, verify old cached glyph bitmap bytes and metrics, and rasterize a glyph larger than the initial atlas in both dimensions.
- Capacity and integer-overflow tests verify refusal preserves texels, entries and shelf cursor; clearing for an explicit scale/lifecycle change permits a later valid glyph.
- NaN, infinity, zero, negative and extreme sizes do not reach unsafe atlas writes.
- A real GPU readback test fills the normal 1024 x 1024 atlas until it grows. It compares previously emitted and clipped text pixel-for-pixel, appends and draws new text, and checks GPU validation errors at pixel ratios 1, 1.25, 1.5, 1.75 and 2.
- The GPU test additionally renders stale normalized UVs without dimension conversion and requires different pixels. This guards against accidentally passing by comparing empty/uniform captures.
- Optional `MOTION_UI_ATLAS_EVIDENCE` exports before/stale-UV/after GPU PNGs; these are rendering evidence, not screenshots of an interactive desktop session.

Validation commands and final results are recorded below. Initial unrestricted parallel compilation exhausted Windows commit/page-file capacity (OS error 1455); the replacement run limits compilation to one job and keeps the full test selection.

## UI and remaining issue scope

No panel files, layout, control behavior, tokens or stylesheet values are changed. DESIGN.md section 0.7 panel remediation rows are retained outside this text-rendering scope, not claimed fixed. Section 12's full interactive workspace/menu/IME acceptance is not claimed by offscreen GPU tests.

Issue #129 remains open for the other independent framework defects and their acceptance requirements. Issues #126 and #134 are unchanged by this patch.

## Completed validation

On Windows with the repository's portable MSVC environment, after the implementation above:

```powershell
. E:/Dev/aem/.tools/msvc-portable/env.ps1
$env:CARGO_TARGET_DIR = 'E:/Dev/aem/target'
$env:MOTION_UI_ATLAS_EVIDENCE = 'E:/Dev/aem/artifacts/issue-129-atlas'
cargo test -j 1 -p motion-ui -p motion-desktop --locked -- --test-threads=1
```

Result: exit 0; desktop 593 passed, 0 failed, 1 pre-existing ignored native-audio test; UI 92 passed, 0 failed, 0 ignored (49 library tests and 43 integration tests). All five new regressions execute without a missing-font or missing-GPU skip. No existing assertion was removed or weakened.

Local evidence:

- `E:/Dev/aem/artifacts/issue-129-atlas-tests-serial.log`: full successful run
- `E:/Dev/aem/artifacts/issue-129-atlas-tests.log`: initial parallel compilation failure, retained separately
- `E:/Dev/aem/artifacts/issue-129-atlas/atlas-{ratio}-{before,stale-uv,after}.png`: raw GPU readbacks at five ratios
- `E:/Dev/aem/artifacts/issue-129-atlas/atlas-1.5-comparison.png`: inspected comparison composed on a dark background with explanatory captions; the middle panel deliberately disables UV conversion and is not a screenshot of the old clear/repack implementation

`git diff --check`, `tools/validate_provenance.py` and the panel style-consumer guard passed. No files under `desktop/app/src/panels/` changed. The generated PNGs and build logs remain local evidence rather than product assets.
