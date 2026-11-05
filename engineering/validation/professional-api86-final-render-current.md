# Issues 85/86: renderer after panel clock and expression preparation changes

Status: complete renderer passed on `b015ddad43ac8fc76b6ec049af2a3822edcbdc06`.
Full desktop acceptance, subsequent main integration and final CI remain pending.

The unchanged source includes the ten explicitly approved panel clock readers,
the cold expression preparation fix and the native rational-clock regression.
`cargo check -p motion-desktop --all-targets --locked --offline` passed in
26.032 seconds. The complete default-GPU motion-render command passed 957 tests,
with zero failures and one original ignored test, in 1833.594 seconds including
compilation. All 121 package targets and the documentation target were matched
against Cargo metadata and actual invocations. No targets were filtered; no
assertion, execution budget, ignore marker or quality threshold was changed.

The original controller session 63700 exited zero and was consumed. Source
hashes still matched the frozen closure when the receipt was written. The
[receipt](data/professional-api86-final-render-v1.json) identifies the exact
source and target counts. The [archive](data/professional-api86-final-render-v1.zip)
retains raw output, the controller, environment wrapper, metadata, terminal
receipt and a manifest verified by reading every archived member back.

Own PR 581 merged to main `c0e25eda` during the gate. These results describe
the recorded `b015ddad` source based on `acaab00b`; they do not establish
acceptance for a later main merge. That main change affects video publication,
and requires integration validation on the new source.

The complete historical desktop gate still has its original precision-menu
worker stack overflow. A later diagnostic pass did not establish a fix. The
separate graph-readout fault fixture is outside the approved panel file list
and remains unapplied pending authorization. The earlier native heap failure
also remains unresolved. No full issue completion or Android acceptance is
claimed.
