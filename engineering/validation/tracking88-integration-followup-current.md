# Tracking 88: main integration and test-contract follow-up

The adapter branch integrated main `eeada782` at `09915c81`. This record keeps
the original failing gates and the subsequent focused corrections separate.
It does not close issues 85, 86 or 88.

## Changes

- Construct the existing six-frame video fixture with `FrameRate::integer(30)`;
  its 30/1 rate, frames, audio duration and original assertions remain unchanged
- Print the original video transaction failure, request ID, declared bytes and
  observed capacity when the existing error predicate fails; the predicate and
  failed-state assertion remain unchanged
- Account for main's `32e91e07` source-rate anti-aliasing in the twice-speed
  remap's independently authored flat PCM control. Reuse the existing direct
  trigonometric FIR reference, rather than the production recurrence/mixer.
  Reverse, nested and frozen controls remain unchanged. Preserve the original
  decoded AAC equality, silence, signal, source removal and frozen-history assertions

The original follow-up changes are test-only. A later CI correction below
increases the job's time budget. No production behavior, panel, style,
dependency, retry or test threshold changes are included.

## Evidence and limits

| Gate | Observed result |
| --- | --- |
| Integrated model/core | 910 passed, 1 original ignore, 144 targets |
| Render library and 11 selected integration targets | 482 passed, 12 targets |
| Normal dev FFmpeg desktop build | Passed |
| Initial complete host/media FFmpeg rerun | 359 passed, 1 AAC remap failure, 2 original ignores, 35 targets |
| Video transaction target with added cause context | 8 passed, 1 original ignore |
| Independent mapped-audio quality target | 5 passed |
| Frozen AAC target after filtered flat-control correction | 1 passed, original equality assertions intact |
| Integrated desktop/UI gate | Link failure before runtime: unresolved `@4HA`; not a passing desktop suite |
| Integrated normal-dev photographic native workflow | 3 views, 7 fresh processes, import/binding, invalid-binding atomicity, undo/redo, fresh reopen and default file/Error stderr isolation passed |

The initial FFmpeg gate could not compile an old integer-rate test fixture.
After its correction, complete host/media ran all 35 metadata-declared targets
and exposed the AAC control's missing anti-aliasing. That failed receipt remains
unaltered. The focused corrected target is not presented as another complete
host/media invocation. A fresh full regression is still required.

The `@4HA` symbol is confirmed undefined in the compiler-produced object; its
cause is unproved. The prior test-only four-codegen-unit configuration was used
by that failed gate. No assertion is removed and no runtime success is inferred
from this linker failure.

The [Rust/GPU CI failure](https://github.com/MAD-Producer/Kilonova/actions/runs/41944558758/job/126973427845)
hid the video task's actual error. Two later failures were poisoned shared-lock
consequences. Storage preflight is one possible alternate error, but CI did not
print its cause or capacity. Local transaction success does not establish CI's
root cause. The new diagnostic preserves the original assertion so the next
failure can identify it directly.

The photographic workflow uses the actual integrated normal-dev binary,
SHA-256 `4711c4e679f20b13bb26a32744aa0f0374cd8b591f5dda4827a24203660bd5b8`. Three later test-only
source deltas are recorded separately; every other archived build input remains
identical. OpenCV source photographs and licenses retain the attribution already
recorded in the repository. This proves separate photographed calibration poses,
not a continuous camera solve, surveyed scale, final all-precision export or a
complete tracking implementation.

## Reproduction records

[Archive](data/tracking88-integration-followup-v1.zip) and
[member hashes and scope](data/tracking88-integration-followup-v1.json) contain
controllers, exact source/command identities, original stdout/stderr and failure
receipts, corrected focused receipts, native comparisons and license files.
Every archive member was read back and SHA-256 verified.

Archive SHA-256: `799418081eb62a0a832307c4fb36fc8909e13068b61ef79ad9c521c7abc59a66`.

## November 5 regression and CI follow-up

The next full desktop/UI invocation used source `3e2e2bee`, Cargo's default
test codegen units, one test thread and the original assertions, stack and
backend. It linked successfully, then reported **1985 passed, one failed and
72 original ignores across 32 target invocations**. The failing test was
`actual_exposure_io_and_terminal_replay_reach_default_reply_channels_then_new_task_recovers`.
Its isolated child exited without a Rust panic message. Windows Application
event 16775, at `2026-11-05T02:02:05.8679779Z`, identifies the same executable
and an access violation `0xc0000005`, with an unknown module. This identifies
the failure class; it does not establish the responsible code or driver.

Running the unchanged original child directly afterward passed in 11.36 s,
including the original IO cause, terminal replay, recovery and default Error
file/stderr outputs. Its executable SHA-256 was
`35c07258215b610c6f2fb75358c3a6895c40db58348ac899b9c308e60663b8c1`.
This focused success does not replace the failed full invocation. The parent
assertion now also prints the child's exit status and code, preserving the
same success predicate so a future native crash is distinguishable from a
Rust assertion or ordinary IO error.

The host/media phase of that full invocation did not execute any tests: a
concurrent check from a divergent worktree had replaced shared Cargo metadata.
The renderer check resolved a foreign `motion-model` without `inverse_step`,
and the host compile resolved a foreign `motion-core` without `tracking::colmap`
and `tracking::document`. Those APIs exist in the integration source. This was
a validation setup error, not evidence for changing those product APIs.
Subsequent builds use a dedicated target directory for each divergent tree.
A fresh isolated all-target check of desktop, host and media with FFmpeg and
optional `motion-desktop-media/ai-depth` passed in 362.422 s, with the source
unchanged. It proves type checking, not inference or a host/media runtime pass.

In [CI run 41951382727](https://github.com/MAD-Producer/Kilonova/actions/runs/41951382727),
the Rust/GPU job annotation explicitly reports its 45-minute execution limit.
The workspace test step passed in 42 min 16 s, followed by a successful SDK
loader step. The separate optimized portable-loop compile was then cancelled
before completing. The job budget is now 60 minutes to retain time for that
existing gate; no test, gate, platform or publication requirement is removed.
All 15 workflow policy tests passed locally after this change. Required checks
for the updated commit must still finish; the cancelled job is not a pass.

The [second archive](data/tracking88-integration-followup-v2.zip) and its
[member hashes and dispositions](data/tracking88-integration-followup-v2.json)
retain the failed full invocation, both optional-depth checks, the unchanged
direct child, original CI logs and timeout annotations. Selected Windows event
metadata is included; no dump was inspected. Every archive member was read
back and compared byte for byte. Archive SHA-256:
`09b8eea538c95aee9bb102f3ae6b9a9ed194257c54c31f87fe39d03f146e30ba`.
The full regression and native-crash investigation remain pending. This
follow-up does not close issues 74, 85, 86 or 88.
