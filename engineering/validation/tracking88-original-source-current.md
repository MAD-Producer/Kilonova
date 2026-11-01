# Original-source content verification for imported tracking

Issue #88 remains open. This backend increment verifies an imported solve's declared
SHA-256 against all original encoded source bytes supplied by its connector. Its
actual host gate ended **failed** at `2026-11-01T03:45:03.393796+00:00` on source stack
`f0c39c3207fee65826b71af70d1cb0e2aeabe096` plus the archived verifier increment. Main was fetched again during
validation and remained `8573d9975cf70d2c8fe05605102ec9d744816070`.

## Boundary and behavior

`tracking::source::verify` validates the asset first, hashes to actual reader EOF
with fixed 64 KiB storage, and requires an explicit positive byte budget. A limit
violation consumes at most one sentinel byte beyond that budget. Typed failures
distinguish invalid assets/budgets, excessive content, cancellation, original IO
causes and digest mismatch. Identical lengths or textual source labels cannot
establish content identity. Only a matching complete stream produces the private
`VerifiedSource` value. The solve, manual corrections and original PTS are immutable.

Cancellation is checked before every read and standard Interrupted-read continuation;
it cannot interrupt an already blocked reader. The connector must publish request,
source and in-flight context before calling this potentially blocking boundary.
Cancellation is a normal disposition, with no per-chunk logs or healthy-path file IO
apart from the caller-requested read. Actual IO failure is returned with its original
typed cause, without source retry, replacement thread or changed return values.

The proof describes only the bytes actually read. It is not a filesystem lease,
permission to reuse a hash after replacement/reopen/relink, metadata authenticity,
or verification of decoded dimensions/PTS/proxy correspondence. The caller still
owns stable reader acquisition, source lifecycle and command error reporting.
This API is not yet wired into a project import command or a relink transaction.

The existing sha2 0.10 dependency is now unconditional for host verification instead
of optional under ai-depth; its redundant dev dependency was removed. The lockfile
and package versions are unchanged. PROVENANCE.md, ATTRIBUTION.md and NOTICE were
checked before this dependency-scope change. No external assets, upstream code,
network hash service or new package was added. All bytes in controls are locally
authored synthetic data or standard SHA-256 test strings.

## Actual controls

| Control | Exit | Passed / failed / ignored |
| --- | --- | --- |
| host-check | 0 | [] |
| tracking66 | 0 | [[66, 0, 0]] |
| host179 | 101 | [[178, 1, 0]] |

The exact current-source guard passed all 66 tracking tests (56 existing plus
10 new). Full host regression **failed: 178 passed, one failed**. The existing
preview-pressure logging test's subprocess suffered an access violation; Windows
Application 1000/1001 records confirm exception `0xc0000005` with unknown module.
The original exact binary/PDB and private event records were preserved before any
later build. Each original child mode was diagnosed once on that same retained
binary; both exited zero. These observations do not establish the original crash
cause, repair it, or replace the failed full gate. No unchanged full acceptance
retry was performed. Full host regression remains pending, and this increment
is not ready for final qualification or issue closure.

Added controls cover the known abc/empty
digests, same-length replacement, an independent Python hashlib multi-chunk golden,
exact budget/sentinel bounds, maximum integer budget, short/Interrupted reads,
initial and midstream cancellation, repeated-interruption cancellation, unchanged
manual/occluded solve bytes, invalid-asset refusal before reads and retained raw IO
causes. No existing assertion or expected count was weakened. After actual failure
inspection and private binary retention, the existing ai-depth configuration passed
a separate compile check with unchanged source. The original controller did not
reach that later step; the separate result is retained as `feature-check.json`.

Archive [source and raw controls](data/tracking88-original-source-v1.zip), SHA-256 `7facddba83e894cd66948e88ce4782ba5f030dc46c5181863007b34b3bc9b7e8`,
36 read-back-verified members. Tests ran serially under one owned host
controller after preceding native/host validators terminated; current source bytes
and HEAD were guarded throughout. No panels, UI or Android changes. Separate
provenance and tool-inventory checks passed. Complete workspace, rendered footage,
desktop/MCP and exact-head cross-platform qualification are not established here.

External format adapters; decoded metadata agreement; persisted original solve and
conversion provenance; #85/#86 project integration; camera/object/parent/independent
pose bindings; gap/manual policies; history/save/reopen/relink/invalidation; rendered
synthetic and calibrated real-footage/lens-replacement controls remain pending.
This is not a tracker or a complete 3D camera solver, and does not resolve #82 P95.

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
