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
