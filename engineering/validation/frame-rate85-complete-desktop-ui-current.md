# Complete desktop/UI runtime regression on the aligned clock branch

Status: all 31 selected Cargo targets passed on frozen source `2a5cec9f7b7127d3bc73ab72f66255d6f926711f`,
including main `b4133761df4bed5341d104740ebbe9820ad14554`. This is not full issue 85/86/88 completion.

The desktop binary completed **1730 passed, zero failed, 72 original ignored**.
Across desktop integration targets, complete UI unit/integration targets and UI
documentation tests, the command completed **1960 passed, zero failed,
72 original ignored, zero filtered** in 2921.328 seconds. Actual
Cargo invocations and result summaries were matched against Cargo metadata.
No test-name filters, assertion changes, budget increases or ignore edits were
introduced. The approved graph-readout fixture and native project-precision
menu tests both passed in this full run.

The command enabled FFmpeg and retained default GPU selection. Only the desktop
test code generation partition count was overridden to four, as documented in
the earlier compiler diagnostic. This does not establish that the original
default compiler access violation or historical native stack/heap failures are
fixed. New parent expression interruption observations are outside this frozen
source and require separate integration validation. Android is excluded.

The controller persisted terminal exit code zero and unchanged source hashes.
After the continuation, its original exec handle was unavailable; its original
process was absent and complete raw Cargo outputs independently established
completion. No controller return code was invented and no tests were restarted.

The [archive](data/frame-rate85-complete-desktop-ui-v3.zip) retains original
controller, environment, output, complete source hashes, Cargo metadata and
target audit. Every archive member was read back and hash-verified; its
[receipt](data/frame-rate85-complete-desktop-ui-v3.json) records scope and limits.
