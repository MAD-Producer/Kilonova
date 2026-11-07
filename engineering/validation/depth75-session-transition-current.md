# Session image depth transition

## Scope

This increment follows merged PR #634 and retains the existing single depth
worker. Session owns the explicit quality controller, actual prepared reference,
original source/document scope and optional admitted candidate. Managed file IO,
decoding, inference and alignment run on that worker. Backend names or resolution
are not treated as evidence of quality; callers supply qualified alternate model,
fit limits, hysteresis and a monotonic clock.

The borrowed sampling view rechecks the live Session binding and actual worker,
request, controller generation and resource instances. New generations revoke
old candidates. Cancellation, seeks, document changes and explicit raw requests
release the transition. The raw Estimate API refuses transition mode rather than
stripping the proof. Candidate planes and paired immutable source pixels enter
existing retention quotas and diagnostics; the already-retained reference is
counted once. Capacity errors retain request/source/original cause and require
explicit recovery, with no retry or raw fallback.

Mapped calibration first strictly rebinds saved authoring data to the actual
reference and prepared RGB fingerprint. For reference = scale*candidate + offset,
only decoded-value anchors become (anchor.value-offset)/scale. Camera distances,
curve, extrapolation, reference camera and world scale remain authored values;
candidate descriptor, provider and raw planes remain actual values. Saved
calibration still refuses direct cross-model rebind. Scene admission independently
checks the source, original saved binding, grids, meaning and metric observations.
Its cache includes exact reference/candidate instances and mapped binding. Paired
candidate color survives file replacement/removal and fresh GPU capture.

## Executed pre-rebase qualification

These results bind the uncommitted increment on parent
`1a939a3dd71d4d37b6635fcbf8b035d615bd67b2`, not later rebased source.

| Control | Actual result |
| --- | --- |
| Complete Session target | 25 passed, 0 failed/ignored/filtered |
| Complete explicit GPU depth_surface target | 51 passed, 0 failed/ignored/filtered |
| Complete explicit GPU Session target | 25 passed, 0 failed/ignored/filtered |
| New controls within those targets | All ten passed |
| Production-file probe on default named owner worker | Passed default Info before backend return, capacity/original backend Error in file and stderr, one-line JSON-only stdout |

The ten new controls cover actual registered-image affine output and accounting;
changed pixels and explicit recovery; original backend failure without retry;
controlled backend blocking and seek invalidation; missing reference/invalid
limits/backward clock; CameraZ/inverse-CameraZ composition; capacity rejection
and explicit recovery after all 128 slots are filled; actual Session capture and
occlusion after source replacement/removal; independent renderer refusal; and
actual renderer GPU pixel equality after cross-model alignment.

The host default already enables gpu-tests, so its first 25-test invocation also
executed the capture control. The explicit GPU invocations independently selected
both complete targets. No original assertions, ignores, timeout, default backend,
numerical gates or 10 GiB build admission were changed. Selected formatted leaf
modules pass rustfmt with child traversal disabled; unrelated pre-existing depth
root/child format differences are retained. No panels were modified.

The production probe first failed to compile because its standalone manifest
omitted the existing motion-render path dependency. Adding that fixture-only
path dependency allowed compilation. The second probe then actually failed with
Windows main-thread stack overflow (0xC00000FD); it never qualified the runtime
checks. Both failures are retained. The third uses the same default named owner-
worker context as desktop engine.rs:805, with no stack-size or RUST_MIN_STACK
override; its assertions and deadlines are unchanged. This verifies that worker
context only. It does not explain/fix the main-thread overflow or establish an
API contract that main-thread Session use is unsupported.

Consumed receipts, raw logs, controllers, synthetic fixture and the exact eleven
executed Rust files are retained in `data/depth75-session-qualified-v3.zip`
(1,598,475 bytes; SHA-256
`c4145e556307728b22957bf48e3e5bbe83def717c20b1681008373349dc1ceb3`).
The adjacent manifest verifies all 51 members. The archive preserves the failed
compile and main-thread execution. No real-model accuracy is inferred from its
controlled synthetic backends.

## Parent qualification and baseline alignment

The exact parent completed the original maintained-workspace command: 5,126
passed, zero failed, 78 original ignores, zero filtered, and all 437 native/doc
invocations accounted for without abnormal exit. Session 63614 was consumed with
actual exit 0. Current-head Linux, Windows and macOS Intel raw CI logs each
verify the 48 worker controls. Parent evidence is retained in
`data/depth75-worker-current-full-v1.zip` (888,013 bytes; SHA-256
`145fc80814cb2dd390098d58577ce3e3fd133e1b3a1e91af17019d17185bdc94`),
with all 24 members verified. PR #634 merged as
`47925a32d78802a6657fd9d59789aca312390c10`, with a Chinese merge message.
Historical restart/PDB/access-violation failures remain separately preserved.
These parent results do not qualify the new increment.

Before final regression this branch was rebased onto freshly fetched main
`47925a32d78802a6657fd9d59789aca312390c10`. Its newer rendering/sampling
changes require their own coverage in the
complete changed-crate suites, all 48 worker plus ten new controls, desktop
integration checks and CI. Pre-rebase passes above must not be relabeled as
post-rebase acceptance. This final regression is pending at this document's
publication; its receipt must record the exact aligned source. Only one own
native/model/CV/GPU workload may run at a time.

## Remaining #75 boundaries

Frozen snapshot capture explicitly refuses an active transition, instead of
silently exporting the retained raw reference. This is an interim refusal, not
completion of frozen export. The next export increment must retain both actual
planes, original saved calibration, prepared RGB identity, resolved model/provider,
selected phase/generation and versioned policy/limits. A wire reader must
requalify immutable reconstructed planes with cancellation/start observation and
bind proof to those exact instances. Metadata-only Session capture must not fit
or decode on its owner thread. Both allocations must enter the existing 192 MiB
bound and wire index/reachability checks. Positive native roundtrip/render,
tampering, cancellation, memory refusal, changed-file verification and no-live-
Session/no-inference export controls remain required.

Measured requested/resolved capability and quality selection, versioned cache,
real continuous-video transition drift/jump and sustained/thermal qualification
remain open. #88 photographed tracking quality remains unqualified. No Android,
UI/panels, product dependency, default model distribution or project-format
change is included. Neither #75 nor #88 is complete.
