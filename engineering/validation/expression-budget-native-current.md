# Expression interruption through the actual native worker

Status: default-channel workflow passed on `cc301c04`, based on main `b4133761`.
Normal dev FFmpeg application build passed without a code-generation override.
Its binary SHA-256 is recorded in the [receipt](data/expression-budget-native-v2.json).

A real stdio MCP process submitted an infinite position expression on the native
legacy camera, object 0. The response retained the target and frame plus
`phase=execution cause=execution_deadline elapsed_us=21201
frame_remaining_at_entry_us=99890`. This is a controlled real timeout, not a
diagnosis of the earlier CI failure. Original 20ms/100ms budgets remain unchanged.

With MOTION_LOG unset, actual file logs contained Info start and Error details;
stderr carried the same Error context. All six stdout replies parsed as JSON-RPC.
No expression source appeared in either error sink. The refused edit retained
the complete project and revision; a subsequent valid expression committed one
revision and undo restored the initial project exactly. The process closed zero.

The first observer attempt found no unlocked layer in its empty fixture and
stopped before editing. The corrected controller uses the existing camera; the
original failed receipt remains retained. No product source, test assertion,
budget, ignore marker or return value was modified for this workflow.

The [archive](data/expression-budget-native-v2.zip) retains controllers, failed
attempt, normal build output, source hashes, raw JSON-RPC and both actual sinks.
Every member was read back and hash-verified. The full desktop/UI run predates
this increment; later main integration and complete issue acceptance remain
separate gates. No Android, physical scanout, original compiler/native-crash fix,
or full 3D-route completion is claimed.
