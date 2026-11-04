# Tracking import host integration

Issue #88 remains open. On base main `8573d9975cf70d2c8fe05605102ec9d744816070`,
actual host validation completed at `2026-11-01T03:35:12.400413+00:00`. The source stack includes
calibrated plate geometry, bounded canonical JSON, explicit world/camera basis and
unit conversion, corrected homography conditioning and exact PTS/VFR time lookup.
These are backend foundations, not a delivered project tracking feature.

| Actual control | Exit | Passed / failed / ignored |
| --- | --- | --- |
| pose44-check | 0 | [] |
| pose44-tests | 0 | [[44, 0, 0]] |
| plane47-check | 0 | [] |
| plane47-tests | 0 | [[47, 0, 0]] |
| time56-check | 0 | [] |
| time56-tests | 0 | [[56, 0, 0]] |
| host-lib-regression | 0 | [[169, 0, 0]] |
| whitespace | 0 | [] |

The earlier pose host command exited successfully but reused another worktree's
35-test binary from the shared Cargo target. Its exact 44-test coverage guard
correctly failed. Subsequent old plane/time queues executed no build or test.
Those receipts are retained. This integration explicitly updated the current host
crate-root timestamp without changing bytes before each compilation, required
44, then 47, then 56 actual tests, and finally ran all 169 host unit tests.
Original assertions and expected counts were unchanged. One owned Cargo/GPU
controller ran at a time, after the preceding native diagnostic terminated.

The plane fix retains original matrix bytes and homogeneous scale, accepting
ordinary 16K translations while rejecting ill-conditioned mappings. Its original
negative control failed at the intended valid-translation assertion; 47 CPU and
host tests then passed. Exact time lookup preserves original signed PTS, VFR,
reverse/freeze mapping and rational bracket weights without float-fps inference
or interpolation across track gaps. The 56-test suite includes 3360 independent
integer-oracle time cases. No project-fps representation or UI changed.

Archive [exact source and raw controls](data/tracking88-host-integration-v2.zip), SHA-256 `7c0feedfd180e59685189997f179cebb936ed11200a57b69d183f996bb3a684e`,
85 read-back-verified members. It includes the stale-coverage failure,
zero-step old queues, original plane negative and direct-source/time controls.
No executables, PDBs or owned profiles are published. Existing dependencies only;
no third-party assets or Android work. Host unit regression does not establish
full workspace, rendered-footage, desktop/MCP or cross-platform qualification.

Still required: actual external adapters and source-content verification;
persisted original solve and conversion provenance; project integration with
open #85/#86; independent camera/object/parent/pose bindings; explicit gap and
manual-correction policies; history/save/reopen/relink/invalidation; rendered
synthetic and calibrated real-footage/lens-replacement controls; latest-main
integration, complete regression and exact-head CI. Full 3D solving remains a
separate later stage. No #82 failed performance result is retried or reclassified.

## Subsequent original-source verification increment

The [original-byte verification boundary](tracking88-original-source-current.md)
now passes all 66 tracking controls, including the unchanged 56 above. Its later
full host gate failed: 178 passed and one preview-pressure child-process access
violation. The original Windows events and exact binary/PDB are retained. Two
same-binary child diagnostics passed but do not explain the crash. This earlier
169-test success does not qualify that later source; complete regression remains
pending. Source verification still needs project import/relink integration.

## Subsequent managed project attachment increment

The [managed attachment completion record](tracking88-project-attachments-current.md)
now covers original-document project/history/save/reopen/package integration and
shared resource-ID admission. Final controls passed 78 tracking tests, 191 host
unit tests, 99 media unit tests, 23 affected clipboard/preset tests and desktop/
ai-depth compile checks. Earlier V1 full model/core regression passed 639 tests
with one original ignore; it is not an exact final-source full-workspace claim.
The original shared-ID negative failed as intended before the correction. Previous
native AV and #82 performance failures remain retained and unexplained/unfixed.
Actual import publication/routing, decoded source identity, pose evaluation,
relink/invalidation, rendered footage and exact-head CI remain pending for #88.

## Subsequent managed import increment

The [managed original import record](tracking88-managed-import-current.md) now
connects actual file acquisition/publication to an undoable saved edit and the
desktop/MCP `import_tracking` command. It checks explicit source, revision and
document generation, keeps rollback outside the session guard and preserves
post-publication acknowledgement failures. Ninety tracking and 203 host unit
tests passed; final desktop coverage includes all nine MCP tests and actual
default-level native import/save/reopen/history/read-only/protocol controls.
Expected supersession keeps its rejection without Error misclassification.
These controls do not establish decoded PTS/proxy agreement, applied poses,
rendered tracking, cross-platform qualification or complete #88 closure. The
original native AV and #82 strict P95 failures remain unresolved.
