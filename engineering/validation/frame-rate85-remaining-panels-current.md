# Issue 85: remaining approved panel clock adoption

Status: focused CPU acceptance and native clock readout review passed; complete
integrated desktop runtime and final cross-platform CI remain pending. This
does not close issue 85 or 86.

The user explicitly approved ten additional panel files: helpers, waveform,
auxiliary, composition, project, timeline, timeline graph, effect timeline graph,
curve comparison view and the panel module. The patch adopts the strict project
clock reader; numeric or missing clocks are not treated as 30 fps. Sampling uses
the canonical rational clock, converting to floating point only at existing
display/sampling boundaries. Waveform source offsets retain subframe timing.

Non-drop timecodes use nominal labels from the exact rate. For example,
1800.75 frames at 30000/1001 displays `00:01:00:00`; the compact graph ruler
displays `60:00`. Valid arbitrary rational rates without a supported SMPTE label
rule display a frame count. Invalid clocks or non-finite frames produce an
observed unavailable readout. Comparisons restore their owned controls and
graphs clear their existing plot state when the clock is unavailable.

The shared `panel.project_clock`, `panel.timecode_frame` and unexpected format
sites use existing bounded process-lifetime observations. Success does not reset
these counters. A first occurrence is visible in the default file log; occurrence
60 escalates once to Error in the file and stderr. Occurrences are not a time
duration. Healthy reads add no log IO. Context retains original causes, caller
location and available document generation/revision without dynamic key maps.

## Verification

The final ten-file source was validated on parent `aa4143f3`. Complete desktop
test targets compile. Thirteen distinct CPU tests pass with zero failures or
ignored tests: three new clock/request/failure regressions and ten existing
timecode, waveform, curve sampling, cache, axis and menu ownership regressions.
Repeated initial/final runs count once. Existing expected values, assertions,
budgets and ignore markers are preserved. Graph tests exercise actual CPU
PaintList/TextAtlas render paths and the model Engine, without a second GPU job.

The real fault regression runs two fresh processes with `MOTION_LOG` unset. Each
performs 65 invalid clock reads and 65 invalid frame reads, with a healthy read
before occurrence 60. Raw logs confirm first/60 suppression, original causes,
healthy silence, process reset, Error file/stderr parity, file-only Warn and
stdout isolation. It does not claim success resets a lifetime counter.

The [archive receipt](data/frame-rate85-remaining-panels-v1.json) records the ZIP
hash and limits. The [evidence archive](data/frame-rate85-remaining-panels-v1.zip)
retains controllers, environment wrappers, raw outputs, source snapshots,
approved proposal and per-member SHA-256 checks. Initial v1 source is explicitly
historical; final v2 and graph runs verify the final source closure unchanged.

## Design baseline and remaining limits

DESIGN section 0.7 rows are retained with reason: this patch changes clock
arithmetic and error observations only. Timeline coordinates and the column
gutter remain intact; path/effect control remediation files are untouched.
No layout, style, dimension, hit area, asset or dependency changes are included.

The [native readout record](frame-rate85-native-readouts-current.md) retains four
actual attached-GPU screenshots: both fractional clocks in both locales visibly
read `00:01:00:00` in preview and timeline. The earlier two-color OS screenshots
failed visual review and remain retained separately.

The complete renderer gate on historical child `162edfed` passed 957 tests with
one original ignore, but its desktop gate failed one precision-menu child with
a motion-engine stack overflow (1697 passed, 69 original ignores). That source
did not contain these later ten panel changes. A single later diagnostic pass
does not resolve the native failure. The panel changes and cold-expression
preparation fix are now locally integrated; final-source runtime and CI remain
required. A separate graph-readout fault fixture clock update remains pending
user authorization in that historical run. The later explicitly authorized
[readout fixture repair](frame-rate85-readout-fixture-current.md) now passes the
original assertions and actual default-output checks. The historical native
heap-corruption failure remains unresolved.
Android is outside the maintained acceptance scope. No final issue-completion
claim is made.
