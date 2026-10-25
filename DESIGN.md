# Motion Studio — UI Design Specification

> **Version: V4.0** — full English translation of V3.2; scope narrowed to Windows desktop only (Android content removed). Supersedes V3.0–V3.2.1.
>
> **Scope**: UI appearance, native drawing layouts, and consistency with existing interactions only. **Goals**: improve density, hierarchy, alignment, and operation feedback.
>
> Applies to the existing repository `E:\Dev\aem`. This is **not** a greenfield project. Do not use this document to restructure the product architecture, rendering architecture, data model, or keyboard routing.
>
> **Inspect the corresponding repository files before implementation.** File paths and line references are drawn from the supplied project audit. If the code has changed, the current repository implementation takes precedence.
>
> **Related**: [desktop/ui/STYLES.md](desktop/ui/STYLES.md) · [docs/ui/pr7-12-acceptance.md](docs/ui/pr7-12-acceptance.md) · [desktop/design/Motion-Studio-UX.md](desktop/design/Motion-Studio-UX.md) · [desktop/ui/icons/README.md](desktop/ui/icons/README.md) · [desktop/KEYBOARD-SHORTCUTS.md](desktop/KEYBOARD-SHORTCUTS.md)

---

## 0. Scope and highest-priority rules

1. Motion Studio is an existing **Windows-native** Rust motion-graphics editor. It uses the in-house `motion-ui` / `PaintList` drawing system and retains 21 historical panel IDs and five workspace presets. Masks and expressions are now property-owned; 19 panels remain available. This specification targets the **Windows desktop UI only**.
2. The authoritative UI styling source is `desktop/ui/styles/dark.json`, loaded using `include_str!` in `desktop/ui/src/style.rs`. Reuse existing tokens for colors, sizes, fonts, and component states. Do not introduce CSS `:root`, CSS Variables, WebView, React, Vue, Electron, Tauri, Node, or a parallel theme system.
3. Consult `desktop/ui/src/ui.rs` and `desktop/ui/src/paint.rs` for native controls/drawing, and `desktop/ui/src/workspace.rs` and the existing dock implementation for layout and docking.
4. Existing shortcuts, undo, subframes, snapping, preview dragging, workspaces, and serialization contracts **outrank this specification**. Only their visual presentation is in scope. Never silently change behavior for visual consistency.
5. Do not invent colors or dimensions incompatible with the existing system. If a new semantic token is genuinely needed, search `dark.json` for an equivalent first. Only then update `dark.json`, its style access paths, and tests before referring to the new token here. **Never represent temporary hardcoded values as existing tokens.**
6. Descriptions of current behavior here come from the user-provided repository audit; unverified details must be inspected in real code. **Report conflicts first** instead of overriding the implementation with this document.

### 0.7 Known existing violations — visual remediation baseline

> Recheck every row against current code. Line numbers may shift.

The following are existing issues reported from repository inspection — **not** completed fixes, and **not** evidence that the corresponding tokens already exist. Track each item through `before → changed location → screenshot/interaction verification`. Do not globally replace unrelated hardcoded values.

| Existing location | Known current behavior | Required acceptance outcome |
|---|---|---|
| `desktop/app/src/panels/path_controls.rs:860,884,953,1002,1070,1149` | Six literal `*y +=` steps: `30/34/36/30/28/28` | Audit meaning and actual height of each row; establish consistent vertical rhythm using existing size styles where possible; do not assume every control has one identical height. |
| `path_controls.rs:893` | Hardcoded two-column split `(area.width() - 30.) * 0.5` | Both fields follow verifiable shared column sizing and do not overflow narrow panels. |
| `path_controls.rs:894-953,1038` | Mixed button heights `26/24/22`, unlike field height `18` and row height `22` | Normalize by button semantics, alignment, and usable hit areas; do not break existing pointer precision merely to equalize measurements. |
| `path_controls.rs:1124,1130` | Custom `paint::rect(SUNKEN)` plus `colors.ACCENT` bypasses `field.number` | Prefer existing native numeric-field rendering and state behavior while preserving numeric editing semantics. |
| `path_controls.rs:1151-1170` | Boolean represented as an `area.width()-24.` full-width button | Use a compact, clearly stateful boolean control; preserve the underlying value and event routing. |
| `effect_controls.rs:958,1043,1050` | Row height `30`, `26px` stepping, and `*y += 34.` | Align property rows with the density and hierarchy system; test scrolling and clipping. |
| `effect_controls.rs:998-1013` | Boolean displayed as text buttons `fx` / `—` | Show unambiguous boolean state; avoid confusing it with an effect enable/delete action. |
| `effect_controls.rs:1053-1064` | Enum displayed as a full-width `area.width()-48.` button | Use a compact, recognizable dropdown control without losing selection functionality. |
| `effect_controls.rs:974` | Hover background manually created using `.with_alpha(0.2)` | Remove `.with_alpha(0.2)`; use existing `dark.json` `$colors.hover` (`#303030`) via the existing `hovered` state mechanism. Do not add `hover.weak` or another color token. |
| `effect_controls.rs` (entire file) | No visual `indent`/`depth` hierarchy | Add visible hierarchical indentation and expansion hit areas; do not render all tree levels flat. |
| `timeline.rs:541-547` and the 11-column label gutter | Existing hardcoded column coordinates | **Compatibility baseline**: preserve coordinates and behavior. If theming is desired later, first create validated layout parameters and regression tests. No blanket refactor required here. |
| `timeline.rs:1432-1436` | Snap guide uses hardcoded `Color::rgb(255,210,60)` | Retain and document current behavior. Any later theme change requires first defining and wiring a token in `dark.json`, then validating drag feedback. |

**Acceptance loop**: first address visible violations in `path_controls.rs` and `effect_controls.rs`, including the Masks/Path screenshots; second, clean up styling provenance; only then consider unused constants unrelated to visual defects. Report each item as `fixed` / `retained with reason` / `pending verification`. **Never hide existing literals behind the claim that `dark.json` is already the sole data source in implementation.**

---

## 1. Visual goals and anti-patterns

**Target character**: dense, restrained, legible, precise professional motion-design software. Take inspiration from After Effects workflows and information hierarchy **without copying Adobe branding, icons, or trademarks**. This is a creation tool, not a web dashboard.

**Prohibited patterns**:

- One property consuming three or four rows; labels, numeric inputs, and keyframe controls separated; excessive blank space stretching control intervals.
- Wide banner buttons used for booleans (for example a full-width `Mask enabled` button). Use a compact, explicitly stateful Checkbox/switch.
- Mixing `Add`/`Invert` with `Pen Mask`/`Rect Mask` as undifferentiated, oversized flat buttons.
- Default HTML-style controls, rounded dashboard cards, decorative whitespace, ornamental gradients, or excessive borders.
- Inconsistent field widths, colors, row heights, or icon states for equivalent properties in different panels.
- Replacing a working UI architecture with another; simulating existing operations using fake buttons or text.

**Priority**: (1) correct semantics/behavior → (2) alignment/density → (3) hierarchy/readability → (4) discoverability → (5) decoration.

---

## 2. Single authoritative visual source: `dark.json`

Do not create a CSS `:root` token table or a second theme constants layer. Mappings below reflect the supplied audit; always verify the actual values in `dark.json`.

| Design meaning | Existing token/source | Recorded value |
|---|---|---|
| Application background | `$colors.app` | `#1D1D1D` |
| Panel background | `$colors.panel` | `#232323` |
| Raised surface | `$colors.raised` | `#242424` |
| Panel header | `$colors.header` | `#1C1C1E` |
| Primary text | `$colors.text` | `#D1D1D1` |
| Secondary text | `$colors.muted` | `#A6A6A6` |
| Disabled text | `$colors.disabled` | `#858585` |
| Focus / accent / playhead | `$colors.focus` | `#61A8F5` |
| Keyframe | `$colors.keyframe` | `#E3D94D` |
| Hover | `$colors.hover` | `#303030` |
| Selection | `$colors.selected` | `#304559` |
| Inset surface | `$colors.well` | `#171717` |
| Body font size | `$sizes.font.body` | `12` |
| Small font size | `$sizes.font.small` | `11` |
| Field height | `$sizes.field.height` | `18` |
| Track row height | `$sizes.row.height` | `22` |
| Panel header height | `$sizes.panel.header.height` | `24` |
| Control corner radius | `$sizes.radius.control` | `2` |
| Popup corner radius | `$sizes.radius.popup` | `4` |
| Visible splitter grip | Hardcoded `desktop/ui/src/dock.rs:310-313` | `4.0` |
| Splitter drag hit band | Hardcoded `desktop/ui/src/workspace.rs:203-205` | `6.0` |

- `metrics::SPLITTER = 4.0` in `desktop/ui/src/theme.rs:88` is **unused** according to the audit; changing it does not change the actual splitter. `RULER(82)` and `PROPERTY_ROW(90)` in `theme.rs` are likewise reported unused. Do not advertise them as active visual control points.
- Do not unconditionally invent `accent-strong`, `playhead`, or `layer-label-*` tokens.
- The snap guideline is currently hardcoded as `Color::rgb(255,210,60)` in `timeline.rs`, not read from `dark.json`. Moving it into the theme must be a separate, tested change.
- **Typography anchors**: body `12`, small `11`; do not indiscriminately raise all text to 13+ or reduce it to unreadable 9px. Account for measured text, ellipsis, baseline alignment, and **Windows DPI scaling**.

---

## 3. Native drawing and layout invariants

- `PaintList` produces visible drawing. A hit area **must not be smaller** than its visible control, but **may be intentionally larger** for pointer precision. Enlarged hit regions must not overlap actionable hit regions of adjacent controls. Focus, scroll clipping, IME, and hovered/pressed/disabled feedback must correspond to the real interaction target.
- **Separate logical dimensions from physical pixels.** Window scaling must not mutate logical control geometry; convert at drawing time. Align 1px separators to the device-pixel grid under existing conventions to avoid blurry lines.
- Reuse native fields, rows, and headers; use common margin, row-spacing, and baseline calculations rather than arbitrary new constants per panel.
- **Preserve existing generous hit tolerances**: Bézier handle visible radius 4 / hit `18×18` (`graph_handles.rs:197`); keyframe diamond visible radius 4 / hit width 10 (`timeline.rs:1875,1227`); graph-box handle per-axis tolerance ≤6px (`graph_box.rs:205`). Node sockets share nearest-point picking for hover, press and release: preserve the existing 9px hover extent and expand it to cover the visible outer diamond at high zoom (`node_editor/view/wires.rs`, `socket_at`). Do not shrink hit regions in the name of density.
- When width is insufficient, first shrink flexible value regions, then truncate names with a full-text tooltip, then use existing scrolling/folding. **Never let controls overlap or clip a close button.**
- Separate scrollable content from fixed top tool rows and bottom status bars. Unused panel space should simply remain background, not be filled with oversized empty controls.
- **A new visual grouping is not a new feature.** Groups, alignment, compact rows, and helper text change presentation only; they must not create a second state model.

---

## 4. Application shell and workspaces — preserve the implementation

| Region | Existing size/layout | Source |
|---|---|---|
| Title bar | `24` | `desktop/ui/src/theme.rs` |
| Menu bar | `24` | `desktop/ui/src/theme.rs` |
| Toolbar | `32` | `desktop/ui/src/theme.rs` |
| Total shell height | `80` | `desktop/app/src/panels/mod.rs` |
| Status bar | `20` | `desktop/app/src/panels/mod.rs` |
| Default window | `1600×1000` | `desktop/app/src/main.rs` |
| Minimum window | `900×600` | `desktop/app/src/main.rs` |
| Default dock region | `(0,80,1600,900)` | Default window and shell |
| Upper section / timeline | `56.8%` / `43.2%` | `dock.rs` |
| Left / right columns | `17.7%` / `20%` (default view) | `dock.rs` |

Splitter resizing uses `dock::clamp_split` with the existing logical-pixel minima (`metrics::MIN_DOCK` horizontally and `metrics::MIN_TIMELINE` vertically). Do not add a percentage floor on large windows. When the parent cannot fit both minima, retain the existing 45% per-side limit. Drawing and hit testing must use the same division geometry; saved ratios strictly between zero and one remain valid. A rejected split request must leave the tree unchanged.

There are exactly **five** existing workspace presets. Preserve IDs, behavior, and dock-tree structure from `desktop/app/src/workspace/presets.rs`.

| ID | Display name | Existing layout |
|---|---|---|
| `default` | Default | `dock::editor_layout()` |
| `animation` | Animation | Default tree with top `linked` split at `0.535` |
| `nodes` | Nodes | Bottom `0.27` → Right `0.32` → Bottom `0.43` |
| `drawing` | Drawing | Bottom `0.25` → Left `304/1600` → Right `286/1292` |
| `all` | All Panels | Bottom `0.35` → Right `0.25` → Left `0.25` |

- Workspace persistence already uses `root`, `floating`, `hidden`, and `window_geometry` in `desktop/ui/src/workspace.rs`. Preserve docking, floating, hiding, and geometry serialization.
- Backquote temporarily expands the dock under the pointer to the main content area; when the pointer is outside the docks, use the focused visible main-window panel. Repeat the command to restore the full layout. Keep the dock's tab stack, ownership and split ratios intact. This is a transient display projection and must never enter saved layouts or project history. Text/IME, popups and active gestures take priority. Workspace changes and panel show/close/float/redock restore the full layout first. Native floating windows already contain one full-size panel; Backquote leaves their geometry and the main layout unchanged.
- Changes to panel internals are allowed; do not replace workspace state with a React-style tab container or parallel layout state.

---

### 4.1 Project working precision

File → Project Precision opens a compact project-wide settings dialog. Its one
inline enum distinguishes 8-bit integer (UNORM8), 16-bit integer (UNORM16),
16-bit float (Float16) and 32-bit float (Float32). It applies to every composition
in the project and must not appear as a per-composition setting or an application
preference. State that preview and PNG/video output use SDR; floating working
precision does not imply HDR output.

Reuse native dropdown, popup, button and text styles with existing notice,
preferences, spacing and row tokens. Choosing an item changes only the draft;
Apply sends the existing project command as one history edit, and Cancel/Escape
has no project effect. An open dropdown owns input until closed; its Enter,
Escape and outside click must not activate the dialog's footer. Respect IME,
repeat and focus-loss guards. Verify English/Chinese, minimum/default window
sizes and controlled 1/1.5/2 pixel ratios with actual GPU screenshots.

## 5. Consistent panel visual grammar

### 5.1 Visual hierarchy

1. **Panel title bar** — panel title and existing dock actions; height `$sizes.panel.header.height = 24`.
2. **Context/action toolbar** — compact filtering, creation, and relevant commands; never mistake an oversized full-width action for a group header.
3. **Object/group row** — collapsible/selectable, with existing deletion, enable, menu, or other controls. Each icon needs a meaningful state and tooltip.
4. **Property row** — stable state/animation control + label + value editor; 2D values use aligned X/Y fields on one row.
5. **Visual states** — hover, selection, editing, keyframe, and focus alter appearance **without changing row height**.

### 5.2 Seven-slot property-row grammar

Visual layout only — does not define new shortcuts or data fields:

```text
[depth indent] [expand/collapse ▾] [track color ■] [type/animation icon] [name] [value / X Y] [trailing control]
```

- These seven slots describe **semantics**, not a requirement that every row fill every slot. Inapplicable slots may have zero width or reserve an alignment position. Indentation comes from actual `depth`. Only expandable items receive disclosure arrows. Neither a color swatch nor an icon replaces hierarchy indentation. Reuse the existing color swatch (`timeline.rs:962`) and `Icon::Fx` (`effect_controls.rs:905`) according to their real meanings.
- Timeline property subrows follow their own existing layout contract (§7); do not override them with this generalized panel rule.
- Camera property rows use `$sizes.camera.row.height = 28` and `$sizes.camera.field.height = 26` to retain the existing numeric hit height while placing vector components inline. Their columns come from the shared inspector layout; timeline camera rows retain the timeline row token.
- Numeric values use right alignment and a tabular-numeral appearance. Preserve existing numerical formatting precision. Keep unit placement (`%`, `°`, `px`) consistent; **never split one property's label and value across opposite corners of a tall row**.
- For 2D properties use two equal-sized X/Y slots in the value region. At narrow widths follow actual minimum control sizes and scroll rules without silently changing dimensionality.
- Do not repeatedly draw a long decorative animation line across the panel for every property. Distinguish panel property editing from timeline keyframes. If an existing mini-track is needed, integrate it into a defined row and distinguish selection/playhead/keyframes; it must not consume multiple empty rows.
- **Booleans must visibly communicate on/off.** Plain text and broad stateless buttons are insufficient.
- Distinguish color wells, numbers, dropdowns, and icon buttons by actual control semantics. Separate destructive actions such as delete from routine editing controls.

Curve-kind properties may disclose an inline graph detail below their single caption row. Keep its channel selector above the graph; this exception is for the graph editor, not for splitting scalar, boolean, enum, color, or vector fields across rows.

Expressions are contextual child details, not a standalone dock panel. Keep the property itself on its existing row. The source editor is collapsed by default, including for saved expressions. Right-click a property and choose Edit expression, or Alt-click its stopwatch, to open a single child row immediately below that property. Put the enabled checkbox in the label column and the source field in the value/time lane; reuse `row.height`, `icon.slot`, and `field.number`. Idle sources stay transparent; focused editing uses the existing field state. Enter commits through `SetExpression`; Escape or the close icon discards the uncommitted draft and collapses the detail. Closing the editor never deletes the expression; Remove expression is a separate context action. Toggle only `enabled`, preserving source, target, seed, and profile. The timeline row plan must reserve the detail before rendering or hit testing so following rows cannot overlap. Legacy panel ID 9 remains readable but is redirected to Properties and omitted from panel menus/presets.

Animatable rows expose a leading previous-key / current-key / next-key rail using three complete `icon.slot` hit regions and the existing icon button states. The centre is selected only at an authored key and disabled for locked/observing targets, unsupported parameters, or non-integral edit times. Missing neighbours disable their individual arrows. Keep this rail inline; it does not replace the stopwatch, add another row, or reduce existing field hit tolerances. Nested inspector hosts retain their existing tree origin; timeline rails remain before the property-tree branch. Values follow the reserved rail without overlapping the stopwatch, including narrow mask panels. Navigation changes view time only and does not write undo history; toggles use the existing typed key commands and one atomic edit.

### 5.3 Component-state matrix and precedence

**Legend**: `✓` required visual behavior; `○` when supported by the component's semantics; `—` not applicable. Disabled visuals must not be overridden by hovered/pressed states. **This is an improvement target, not a claim that every state is already implemented.**

| Component | default | hovered | pressed | focused | selected | disabled | invalid |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| Numeric field | ✓ | ✓ | ○ | ✓ | ○ | ✓ | ✓ |
| Text field | ✓ | ✓ | ○ | ✓ | ○ | ✓ | ✓ |
| Button | ✓ | ✓ | ✓ | ✓ | ○ | ✓ | — |
| Boolean switch / Checkbox | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Dropdown / enum | ✓ | ✓ | ✓ | ✓ | — | ✓ | ✓ |
| Tree node / group row | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Icon action | ✓ | ✓ | ✓ | ✓ | ○ | ✓ | — |
| Keyframe / timeline marker (no style layer; §7.1) | — | — | — | — | — | — | — |

**Exceptions**

- Keyframes/timeline markers are drawn directly, e.g. `paint::diamond([x, y + row_height * 0.5], 4.0, colors.KEYFRAME)` (`timeline.rs:1228,1865`), **without** the `dark.json` component-style resolver. Their visual and interaction contract is in §7.1; `—` in this row does not mean no interaction feedback.
- An enum's selected value is **application data**, not a selected field-style state. Existing `field` exposes `hovered` / `focused` / `invalid` / `disabled` only; do not add a `selected` token merely to populate the matrix.

**Style-reading baseline** — counts reported from JSON inspection of the current `desktop/ui/styles/dark.json`:

- `styles`: **28** style keys
- `tokens.colors`: **34** color tokens
- `tokens.sizes`: **30** size tokens

These are the audit baseline, **not permission to assume missing keys exist**. Verify inheritance and state precedence against the current `dark.json` and `desktop/ui/src/style.rs`. The proposed sequence — `extends` → component state → `disabled` last → `invalid` overriding `focused` border — **must be validated in source and must not be asserted as existing behavior before verification**. Any genuinely new key/token must first be added to `dark.json`; never establish a second style system in paint code.

### 5.4 Reduced Motion and UI accessibility

The audit reports four motion-duration tokens (`90/90/100/140ms`) plus `motion.easing` in `dark.json`; verify exact paths and consumers. **Do not rely on browser `prefers-reduced-motion` CSS.** Read the Windows reduced-motion/accessibility preference, or offer an app setting, and map that to a native UI `reduced_motion` semantic setting.

- When reduced motion is enabled, decorative translation, scaling, and spring transitions should become `0ms`. Only essential, brief, interruptible opacity-state transitions may remain; if the user's setting requests no animation, switch immediately.
- **Never suppress the animation authored by the user** in the timeline, preview, or export. This setting applies only to editor UI transitions and must not change keyframes, project data, or rendered content.
- If OS preference integration does not yet exist, mark it not implemented and test manual mode changes. Do not claim automatic system preference detection.

### 5.5 Density and empty states

- Use field height `18`, content row height `22`, and panel header `24` as density anchors. Increase a specialized control's height only for a clear interaction reason.
- Pack content contiguously from the panel top. Blank space after the last item is normal; **never stretch row spacing just to fill a panel**.
- Maintain common left baselines and value-field edges for labels, rows, groups, and controls. **Fix layout before changing theme tokens.**
- Empty states should provide short, actionable guidance, not oversized promotional cards.

---

## 6. Screenshot-specific remediation: Masks and Path

This is the first visual acceptance target. Preserve actual Masks/Path functionality; do not change rendering algorithms or the project model.

### 6.1 Masks panel

**Observed screenshot problem**: `Mask (1)`, `Pen Mask`/`Rect Mask`, `Mask 1`, `Add`/`Invert`, Opacity/Feather/Expansion values, keyframe controls, and `Mask enabled` are vertically scattered and semantically flattened.

**Required information order**:

1. **Fixed header** — `Masks` with existing Dock action.
2. **Compact mode toolbar** — `Pen Mask` / `Rect Mask`, preserving existing selection state; responsive widths, not two oversized banners.
3. **Mask object row** — disclosure, `Mask 1`, enabled state, delete; selected row receives `selected` background. Preserve the app's deletion confirmation behavior.
4. **Mask settings group** — compact controls for mask mode and inversion. `Add` is an enum/mode value; `Invert` is a boolean. **Never render both as visually identical full-width buttons.**
5. **Opacity** — label, relevant animation control, and numeric field on one row; preserve precision and keyframe behavior.
6. **Feather X/Y** — label, animation control, and aligned equal-width X/Y value slots on one row.
7. **Expansion** — follow the same property-row grammar.
8. **Mask enabled** — use a compact `Checkbox` to display the existing `mask.enabled`, on the mask object row or directly adjacent to its settings. Preserve the existing `MaskAction::Options { enabled: Some(!mask.enabled), .. }` (`path_controls.rs:1173-1179`). **No full-width bottom button, no new model, no modified Action contract.**

**Caution**: check current names, actual control count, and keyframe hit regions in source. Do not remove functionality solely because the screenshot layout is poor. Preserve mask paths, modes, inversion, feather, expansion, opacity, and enabled status.

### 6.2 Path panel

> **Screenshot observations, not verified code facts.** The supplied Path screenshot suggests inconsistent hierarchical indentation, broad gaps between labels and values, and Repeater row-density issues. The visual hierarchy of `Composite copies: below` and `Edit path with Pen` warrants inspection. Their actual drawing functions and line locations are **not yet verified**. Locate the real Path renderer before editing; **do not assume every such control lives in `path_controls.rs` merely because of the filename**.

Targets pending source verification — report conflicts instead of guessing:

- Preserve clear indentation for `Vector contents → Group → Geometry / Repeater → property`. Disclosure, icon, and text must not overlap.
- Arrange Repeater `Copies`, `Offset`, `Position`, `Anchor`, `Scale`, `Rotation`, `Start opacity`, and `End opacity` as adjacent, uniformly aligned compact property rows.
- Align scalar and 2D value slots within the same right-side editing region; X/Y fields sit beside each other rather than wandering across the panel.
- `Composite copies: below` is configuration/status, not floating centered decoration. Choose a right-aligned property value or explicit status presentation according to its verified semantics.
- `Add repeater` should remain a low-emphasis toolbar action; align the delete `×` with the repeater's own row.
- `Edit path with Pen` is a vector-path context action. `VECTOR PATH` can be a group label, not a justification for large irrelevant separators or banners.
- Ensure Path transform properties use common fields, units, focus behavior, and keyframe-state visuals, rather than a duplicate ad hoc control family.

### 6.3 Screenshot acceptance

- Masks and Path must not show vertically misaligned labels/values, excessive spacing between controls, drifting input widths, stateless booleans, or keyframe icons encroaching on labels.
- At narrow and wide panel widths, identical property classes preserve predictable label starts, value edges, and icon baselines.
- Compare screenshots across: default, selected, expanded, hovered, keyboard-focused, readonly/disabled, long-name, multi-field, minimum-window (`900×600`), and DPI-scaled cases.

---

## 7. Timeline — preserve current layout and interactions

### 7.1 Tracks and columns

Shy is a layer-row checkbox in a complete `icon.slot`, adjacent to the existing layer switches. Its marked state organizes the timeline list; it does not mean render visibility. A matching selected icon in the timeline toolbar toggles the hidden-row projection. Keep both controls inline, preserve row height and existing hit targets, and explain their list-only meaning in localized tooltips. Allocate the new column in the shared styled column plan, including when sampling columns are unavailable. The search field yields space using `timeline.search.inset`, `timeline.search.reserved`, and `timeline.search.max_width`; their initial values preserve the existing search layout apart from the added toggle. Shy selection/filtering behavior and project persistence are functional requirements of #256, outside the scope of visual remediation.

The composition work area occupies its own narrow lane below the time labels and audio markers. Read its height, inset, handle size, hit tolerance and line thickness from `timeline.work_area.*`; reuse hover and accent colors. Keep the existing layer columns and row heights. Drag either endpoint or translate the range in this lane without seeking; show a local preview and commit once on release. Escape, focus loss or a changed authored snapshot discards the preview. Preview range, starting point, frame skipping and resolution use explicit dropdown choices; looping uses a checkbox. Show actual render dimensions and decoder cache counts, and identify on-demand decoding rather than presenting it as a fully cached RAM preview. The movie export scope is an explicit full-composition/work-area dropdown.

The timeline layer gutter preserves the **11** existing columns and adds a 3D switch within the name allocation. The compact layout may hide blend mode as specified below. **Do not invent `solo` / `shy` columns.** Offsets below are from `desktop/app/src/panels/timeline.rs`:

| # | Column | X offset / width |
|---:|---|---|
| 1 | Visibility toggle | `+10` / `16` |
| 2 | Lock toggle | `+28` / `16` |
| 3 | Index | `+50` / `14` |
| 4 | Track color swatch | `+66` / `6×12` |
| 5 | Disclosure triangle | `+75` / `14` |
| 6 | Layer-type icon | `+91` / `18` |
| 7 | Name | `+112` / `gutter - 398` |
| — | 3D layer switch | one `$sizes.icon.slot` at the right edge of the name allocation; subtract that slot from the name's text rectangle |
| 8 | Track matte | `gutter - 280` / `78` |
| 9 | Blend mode | `gutter - 196` / `76` |
| 10 | Parent-link icon | `gutter - 114` / `14` |
| 11 | Parent | `gutter - 94` / `86` |

`gutter = min(640, area.width() * 0.52)` — declared as `const GUTTER: f32 = 640.0` at `desktop/app/src/panels/mod.rs:69`.

Header, name, 3D switch and optional cells share `ColumnRects::styled`; retain the historical optional-column widths and gaps above. Reserve the `icon.slot` 3D target before allocating name text. Hide the blend column if keeping it would leave less than `value.min_width` for the name. If the name still cannot retain `value.min_width`, hide the Parent & Link header, icon and cell together and reclaim their allocation; parenting remains available in the inspector. Width-based hiding must preserve the user's column visibility preferences, so wider panels restore the requested columns. Never keep overlapping invisible hit regions. Preserve the full 3D target at supported minimum floating widths; if even that slot cannot fit beyond the fixed leading controls, hide its header and control together. A dropdown whose column becomes hidden must close without committing. Existing visibility, lock, disclosure and time-lane hit regions stay unchanged.

Header band total height `64`; ruler at `+36`, height `28`; rows begin at `+64`; layer row height `$sizes.row.height = 22`. **Zoom control sits on the ruler's top-right, not in a new bottom zoom bar.** Keyframe diamonds are directly drawn primitives, not component-style instances (see §5.3).

A horizontal scrollbar occupies the bottom of the time lane. Preserve a full `field.height` pointer target; the centered visible pill uses `timeline.scrollbar.thumb.height`, `.radius`, `.min_width` and `timeline.scrollbar.inset` from `dark.json`. Use the panel background, a border-token top separator, and border/disabled/muted/focus colors for inactive/idle/hovered/dragging states. Its thumb represents the same zoom and offset as the ruler and clip/curve lane. Dragging it changes view state only; it must not seek, edit layers, change selection, or enter undo history. The label/property gutter remains fixed. The graph's existing lower ruler stays in place; its mode caption uses the left footer so the scrollbar does not cover a seeking target. At 100% the full-width thumb is inactive.

While a layer move, trim or ruler scrub is held near either horizontal edge, pan the view using elapsed time and preserve the gesture's composition-time anchor. Stop on release, Escape or focus loss. Offscreen clip endpoints must neither paint nor accept edits in the property gutter. Changes to zoom must update the ruler and clip coordinates in the same frame.

### 7.2 Property subrows

- Group headers use the existing header surface and primary text; child rows retain explicit indentation and subtle tree guides. All values of one property stay inline; do not spend space on redundant X/Y labels for a two-value offset.
- Layer properties show Masks before Transform in both the inspector and the timeline. Masks and vector contents own their disclosures; there is no extra Geometry wrapper. The 3D Checkbox belongs directly to each spatial layer's timeline row, with a matching column header; it preserves locking, observation-only mode and undo. At narrow widths the blend column is hidden before compressing the layer name to make room for the switch. Anchor is a real static source-space property; position and scale components share a row. Existing vector contents and effect-instance rows preserve their native editors.
- The timeline Masks group exposes the same add menu as the inspector. Each mask header reserves four inline action slots for enabled state, path editing, revealing its timeline channels, and removal. Reserve these slots before clipping the name and disclosure hit region; at minimum floating widths, reduce the group's leading inset before clipping the name, keeping the disclosure and action hit regions intact. Preserve row height and disable edits for locked or observation-only layers.
- A hairline horizontal separator spans each timeline property row into its keyframe lane, using `tree.separator.width` and `tree.separator.opacity`.
- Time ticks and labels belong to the ruler. Do not extend a vertical grid line at each second through the layer/property body. Preserve the playhead, keyframe marks, snap feedback, and the label/lane boundary.


Property subrows are **embedded in the existing name-gutter area**, not a separate property-track area. Existing `property_layout` is in `timeline.rs:34-57`: stopwatch at `+98`, name at `+118`, values begin around `+0.52×width`. Order: **Position → Scale → Rotation → Opacity**. 2D components `[0,1]`, 3D components `[0,1,2]`; 2D Rotation uses `[2]`. **Do not rewrite this geometric contract for appearance alone.**

Differentiate a layer segment bar (in/out/duration) from property keyframes (diamonds); segment endpoints must not look like keyframes.

### 7.3 Time precision, snapping, clamping

- **Frame values are `f64`, not integers.** Integer frames are the default snapping/rounding unit. `Alt` while trimming disables rounding and allows subframe trimming. `Alt` during segment move disables snapping only, not rounding.
- Trimming already clamps to `±motion_core::MAX_FRAMES` and enforces a positive segment width; ruler dragging clamps to `[0, total-1]`. **Preserve this behavior.** Readouts must display the actual clamped result, including boundary values, rather than an unapplied input.
- Snap tolerance comes from `desktop/ui/src/snap.rs`: **8 logical pixels, independent of zoom**. Do not claim universal snapping.
- Snap guides appear **only** in three existing timeline drag contexts:
  1. **segment trim** — targets: `0`/`total`, playhead, other layers' in/out/keyframe/reference, work-area bounds; excludes the dragged layer
  2. **segment move** — excludes the moved layer; either edge can snap while the opposite edge preserves `duration`
  3. **ruler drag** — all layers, excludes the current playhead, targets within `[0, total-1]`
- The guide is currently 1px and hardcoded `Color::rgb(255,210,60)`, **not** a `dark.json` token. Node canvas, graph box, transform handles, and numeric input do **not** inherit equivalent snapping.

---

## 8. Interaction semantics, modifier keys, and input safety

### 8.1 Existing shortcuts

- Show property shortcuts: `P`, `S`, `R`, `T`, `U` (`desktop/app/src/shortcuts.rs:69-73`).
- Add/remove a property keyframe at the playhead: `Alt+Shift+P/S/R/T` (`shortcuts.rs:80-83`).
- **Do not claim that `V`/`H`/`Z` or `B`/`C`/`E`/`L`/`Q`/`W`/`Num5`/`Num6`/`F4`/`Minus`/`Backquote`/`Slash` are available.** Adding them would first require changes to the `Key` enum in `desktop/ui/src/input.rs`, event routing, and separate acceptance.
- Do not define "clicking the stopwatch creates a keyframe at the current frame" without verifying the actual implementation. Draw and handle the stopwatch according to the repository.

### 8.2 Context-specific modifier matrix

| Modifier | Context | Existing behavior |
|---|---|---|
| `Alt` | Segment trim | Disable rounding; allow subframes |
| `Alt` | Segment move | Disable snapping; preserve integer rounding |
| `Alt` | Keyframe drag | Lock value axis, adjust time only |
| `Alt` | Bézier handle | Unlink opposite handle |
| `Alt` | Mask/path point | Disable automatic opposing mirroring |
| `Ctrl` | Keyframe drag | Duplicate rather than move |
| `Ctrl` | Bézier handle | Lock time component (X unchanged) |
| `Alt` | Corner/edge scale | Release the uniform constraint; scale the controlled axes independently |
| `Shift` | Edge scale | Scale uniformly; `Alt` takes precedence |
| `Ctrl` | Corner/edge scale | Preserve the default or Shift/Alt scaling mode |
| `Ctrl` / `Shift` | Segment click | Additive/range selection; suppress dragging |
| `Shift` | Bézier handle | Snap angles to 15° |
| `Shift` | Rotation handle | Snap angles to 15° |

These are **only** behaviors evidenced in the supplied audit, **not** an exhaustive listing across every input context reported in the audit. Verify `timeline.rs`, `graph_keys.rs`, `graph_handles.rs`, `path_controls.rs`, and `transform_controls.rs` before changing them.

### 8.3 Focus, menus, keyboard safety

- Keyboard actions target the focused widget and valid shortcut context only. Global shortcuts must not interfere with IME composition, text entry, open menus, or active drag gestures.
- Opening a dropdown blocks other inputs in the same panel; overlays render after regular content to preserve actual z-order and hit order (`desktop/ui/src/ui.rs`, `widgets/composite.rs`).
- Preserve the existing repeat-key whitelist in `shortcuts.rs:131-142`: `PreviousFrame`, `NextFrame`, `MoveLeft`, `MoveRight`, `MoveUp`, `MoveDown`, `PreviousKey`, `NextKey`. **Do not enable repeat for every key.**
- `Esc` cancelling an active interaction must preserve the current `cancel_interaction` contract; visual cleanup must correspond to actual gesture state.
- Effect search defaults to one addition followed by closing. Its native checkbox, or Tab while search owns input, explicitly enables continuous additions; retain the query and native window ownership until cancellation. Repeated keys and IME confirmation never add effects. Read popup geometry from `effect.search.*` and existing field/row/padding tokens. Successful additions identify their effect and target layers, expand the new effect, and reveal its selected header using the existing selection color. Failure preserves the original project and reports the original cause through the existing error boundary.

---

## 9. Undo, history, and non-history UI state

- Project history is in `crates/motion-core/src/editor.rs`, using `VecDeque<History>` snapshots with **128 total undo+redo entries** and a shared **32 MiB** budget. Eviction order: `undo.pop_front()` before `redo.pop_front()`.
- Node graph history is a **separate 64-entry stack** with no equivalent byte budget (`desktop/app/src/node_editor.rs`). **Do not merge these stacks for superficial uniformity.**
- During a gesture, `undo()` returns `Err(Error::Gesture)`; do not bypass gesture boundaries to make a UI button appear active.
- Existing `Action::History(i32)` gesture protocol: `2=BEGIN`, `3=COMMIT`, `4=CANCEL`. After an active drag begins, `Esc` queues **exactly one** `History(4)`. Indicators must reflect beginning, committing, and cancelling gestures accurately.
- **Not added to project history**: `Seek` / `TogglePlay`, `ViewKind` / `ViewOrbit` / `ViewZoom` / `PreviewMode`, workspace switching/docking/floating, import/export actions, timeline zoom/scroll, and `graph_mode`. UI layout updates must not cause these to write history.

---

## 10. Composition viewport, performance feedback, and preview

- Composition screen-space ↔ composition-space transforms must share **one mapping chain** and round-trip consistently across all pan/zoom combinations. Dragging an item back to its origin must not accumulate position drift. `SelectAt` and numeric property edits must agree on the resulting coordinates.
- Pure preview dragging uses `PreviewDrag` (`panels/types.rs`); it changes only preview output, **not** project state or undo history.
- The Info panel already displays `state.desktop.uiPerformance`: composition render count, texture reuse rate, and last presentation time in milliseconds (`panels/auxiliary.rs`). **Verify any redraw optimization against these counters**; do not claim no panel redraw without measurement.
- New UI painting must respect existing GPU batching, clipping, caching, and dirty flags. Do not turn lists, video frames, or keyframes into unnecessary numbers of independent draw primitives for appearance alone.
- This specification does not prescribe a replacement rendering engine or GPU pipeline.

### 10.1 Node canvas view controls

- The node toolbar's View menu and canvas context menu expose native checked grid/minimap choices, with the same semantics in Chinese and English. Keep the grid visible and minimap hidden for a new workspace; persist these preferences in workspace state only.
- The minimap sits inside the canvas at its bottom-right edge. Read its dimensions, margin, padding, world padding and viewport opacity from `node.minimap.*` metrics in `dark.json`; reuse existing chrome, border, muted text and accent colors.
- Fit node bounds and the visible canvas into one aspect-preserving projection. Show the current viewport distinctly; click/drag navigation changes only canvas pan. Cancel an unfinished drag on Esc or focus loss, and capture input above underlying nodes. Hide the overlay when there is insufficient room, retaining its enabled preference.
- Grid/minimap visibility and minimap navigation must leave graph source, compilation stamps, permanent output and both history stacks unchanged. Text, IME, menus and active gestures retain priority over G/N; Ctrl+G keeps its grouping behavior.
- Node choice menus align to the value field, open below it when space permits and above it near the canvas bottom, and remain inside the canvas. Measure option text for width, bounded by the field and `dropdown.max_width`. Use existing popup/row states, body font and padding; retain the logical pointer row size with `node.dropdown.row.height`. At the user's explicit request, canvas-owned choice menus follow the same zoom as their card, including text, row height, padding, border and shadow. Hit regions follow the painted rows with no overlap. Escape or an outside click cancels without changing source or history.

---

### 10.2 Node diagnostic details

- Keep the current validation diagnostic available from a Details toggle in the footer. The bottom drawer shows the original cause in full, with explicit code, node and port fields; wrap paths and IDs and scroll overflow without changing graph content
- Locate node resolves the qualified node ID through nested groups, selects it, and centers it above the drawer. Navigation preserves source, collapsed state and graph history. Disable location for missing/stale targets, active edits and menus
- On the diagnostic node, mark each input lacking both a connection and a stored default with a hollow ring using the existing error token. Keep valid defaults and other draft nodes unmarked
- The drawer owns its pointer region and focused scrolling keys; Esc closes it and Enter locates the node. Never route destructive canvas keys or a wire drop through the drawer. Successful validation removes the current diagnostic; this drawer does not imply multi-error collection

### 10.3 Node first-use guidance

- A fresh standalone demo identifies itself as an editable example and offers the example browser beside a Continue action. An empty standalone root offers the same starting point and the existing Shift+A shortcut
- Reserve a separate strip above a fresh demo canvas; empty roots use their vacant canvas without changing drop coordinates. Do not cover nodes or draw a modal tutorial. Continue preserves source, selection, history, preview and node screen positions. Hide the strip when the dock cannot keep both instructions and a usable canvas
- Opening non-empty saved documents, layer effects and nested groups must not introduce this welcome strip. Never replace an existing graph implicitly. Use the existing example browser, save/discard choices and stale-snapshot guard
- Put the independently authored two-node Solid Color example first. Opening the browser and its default example takes two clicks; selecting Color exposes editable RGBA controls. Retain advanced examples as independent editable sources
- Use existing panel, text, muted, row-height and padding tokens. Validate both languages, minimum and default windows, empty state, dismissal, real GPU output, source reopening and undo

### 10.2 Node catalogue search

- The Add popup and library share one catalogue matcher. A nonblank query searches all categories; blank browsing preserves category order. Match English and Chinese titles, independently authored aliases, pinyin/initials and short descriptions. Require every query token and rank title matches first.
- Use the native text editor for caret movement, selection, deletion and IME commits. Up/Down selects a result, Enter adds it, and Esc exits search. IME confirmation must never add a node. Keep the popup's opening world position when adding; empty results and cancellation leave the graph unchanged.
- Show a title and short description per result, with actual input/output types for the selected or hovered entry in a fixed footer. Read popup dimensions and result/footer heights from `node.add_menu.*` and `node.search.*`; reuse existing text, inset, hover and selection tokens. Clamp the popup to its panel and clip rows and details independently.
- In a short dock, remove the popup's redundant scope heading to retain a complete result and input/output footer, and scroll categories separately. In a short library, remove its redundant title and search-results heading first; omit the details footer only when a complete result would otherwise be hidden. The Add popup remains available for port details.
- Record only successful catalogue choices in a bounded, deduplicated list of 16 recent keys. Persist it in workspace state, retain it across graph undo/new/open, and ignore unknown or oversized saved keys. Browsing and failed additions do not update recents or graph history.

---

### 10.2 Node and group names

- Node context menus expose Rename. A group or boundary node also exposes Rename ports, followed by a separate input/output selection menu. Keep names separate from stable node IDs, socket keys, types, units and connections.
- Use the native text editor in a bounded popup. Read dimensions from `node.rename.*` in `dark.json`; reuse existing chrome, border, field and muted text colors. Confirm with Enter or Apply; Escape, focus loss and outside clicks cancel without a graph edit. IME confirmation must remain in the editor.
- Save ordinary titles in the optional `display_name` field. An empty ordinary title restores its operation or public-parameter name. Group and port renames update their existing bilingual metadata; parameter labels retain their existing bilingual inspector fields. Limit names to visible text of at most 120 UTF-8 bytes.
- These context-menu rename commands produce one undoable source edit, without a semantic or preview revision change. Default group and public-parameter names are readable, numbered and distinct in both UI languages. Exposed port labels use readable node titles, while their socket keys stay stable.
- This naming layer does not implement reusable group definitions or per-instance parameter bindings. Those remain a separate group-encapsulation task.

## 11. Existing project model and Project panel

- Project model: `crates/motion-model`, Rust + serde; project format `.msproj`. **Do not introduce TypeScript `Project`, `Keyframe<T>`, `AnimatedProperty<T>`, or `EditorState` types** in this design document.
- Workspace state is a concrete struct in `desktop/ui/src/workspace.rs`; do not replace it with a hypothetical `workspaceLayout: unknown`.
- The Project panel currently shows a **flat asset list** and a footer with item/project counts. It does not currently have a `name`/`type`/`dimensions`/`fps`/`duration` column-header table, nor a confirmed virtualized list. Future expansion must be marked **not implemented / separate feature request**, outside this visual-remediation scope.

---

## 12. UI redraw acceptance checklist

**Baseline**: treat every §0.7 row as a traceable remediation item. Report `location / before / after / screenshot or interaction evidence / status`. Identify unimplemented work and reasons; **never mark the whole specification compliant without evidence**.

### No functional regression

- [ ] `dark.json` remains the normative single styling source. No CSS/parallel theme is introduced. Verify each §0.7 hardcoding issue; **do not pretend existing literals have already disappeared**.
- [ ] No new frontend stack, state model, project format, or history system.
- [ ] All 19 available panels and five workspace presets work; legacy Masks/Expression layouts migrate without losing project data or other panel ownership.
- [ ] `f64` timeline values, `Alt` subframe trim, and existing snap/clamp behavior are unchanged.
- [ ] Shortcut, IME, focus, menu, drag, and Undo precedence is unchanged.
- [ ] Windows desktop presentation has no incorrect clipping, pointer offsets, or focus drift across supported DPI/window sizes.

### Visual acceptance

- [ ] Masks and Path use consistent title/group/object/property hierarchy.
- [ ] Scalar properties retain one-row semantics; 2D fields sit alongside each other with aligned edges.
- [ ] Boolean controls show state; enum `Add` and boolean `Invert` are visually distinguishable.
- [ ] Repeater and Mask property rows have no large, useless gaps and scroll compactly.
- [ ] Audit §5.3's 8×7 state matrix. First verify actual state-resolution code; add tests for `disabled` and `invalid`/`focused` precedence without assuming it already exists.
- [ ] Reduced Motion applies only to editor UI transitions, not preview/export of authored animation; mark OS integration missing if unimplemented.
- [ ] Hover/selected/focused/disabled/editing/keyframed are visually distinguishable, without layout jitter.
- [ ] Long names, narrow panels, minimum `900×600`, default `1600×1000`, and Windows DPI scaling lay out correctly.
- [ ] Timeline preserves the existing columns, adds the layer-row 3D switch, and follows §7.1's compact blend-column rule; zoom stays on top and property subrows stay in the name-gutter region.
- [ ] Text baselines, icon centers, and value-field edges align across panels.

### Interaction and performance acceptance

- [ ] Hit regions contain drawn controls without conflicting with neighboring hit regions; preserve generous existing hit tolerances. Overlays capture events in correct z-order.
- [ ] Layer/mask selection, keyframe moves, and numeric editing refresh UI state without fake or stale feedback.
- [ ] Preview dragging does not pollute project history; `Esc` cancels the active gesture exactly once.
- [ ] Coordinate forward/inverse transforms round-trip; clamped field values display actual boundary results.
- [ ] Compare Info-panel rendering counts, texture reuse rates, and presentation milliseconds with the baseline; no unexplained regression.

---

### Node input defaults

- Keep each stored default on its input socket row. Use the existing numeric editor for scalar dragging and text entry, a comma-separated field for vector components, a checkbox for Bool, and the existing color picker for Color.
- Use existing semantic field, label, padding, border, selection, error, and disabled tokens. Idle numeric/vector values have no field background; editing reveals focus and validation state. Preserve socket hit tolerances.
- Wired inputs keep their stored fallback visible in a disabled state. The wire determines execution; disconnecting restores the saved value. Read-only controls cannot start an edit.
- Cards with stored defaults put outputs after the input rows so value controls cannot overlap output labels or sockets. These are actual port rows, not spacer rows. Cards without defaults retain their existing layout.
- Numeric drags and color gestures create one history entry; Escape restores the starting value. Vector text owns select-all, navigation and deletion keys and does not commit during IME composition. Input changes refresh the live preview without editing the user's composition.

## 13. Execution rules for coding agents

1. **Inspect before editing.** Read `desktop/ui/styles/dark.json`, `style.rs`, `theme.rs`, `ui.rs`, `paint.rs`, and the affected panel sources. Identify each visual defect, its drawing code, and existing tokens that will be reused.
2. **Keep patches local.** Prioritize Masks/Path row layout and native control grammar. Do not use a visual fix to migrate Rust UI, introduce web technology, or change the project model.
3. **Never fabricate missing functionality.** If a mockup suggests an unimplemented feature, mark it for discussion. Do not add it silently; do not remove a working feature merely because its screenshot looks poor.
4. **Verify every patch.** Run formatting, compilation, existing tests, and UI screenshot/hit tests at minimum and default window sizes. Verify Windows desktop DPI and focus behavior.
5. **Report truthfully.** Distinguish `already existing` / `modified in this patch` / `new request` / `unverified`. **Successful compilation is not a substitute for visual and interaction acceptance.**

---

**Document boundary**: this file governs only the appearance, layout, and interaction presentation of Motion Studio's existing Windows-native GPU UI. It does not redefine software architecture, rendering strategy, time model, complete shortcut map, or project data formats.

**Related documents**: [desktop/ui/STYLES.md](desktop/ui/STYLES.md) · [docs/ui/pr7-12-acceptance.md](docs/ui/pr7-12-acceptance.md) · [desktop/design/Motion-Studio-UX.md](desktop/design/Motion-Studio-UX.md)
