# Native prerender coordinator (#82)

Status: component accepted at `edf5d0f6cbb5b98aef16db01e544f26b09c2f3a9`. Engine/Session presentation and
foreground P95 acceptance are still pending; capability remains disabled.
Android is excluded. No panels, visuals, dependencies or assets changed.

The foreground owner joins the directional scheduler, the single native worker
and the exact native frame cache. Scheduler next_key only inspects eligibility.
An actual shared-budget foreground destination is reserved before taking the
scheduler ticket. A single active prepare/render job owns its slot and destination
until acknowledgment. Cancellation/generation changes cannot install old work.
Frame keys and native byte contracts are checked before upload. Cache lookup
counts remain distinct from future submitted/presented-hit counters.

Disabled initialization creates no thread, device or cache. Explicit disable
cancels and closes the old owner without joining a running thread. Re-enable
can replace only that explicitly retired owner after its thread and retained
CPU permits have retired. Spontaneous worker failure does not auto-restart.

Native cache readback retains the admitted source and allocates only shared-budget
staging. It checks renderer ownership and exact canonical storage; it performs no
clamp, alpha transform or display conversion. It is synchronous export/validation
code, never preview cache polling.

## Actual evidence

- Scheduler: 13 passed, zero failures/exclusions
- Producer/worker/coordinator: 18 passed, zero failures, one manual exclusion
- Four-precision producer control: explicitly executed, one passed, none excluded
- Four-precision upload/raw-readback control: explicitly executed, one passed,
  none excluded; exact encoded UNORM bytes, HDR/negative float and alpha retained
- Full media regression: 90 passed, zero failures, two manual exclusions
- Desktop ffmpeg/host ai-depth checking and provenance passed; source unchanged

Focused counts overlap the full media run. The precision producer exclusion was
executed separately; the existing keyframe timing manual control was not rerun.
This is not a new full-renderer/workspace or cross-platform acceptance.

Four coordinator controls cover cold/warm hit accounting and exact native red
pixels, destination admission before dispatch, disabled initialization, actual
disable/re-enable retirement, invalidation before acknowledgment, stale refusal
and subsequent recovery (combined across the four tests). Existing pixel
assertions were not weakened.

Retained failures: v1 rejected test access to a private texture; v2 exposed the
actual missing COPY_DST flag on an intermediate capture texture. The final
readback avoids that intermediate texture. Original failures and successful
source-bound logs are all archived.

Evidence: `data/native-prerender-coordinator-v1.zip`; SHA-256 `4a6007ba780484af14baf826c2f5918bc6459ec20da268ca595da2b6d229a7e1`;
411043 bytes; 34 entries, each read back and hash-checked.

## Remaining

Connect foreground demand interruption and default logging to Engine, Session
cache-to-scratch conversion/presentation, authoritative revision/source/device/
output-profile invalidation, real observer/quality coverage, full regression
and paired playback/seek/edit foreground P95. Prove Info start is flushed before
a controlled handler block, plus actual default file/Error stderr/stdout isolation.
No disk cache exists; no disk corruption/full-disk acceptance is claimed here.
