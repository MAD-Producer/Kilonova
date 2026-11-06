# Issue 86: photographed screen API checkpoint

This increment adds one native GPU regression for a manually aligned screen in
a real photograph. It does not change product rendering, panels, interaction,
the existing procedural screen assertions or the chroma-key sample. The source
baseline is `e4bb46506925fcdc714c82942458b8238a776409`, including main `ead8ce0c`.
The receipt hashes the added test, fixture and attribution files separately.

## Actual source and authoring

The previously attributed NASA Destiny laboratory photograph, `ISS007-E-11800`
dated 2003-08-03, contains the laptop screen used here. Its original SHA-256 is
`2efb4c28adab8e70ed026856ed0d6e4472ea7de4d0576f37a87150775fdf8c5f`.
The new test-only 960-by-654 RGB derivative has SHA-256
`e8ed893f990ba8850000064e153d57b84e6e1d5c0a0aa2ccc8dfd1254b2e379d`.
`source.json` records the author/source, exact revision, informational-use
guidelines, both identities, explicit sRGB interpretation and Lanczos resizing.
NASA / the ISS crew remain credited; this is not repository-MIT-licensed
photography, product artwork or NASA endorsement.

The four points `[168,60]`, `[219,74]`, `[205,116]`, `[154,106]` are manually
authored from that photograph before native execution. They are not fitted
to rendered pixels, detected by a tracker or surveyed physical geometry.
The replacement RGB gradient is independently authored test content.

The initial editable project registers both PNGs and places the photograph
as its background layer. An actual public JSON edit batch adds the insert,
applies the existing `corner_pin` effect and animates opacity from 0.35 to 0.9.
The composition clock is exactly 30000/1001. The insert's clip starts at frame 5
with offset 5, so composition keys 5/15 must be stored at local keys 0/10.

## Independent pixels and state persistence

An independent f64 eight-equation correspondence solve maps output pixel
centres to source UV. The reference bilinearly samples source color, composites
the animated opacity over the actual photograph in linear light, then applies
the sRGB transfer. It does not call renderer helpers, the analytic WGSL inverse
or Scene sampling to obtain expected pixels.

The budget is declared in the fixture before native execution: maximum error
three units per RGBA channel, a two-pixel screen-edge exclusion and at least
1,500 checked insert-interior pixels. Background pixels are also checked; this
is not a handful of selected successful colors. The fixed qualification excludes
1,054 pixels while the insert is active. No assertion or threshold was adjusted
after execution.

| Composition frame | Checked pixels | Checked insert interior | Maximum channel error |
| --- | ---: | ---: | ---: |
| 5 | 626,786 | 2,031 | 1 |
| 7.5 | 626,786 | 2,031 | 2 |
| 15 | 626,786 | 2,031 | 2 |
| 0 | 627,840 | 0, outside clip | 0 |
| 26 | 627,840 | 0, outside clip | 0 |

The five samples deliberately include fractional time, out-of-clip states and
nonmonotonic seeks. The animated outputs must differ over at least 1,500 pixels.
One undo restores the exact initial project and source photograph; redo restores
the edited project and byte-identical native output. Saving and opening a fresh
Session preserves the model and all five exact native RGBA results.

Three frozen PNG plans capture frames 5, 7.5 and 15. After deleting the live
insert, closing both Sessions, dropping the live renderer and removing both
PNG source files, each plan must export exactly the previously captured pixels
and revision. All three pass. This exercises actual frozen source ownership,
not mocked output or a saved PNG substituted for rendering.

## Commands and observed result

```text
cargo test -p motion-desktop-media --test professional_screen_photo --no-run --message-format=json --features ffmpeg --locked
```

The source-hashed link passes in 30.515 seconds, including controller overhead.
The exact executable is then run once with `--test-threads=1 --nocapture` and
`MOTION_PHOTOGRAPHED_SCREEN_ARTIFACTS` pointing to the retained output directory.
Its SHA-256 is
`af753c8f78d970ec0a37e47b099d88d727e6ed5f4e5529471db042111bb32f6b`.
Native execution passes **1 test, zero failures, zero ignores, zero filtered**
in 26.375 seconds including observation overhead. All source hashes and the
executable remain unchanged through execution. The actual source/cropped three
animation samples and full frame-15 output were visually inspected. The crop
review uses an explicit threefold nearest-neighbor diagnostic enlargement;
original output PNGs remain untouched.

The test also runs through ordinary Cargo with the actual GPU feature:

```text
cargo test -p motion-desktop-media --test professional_screen_photo --features ffmpeg,gpu-tests --locked -- --test-threads=1
```

This second command documents reproduction; it is not an additional executed
run or test count. Original backend, stack, product profiles and default Info
configuration are preserved. Controlled build caches and temporary files are
on E:, in the branch's dedicated target. No cache or worktree cleanup is used.

## Remaining boundaries

This is static photographed alignment with animated display opacity. It does
not accept automatic tracking, camera-motion footage, reflection/light matching,
photographed temporal quality, HDR or other working precisions. The enlarged
review visibly retains staircase edges; antialiased edge quality is excluded
from this checkpoint, not declared fixed. The procedural projective negative
control and earlier photographed key/holdout evidence keep their separate scopes.

Complete maintained regression, unresolved historical native process failures,
final-source cross-platform CI and the other issue 86 requirements remain
unaccepted. A new passing fixture does not replace those failures. Android is
retired and outside the user's maintenance scope.

## Evidence

[Link/run commands, source closures, actual input/output/goldens, public edits,
projects, frozen PNGs and visual review](data/professional86-screen-photo-v1.zip):
19,521,179 bytes, 38 members, SHA-256
`36bbc25c29f46fb412e7127323251217abf0806ed433650016877c4c601bab79`.
All members were read back and verified. The adjacent JSON retains the complete
member manifest. Fixture-local credit, ATTRIBUTION, PROVENANCE and NOTICE record
the test-only derivative's source and modification scope. No executable, runtime
binary, model weight, Adobe content or product release asset is included.
