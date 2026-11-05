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

These are test-only changes. No production behavior, panel, style, dependency,
timeout, retry or test threshold changes are included.

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
