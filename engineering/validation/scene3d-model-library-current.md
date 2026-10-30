# Desktop model library integration

Local source-bound acceptance completed against `f325bfe54a3da4540fe5533a5287d78348f8763a`.
The full desktop run passed 1,392 tests with zero failures;
26 existing/manual controls remain enumerated, not passed.
The four-case model-library screenshot control ran explicitly and passed
separately. Sixteen focused controls overlap the full run and are not added
again. Published-head cross-platform CI remains a separate gate.

## Library registration and source ownership

`workflow::library_media::Request::Model` enters the existing serial
`ProjectAsset` handler, which checks the captured document/composition before
opening the source. `workflow::model::register` shares the bounded importer,
exclusive frozen bundle publication and cleanup used by composition import.
The registration and captured `ProjectItem::Model` folder placement form one
undo transaction. Registration creates no layer and does not require a spare
layer ID. An absent destination folder is refused before opening the source.

The normal media picker includes glTF, GLB and OBJ. Project-panel file drops,
including floating project windows, use library registration. Main and floating
composition drops retain explicit layer creation. These are implemented source
routes; physical operating-system file-drop injection is not yet evidence.

`Request::PlaceModel` resolves the existing immutable source through the
session's prepared model catalog. It selects the imported default scene or
scene zero, preserves the rest pose and 100 pixels/metre convention, and adds
one independent editable 3D layer. Repeated placement shares the original
asset and writes no duplicate bundle. Each placement has its own undo boundary;
undoing a layer does not undo registration or delete the owned source file.

The existing model asset schema has no user-authored source label. This
increment does not invent one: placement names use the stable asset ID, while
the initial composition import retains the source stem. Automatic bounds fit,
animation selection and imported-camera activation remain separate decisions.

## Executed controls

- `engine::model_library_tests::model_library_registration_and_repeated_placement_have_independent_undo_boundaries`:
  three source formats, captured folder, unchanged layers during registration,
  deleted originals, undo/redo, repeated shared placement, exact reopen and
  actual red native pixels
- `engine::model_library_tests::model_library_refusals_and_superseded_requests_preserve_document_and_owned_files`:
  invalid folder before source IO, superseded cancellation, missing identity,
  invalid frame, unchanged state/file count and explicit recovery
- `shell::project_import::tests::model_project_drop_registers_only_in_the_captured_folder`:
  production project-drop action, three formats, captured folder despite
  browser navigation, no implicit layer and one-step registration history

These controls supplement the existing import suite. Their actual commands,
outputs, source/configuration hashes and counts are retained in the verified
archive below. This increment does not enable background prerendering.

## UI boundary

The requester explicitly approved the model-library panel exception, including
the corresponding tests. The reviewed patch is retained outside the worktree at
`target/silent-failure-validation/model-library-routing-proposal.patch` and
passed `git apply --check` before application. It is now applied: existing model
rows offer Place in composition and can be dragged to a timeline track. The
implementation reuses the existing menu, drag geometry, tokens and cancellation
semantics. No new layout, style value or hit tolerance is introduced.

Two new drag controls cover time mapping, exactly one placement action,
cancellation and stale-document rejection. A native child-process control,
`panels::project::tests::native::models::model_library_native_menu_and_placement_reuse_owned_sources_in_both_languages`,
executed library registration, the actual model context menu and
rendered placement at 1600x1000 and 900x600 in both languages. The 1600 capture
uses the default workspace. The 900 capture first retains the default layout,
then tabs Effect Controls with Project using existing workspace docking so
the model rows have a viewport. It also checks
source deletion, unchanged shared asset identity, separate placement undo/redo,
actual red composition pixels, a default-level missing-source Error, and
a second shared placement through the production pointer-release route.
The drag preview is captured before release, and its independent undo/redo
boundary is verified afterward.
All four native cases passed and the actual screenshots were visually
inspected. They use controlled production Input with a 1x fixture projection;
they do not claim physical OS input injection or every system DPI. The 1600
cases produced 13,713 red composition pixels; the retabbed 900 cases produced
3,272. The shared-asset, source-deletion and independent-history checks passed.

The first native checkpoint passed both 1600 language cases but failed at
900x600: the unchanged default Project dock has zero item viewport height,
so the model row cannot be clicked. Its screenshot, panic hook/backtrace and
test output remain in `model-library-regression-v1`. The replacement fixture
uses the supported tab-docking operation above while preserving all original
menu, pixel, drag and history assertions. This does not fix or accept the
default minimum-window layout limitation. No production layout/style/hit
geometry is changed; that existing limitation remains pending separately.

Following the existing native screenshot fixtures, the four-process capture
is an explicitly run manual control (`--ignored --exact <name>`). The checkpoint
must execute it and retain its results; an ignored row in a standard test run
does not count as a pass. The library ownership and drag controls remain in
ordinary CI. This avoids adding another repeated screenshot study to every
platform's default unit-test run.

All existing DESIGN section 0.7 remediation rows are retained: this increment
does not change the path/effect controls, timeline gutter or snap-guide styling.
Those unrelated violations are not reported as fixed.


## DESIGN section 0.7 disposition

The following locations are byte-for-byte unchanged by this increment. Each
row is retained with reason: outside model-library routing, no visual
remediation attempted. Existing violations are not silently marked fixed.

| Baseline item | Status and reason |
| --- | --- |
| `path_controls.rs` six vertical steps | Retained; no path-control changes |
| `path_controls.rs` two-column split | Retained; no field layout changes |
| `path_controls.rs` mixed button heights | Retained; no control geometry changes |
| `path_controls.rs` custom numeric background | Retained; no numeric paint changes |
| `path_controls.rs` full-width boolean | Retained; no boolean changes |
| `effect_controls.rs` row height and steps | Retained; no effect-list changes |
| `effect_controls.rs` boolean buttons | Retained; no effect control changes |
| `effect_controls.rs` full-width enum | Retained; no enum presentation changes |
| `effect_controls.rs` manual hover alpha | Retained; no hover color changes |
| `effect_controls.rs` hierarchy | Retained; no effect hierarchy changes |
| `timeline.rs` label gutter coordinates | Retained compatibility baseline; no coordinate changes |
| `timeline.rs` snap guide color | Retained compatibility baseline; no styling changes |


## Verified evidence and limits

[Executed import/library checkpoint](data/scene3d-model-library-native-v1.zip),
SHA-256 `1874928d8d4c1fa7fd9d36452e5ef7f4a869599e673c69e07d2f739348708c12`.
All 138 entries were read back and verified,
including the sixteen menu/registration/placement/drag captures, the retained
initial narrow-window failure and default file/Error-stderr evidence.

The separate imported-model source `960e5bdb5d7e31a298efda2a87bfa317adcbca45`
passed 1,387 desktop tests and 136 host tests, native editor rendering and an
actual desktop-binary MCP stdio/save/reopen control. That stdio control produced
20,000 red pixels, removed the original model/MTL and reopened exact native
pixels. Its default Info start, file Error, Error stderr and JSON-RPC-only
stdout were verified. It is evidence for that exact import source, not a claim
that the later library binary was run through the same stdio script.

The current library source separately passed full desktop tests, desktop check,
style-consumer validation and provenance. The older complete maintained-workspace
checkpoint is [documented separately](scene3d-maintained-workspace-current.md);
its counts must not be presented as a full-workspace run of this newer source.

No Android, automatic bounds fitting, automatic animation choice, arbitrary DPI
or physical OS input injection is accepted here. The default 900x600 Project
viewport limitation remains unfixed. No third-party asset, dependency or Adobe
content is introduced; the triangle and screenshots are independently authored.
