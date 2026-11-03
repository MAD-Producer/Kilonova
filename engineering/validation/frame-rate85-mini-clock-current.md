# #85: rational mini-curve clock and graph range identity

The eight mini-readout failures recorded in
[the complete desktop gate](frame-rate85-panel-fixtures-current.md) are fixed in
the explicitly approved five-file scope. Ten distinct focused tests now pass.
This is **not** a passed full desktop, final-main or cross-platform gate; #85
and the complete 3D route remain open.

## Functional correction

`animation_controls/mini.rs` now consumes `project_clock::read`, the existing
strict numerator/denominator reader, before converting the validated ratio to
f64 for display sampling. It no longer rejects healthy rational snapshots or
silently invents 30 fps. An invalid clock still produces the unavailable card
and the original observation site, with both the existing context and the
strict reader's original cause.

`graph_ranges::Scope` retains the normalized `FrameRate` separately from the
three integer geometry values. Changing `30000/1001` to `2997/100` resets
retained navigation even though both display approximately 29.97 fps. An
equivalent ratio, such as `60000/2002`, preserves navigation.

The remaining three approved files are existing test modules. Two new tests
exercise actual composition-frame readouts, per-second units at integer and
fractional rates, denominator changes and equivalent-ratio navigation. Existing
assertions, tolerances, test discovery and ignore markers are unchanged.

## Preserve the distinct sampling fault

The original observation test used numeric `fps=1e308` to overflow derivative
sampling. Numeric fps is now invalid at the strict clock boundary. The approved
fixture uses a valid clock and finite authored opacity endpoints `1e37` and
`3e38`, with a distinct source revision. Sampling and percent conversion then
reach the original `comparison sample is not finite` fault.

The original checks still require that original cause, independent
`curve.mini.readout_sample` escalation, default file/stderr output and unchanged
healthy-recovery behavior. No error is swallowed, substituted with success or
reclassified as a normal branch merely to pass a count assertion.

## Executed evidence

The frozen source is `73020b5c` plus the five recorded, user-approved file
changes. All tracked crate/desktop files and Cargo manifests/lock were hashed
before and after the focused gate. This identifies the actual uncommitted
tested source rather than claiming that the clean `73020b5c` already had the fix.

- All desktop test targets compiled with
  `cargo test -p motion-desktop --no-run --locked --offline`: pass, 179.797 s.
- Exactly the eight originally failing tests and two new tests ran, each with
  `--exact --test-threads=1`: **10 passed, zero failed/ignored**. Each final
  summary proves that one actual test ran; other filtered cases are not counted.
- Total focused gate: 193.750 s, original process exit 0,
  `sourceUnchanged=true`. These selected cases use CPU/model/paint consumers;
  they do not create another native/worker/GPU acceptance workload alongside
  the independent complete renderer gate.
- Two already-passing observation tests were deliberately executed again only
  to retain their previously temporary output: 3.188 s total. They are not
  counted as two additional distinct regressions.

Raw captures from both first/reset child processes verify default Info
initialization, first Warn and lifetime occurrence-60 Error, suppression after
60, original causes, healthy normal-branch silence, and process reset. Error
appears in the log file and stderr; observation records are absent from stdout.
These existing counters accumulate over a process lifetime; healthy recovery
does not reset them. Sixty occurrences are not equated to one second.

`data/frame-rate85-mini-clock-v1.zip` has 55 members, 205,674 bytes, SHA-256
`96008fb14cb197abd2c2af5e110bc9cd0ceed76a75265b4c30e09ce028c9dfb4`.
The receipt and every archived member were read back and hash checked. It
contains exact commands, reports, logs/stdout/stderr, the tested five source
files, controller/environment scripts and the approved patch. Status:
`mini_clock_fixed_focused_tests_passed_full_desktop_pending`.

## Remaining acceptance and presentation boundary

Other panel fps readers, fractional timecode and actual fractional-readout
screenshots, final-main integration, complete maintained regression and
cross-platform CI remain separate work. The earlier native heap corruption is
not repaired or diagnosed by these CPU readout tests. The original failed full
desktop record and both earlier failure archives remain unchanged.

No control, layout, row height, style token or hit region changes. All DESIGN.md
section 0.7 baseline rows are retained with that reason: the path-controls row
steps, column split, mixed button heights, numeric background and boolean row;
the effect-controls row steps, boolean, enum, hover background and hierarchy;
and the timeline gutter coordinates and snap guide. None is declared a visual
fix or waived from its own acceptance.
