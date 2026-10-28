# Frozen composition camera sources

FrozenVideo retains the session's immutable CPU model-source catalog together
with its project snapshot. Scene construction for exact composition preparation
installs that same catalog before sampling, preserving source camera animation,
rigs and fixed-aspect viewport after the editing session closes. Later session
edits cannot change the reader's saved project. The catalog shares its owned
parsed scenes; freezing does not parse sources, allocate GPU objects or perform
source IO under the session/registry locks. Releasing the last reader releases
its retained parsed source. Reader limits, sequence semantics and wire output
remain unchanged.

Five focused frozen-reader tests pass, including two new tests. The public reader
is exercised after live camera edits, session close and managed model-file
removal. Assertions cover the independent animation clock, unchanged snapshot,
exact fixed-aspect viewport, source ownership release and failed-frame sequence
recovery. Native GPU captures of the reader's actual scene-sampling path match
live session pixels at frames 0 and 30 for Unorm8, Unorm16, Float16 and Float32;
both frames contain visible mesh pixels and animation changes their output.

The focused result was followed by the source-bound combined regression with
persisted calibration, camera ancestry editing and main 2af21061: 879 Rust tests
and 13 workflow tests pass, with zero failed/ignored Rust tests. See
[integrated evidence](depth-camera-integrated-current.md). This is selected
affected regression, not complete workspace or cross-platform acceptance. Existing assertions, timeouts, budgets and
skip annotations remain unchanged. No UI, dependencies or third-party assets
change. This proves frozen reader scene preparation and native frame output,
not end-to-end encoded video/container output. Complex depth integration,
reprojection/DOF and final #87 acceptance remain open; Android is excluded.
