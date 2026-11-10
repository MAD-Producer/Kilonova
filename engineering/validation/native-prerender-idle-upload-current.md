# Idle native cache upload boundary

Issue [#82](https://github.com/MAD-Producer/Kilonova/issues/82). Component acceptance on base `652fbe6525f5813a85620fe0d5d2ad4eef5f9bfb`; not whole-application performance acceptance.

The original coordinator installed every completed native frame through `install_native_for_next_submission`. This preserves playback fusion, but idle warming has no imminent foreground submission. An edit can invalidate the warm cache while its unsubmitted upload and charged destination remain owned until a later foreground submission.

Idle completion now uses the existing immediate `install_native` API; playback retains deferred installation. No device wait, retry, foreground rendering, UI change or dependency is added. Submitted GPU work remains nonpreemptible, and actual completion still controls allocation retirement.

## Actual native controls

Controller `validate-prerender-idle-upload-v5.py` completed at `2026-10-31T08:13:25.100702+00:00`. Against original production, the idle test failed exactly 0 passed / 1 failed at `idle warmup must submit its own upload`. With the correction and the same assertions, both idle and playback tests passed (2/0/0). Coordinator regression passed 17/0/0; media prerender regression passed 69/0/3; the explicit original four-precision fusion control passed 1/0/0. Provenance, tool inventory and whitespace checks passed.

The fixture uses the public allocation-budget accessor and prepares the real native producer before recording the shared budget baseline. It verifies actual 32x24 red pixels without installing the foreground cache. Persistent background-renderer allocations are therefore included before testing upload retirement. Submission counts, charged-byte assertions and recovery conditions were retained.

Earlier attempts remain in the evidence: v2 did not compile because the new fixture accessed a private budget field; v3 selected zero tests with `--ignored` (GPU tests are enabled by default); v4 exposed the fixture's missing native-producer allocation baseline and failed both positive controls. These are not accepted runs. v5 uses `--include-ignored` and the corrected real-producer setup; no existing assertion or performance criterion was weakened.

## Evidence and limits

Read-back-verified archive: [native-prerender-idle-upload-controls.zip](data/native-prerender-idle-upload-controls.zip), SHA-256 `8910b313a24ca4cfdfeacdbbd76f75170d1494f515c406406d57b4b3503b6ce0`, 37 members. It contains original/fixed source, terminal receipts and actual logs, including earlier failures. No process dump, binary or private machine report is included.

Original combined-v8 still failed strict foreground input P95 (property pair 1 ratio 1.0251986313286552), one native effect-search child, and MCP startup with stack overflow. These component passes establish the upload boundary only; they do not diagnose those failures, qualify final integrated source, or complete #82. Android is out of maintained scope. No panel files changed.
