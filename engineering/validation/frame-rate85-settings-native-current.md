# Issue 85: native composition-rate settings acceptance

This record covers the existing composition settings modal on the integrated
`df3016b2` baseline plus the new test source. It adds verification, not product
logic, layout, style, hit areas or panel changes. Android remains excluded.
The separate failed full regression and unresolved native access violation are
not replaced by this focused result. Issue 85 and the full 3D route remain open.

## Actual execution

The new Windows GPU test runs a fresh owned child process and hidden attached
native windows. Inputs go through the existing Shell, composition modal,
dropdown widget and command router; there is no OS foreground input injection
or mocked engine reply. All 12 captures use the actual native GPU readback.

The final focused invocation passed one test, with no ignores, in 14.24 seconds
after compilation. Its source hashes remained unchanged throughout execution.
English and Chinese each exercise both policies for `24000/1001` to
`30000/1001`:

| Policy | Original position keys | Applied position keys | Explicit total frames |
| --- | --- | --- | --- |
| Keep frame positions | 0, 24, 96 | 0, 24, 96 | 240 |
| Keep time in seconds | 0, 24, 96 | 0, 30, 120 | 300 |

The test verifies the actual rational rate, unchanged key values and easing,
one revision for successful Apply, atomic undo/redo and exact saved-project
readback. Cancelling a draft preserves the project, revision, undo/redo state
and existing project file bytes. GPU captures independently preserve engine
state and contain nonblank UI pixels. The options, selected drafts, rejected
shortening requests and explicit-duration drafts were visually inspected in
both languages.

## Rejected shortening is part of the contract

The original new-test invocation failed its Apply assertion after the English
frame-position branch had passed. The seconds branch requested 240 total
frames while the implicit full-length layer retimed to 300. The existing
`shorten=reject` policy correctly rejected that shortening request. Its raw
failure and source remain in the archive.

The final test retains all success assertions and adds an explicit rejection
check: the modal displays an error while project, revision and history stay
unchanged. It then types 300 into the actual duration field and applies the
valid request. No production return value, shortening policy, old assertion,
timing budget or retry behavior changes. A caller must currently request the
matching composition duration explicitly; selecting the seconds policy does
not automatically rewrite the duration draft.

## Visual and validation limits

The rejected error line currently contains a clipped serialized error message,
and the previous error remains visible while the user corrects the draft.
These are retained presentation limits, not a visual acceptance claim or a
reason to weaken rejection checks. The test does not change them. DESIGN
section 0.7 remediation rows are retained because no product paint, layout,
tokens or panel source changed.

This verifies the existing native settings interaction, not automatic duration
selection, every rate or locale, long-sequence drift, source-video decoding,
final movie timestamps, a new-process project reopen or a complete desktop
suite. Other requirement-level records retain their own source boundaries.
The fresh head's required CI and the failed local full regression are separate.

## Reproduction and retained evidence

On Windows with the default GPU feature and an actual adapter:

```text
cargo test -p motion-desktop --features ffmpeg --bin motion-studio --locked --offline shell::rational_clock_native_tests::settings::rational_rate_settings_native_dropdown_cancel_apply_and_history -- --exact --test-threads=1 --nocapture
```

Set `MOTION_RATIONAL_SETTINGS_UI_ARTIFACTS` to a directory to retain native
captures. The test otherwise uses a temporary directory. Default Info, original
stack/backend and source-specific build artifacts were used for this record.

The [archive](data/frame-rate85-settings-native-v2.zip) and
[SHA-256 member manifest](data/frame-rate85-settings-native-v2.json) contain
the original failed run, the corrected valid-request run, exact source snapshots,
controllers, raw logs and native images. Each archived member was read back and
verified. The rejected run is not overwritten or counted as a pass.
