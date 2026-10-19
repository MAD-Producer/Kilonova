# Icon geometry and triangle clipping allocation verification

## Scope and ownership

This increment addresses the `icons::draw` and triangle-polygon temporary allocation sites in issue #129. It does not close that umbrella issue. The isolated `perf/ui-icon-geometry` branch starts from main `be0432559cb9bf26f9833397cc2f9283773005f1`. Recent branches and open PRs were checked before implementation: #343 handles dock constraints, #344 handles layer render sampling, and #345 handles text bounds and text allocations. None implements this geometry change. Their branches and the root worktree were preserved.

No panel, style token, icon source, asset, dependency, shortcut, interaction or serialization file changes. Existing SVGs and their tessellation are retained. This is an allocation optimization with rendering compatibility tests, not new error observation coverage or a UI redesign.

## Changes and retained costs

- Cache each icon's normalized SVG viewport clipping in the existing immutable registry. Drawing transforms that cached mesh directly into one exactly sized owned triangle vector and appends its shape to the destination list. It no longer creates a child paint list or invokes list-wide clipping per icon.
- Preserve `Shape::Triangles`, aspect fitting, colors, painter order and host clipping. Retain the original empty output when floating-point coordinates collapse a positive icon edge to a zero-width viewport.
- Clip each generic triangle using two fixed stack buffers instead of allocating a polygon and four intermediate vectors. Each clipping plane emits at most twice its input count, so capacity `3 * 2^4` is a conservative bound, including degenerate/nonfinite input. Intersections, plane order and output triangulation are unchanged.

One owned output vector remains per icon. Generic list clipping still allocates its result, maps and replacement shape/color lists. Cold registry compilation and growing destination-list capacities still allocate. This does **not** meet the whole issue's zero-per-control-allocation objective or establish an application frame-rate improvement. Eliminating retained public triangle ownership and the other list allocations requires further work.

## Failure reproduction and regression evidence

The two new allocator tests failed against unchanged main production code, with assertions left intact:

| Measured operation | Original main | Optimized path |
| --- | ---: | ---: |
| One warmed Star icon | 14 allocations | 1 allocation |
| 200 warmed Star icons | 2800 allocations | 200 allocations |
| Clip one crossing triangle | 11 allocations | 6 allocations |
| Clip 200 identical crossing triangles | 1006 allocations | 6 allocations |

The allocator forwards to `System` and counts only the measuring thread. Registry initialization and destination paint-list capacity are warmed outside the icon measurement. Triangle inputs are prepared outside the clipping measurement. The public clipping operation is measured, including its output and bookkeeping allocations; the test also verifies that each repeated triangle produces the same nonempty geometry. Evidence: `artifacts/issue-129-icons-before.log` and `issue-129-icons-after.log`.

A CPU reference retains the original clipping algorithm and compares output float bits for 5000 deterministic random triangles plus edge, degenerate, nonfinite and incomplete-triangle cases across three rectangles. It passes both before and after the implementation. The GPU regression likewise passes against the original and optimized implementations: every registered icon, five DPI ratios (1, 1.25, 1.5, 1.75, 2), and zero, one or two host clips. It includes non-square fitting, fractional coordinates, mixed alpha, viewport-crossing geometry and interleaved opaque overlays. The legacy path still compiles the original raw mesh and clips after transforming it. A real GPU adapter/device is required, with no skip or silent-success fallback.

All fifteen saved original/optimized icon grids have exactly identical RGBA pixels; dimensions, hashes and equality results are retained in `artifacts/issue-129-icons/pixel-comparison.json`. These are actual offscreen GPU controls, not full-editor screenshots or physical input automation. Captures and references are under `artifacts/issue-129-icons/before/` and `after/`. GPU logs: `issue-129-icons-gpu-before.log` and `issue-129-icons-gpu-after.log`; CPU logs: `issue-129-triangles-before.log` and `issue-129-triangles-after.log`.

Review caught one compatibility regression in the intermediate optimization: positive size at extreme coordinates could previously have an empty rounded viewport, whereas the new direct path emitted a degenerate shape. A new regression failed on that intermediate version and the explicit viewport check restored empty output. Evidence: `artifacts/issue-129-icons-collapsed-before.log`. No existing assertion was changed to accept a different shape representation or hide a failure.

## Final acceptance

The full UI suite passed: **106 passed, 0 failed, 0 ignored**, including all five new regressions. The framework example build passed. A second comparison of the final implementation's fifteen grids against original main also found identical pixels; evidence is `artifacts/issue-129-icons/final-pixel-comparison.json` and the captures under `final/`. Logs: `artifacts/issue-129-icons-ui-tests.log` and `issue-129-icons-framework-build.log`.

The standalone framework passed all four English/Chinese and default/override-style combinations in real main and floating windows, including numeric undo, gesture ownership, Tab guards and layout round-trip. The override verifies successful reload and invalid-reload rejection. The inspected adapter is NVIDIA GeForce RTX 4070 Laptop GPU, Vulkan. These probes inject normalized events and do not claim physical input or whole-editor acceptance. Reports/captures are under `artifacts/issue-129-icons/native-default/` and `native-override/`; logs are `issue-129-icons-native-default.log` and `issue-129-icons-native-override.log`.

The full desktop suite also passed: **593 passed, 0 failed, 1 existing native-audio test ignored**, in 326.27 seconds. Combined with UI, the result is **699 passed, 0 failed, 1 existing ignore**. Evidence: `artifacts/issue-129-icons-desktop-tests.log`. After the interrupted observation, the completed test summary and absence of a live Cargo process were verified; the test was not restarted.

Provenance passed for 1453 tracked paths after staging the tests and this record. The panel style guard, new-test formatting and whitespace checks passed. The panels diff is empty.

Commands use the portable MSVC environment and `CARGO_TARGET_DIR=E:/Dev/aem/target`:

```text
cargo test -j 1 -p motion-ui --locked -- --test-threads=1
cargo build -j 1 -p motion-ui --example framework --locked
cargo test -j 1 -p motion-desktop --locked -- --test-threads=1
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-icons/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-icons/native-default
python tools/ci_ui_framework.py --binary E:/Dev/aem/artifacts/issue-129-icons/bin/framework.exe --output E:/Dev/aem/artifacts/issue-129-icons/native-override --style desktop/ui/styles/example.override.json
python tools/validate_provenance.py
rustfmt --edition 2021 --check desktop/ui/tests/icon_geometry.rs desktop/ui/src/icons/geometry_tests.rs desktop/ui/src/paint/triangle_clip_tests.rs
git diff --cached --check
```

## DESIGN.md and remaining issue disposition

All twelve §0.7 baseline rows are retained with reason: no path row steps, columns, control heights, numeric backgrounds or boolean presentation changes; no effect row heights, boolean buttons, enum buttons, hover alpha or indentation changes; no timeline gutter coordinates or snap-guide color changes. Existing presentation is not certified compliant by this work. The §5.2 row grammar, hit areas, styles and menus are unchanged. No context-menu acceptance is needed because no menu changes.

Issue #129 remains open for borrowed input routing, the remaining geometry/list allocations, text cluster editing and other unverified subitems. Text allocation work is separately proposed in #345; combining these changes must be distinguished from this branch's own test results. Issues #126 and #134 are not certified complete by reducing UI geometry allocations.
