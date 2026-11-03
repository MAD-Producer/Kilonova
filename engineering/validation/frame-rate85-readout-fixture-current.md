# Issue 85: approved readout-overflow fixture

Status: the original hover regression passes with all assertions unchanged.
The user explicitly approved this one additional fixture file. No production
panel, observation threshold, layout, style or hit area changes are included.
DESIGN section 0.7 baseline rows are retained for that reason.

The old numeric `1e308` fps reached overflow before rational clocks became
mandatory. It now fails earlier at strict clock validation. The replacement
uses a valid 240/1 clock and a finite large authored slope. The healthy snapshot
retains 30/1 and shares that slope: its per-second f32 readout stays finite,
while the 240/1 hover result overflows at the original boundary. Bounds remain
finite. The deliberately shortened provider composition still caps curve
sampling before the keys; it is not represented as a legal saved project.

The initial attempt changed only the fault snapshot's slope. The effect graph
then produced the expected failure, but comparison retained its initially
cached healthy track and the original assertion failed. That output is retained.
Sharing the finite slope in both snapshots corrects the fixture's cache setup;
no product cache or sampling behavior was changed. The original 300 fps proposal
was never applied because it exceeded the supported rate; 240/1 is valid.

The actual original test passed in 120.922 seconds including compilation. Two
fresh child processes execute the real PaintList/TextAtlas hover consumers.
Each retains the original first/59/60/120 checks, healthy intervening reads,
available-curve assertions, exact frame/object/revision/cause checks, and
unchanged saved-project/history assertions. Each actual default-Info file has
five records: initialization and first-Warn/once-at-60 Error for two sites.
Each stderr has exactly the two Errors; stdout contains no observation records.
These are documented lifetime counters: healthy reads preserve their count;
60 occurrences do not imply any fixed duration. The second process proves reset.

The controller additionally compared all existing assert macros against HEAD
and inspected the real files/stdout/stderr independently. Session 6892 exited
zero and was consumed; the original failed session 61424 exited one and was
also consumed. The [receipt](data/frame-rate85-readout-fixture-v2.json) and
[archive](data/frame-rate85-readout-fixture-v2.zip) retain both outputs, exact
before/after source, patch, command, environment, source closure and a read-back
verified per-member manifest. Complete integrated desktop/UI regression and
final CI remain required. Earlier native crashes remain unresolved.
