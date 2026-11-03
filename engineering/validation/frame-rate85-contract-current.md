# Issue #85: exact frame-rate primitive, project adoption pending

Base: maintained main `84e0be9b`. This is an implementation step toward issue
#85, not delivery of rational project FPS. Project/Composition still store
`u32` FPS; no setting, UI, migration, render clock or export behavior changed.
Android is retired and excluded. The original issue and 3D route remain open.

## Implemented contract

`motion_model::FrameRate` stores a reduced positive numerator/denominator.
Reduced components are bounded at 1,000,000; reduction precedes that check.
Canonical equality and hashing preserve the distinction between 2997/100 and
30000/1001. The explicit wire object requires both fields and rejects unknown
fields, zero, fractional components and non-object representations. There is
no default FPS, legacy-document fallback or implicit decimal-rate inference.

Integer frame/tick and cross-rate boundary conversions use i128 arithmetic,
one original signed index, and explicit Floor/Ceil/NearestTiesAwayFromZero.
Timebases are bounded, and unrepresentable i64 results return the original
numeric error rather than saturating. No duration is accumulated per frame.
The quantizing functions are for edit/clip/media boundaries; they must not
replace fractional renderer evaluation or round subframe cache keys.

## Executed evidence

`cargo test -p motion-model --test frame_rate --locked --offline --
--test-threads=1` exited 0: **5 passed, 0 failed, 0 ignored**. The original
controller captured source identity before and after execution, raw stdout/
stderr, command, process identity and the maintained-main base. It used an
isolated E-drive target and the existing offline dependency cache.

The controls exercise canonical/invalid wire data, integer clock preservation,
24000/1001, 30000/1001 and 60000/1001 over long sequences, exact 48k audio
boundaries, negative half ties, cross-rate clip rounding, zero timebases,
maximum signed indices and overflow rejection. Original project/curve tests
were not modified. This selected target is not a new full model/workspace gate.

Evidence: [manifest](data/frame-rate85-contract-v1.json) and
[source/log archive](data/frame-rate85-contract-v1.zip).

## Required continuation

| Boundary | Remaining implementation and proof |
| --- | --- |
| Project/composition schema | Adopt FrameRate, require the current serialized version and canonical rate object; preserve integer sampling results |
| FPS changes | Explicit keep-frame versus keep-seconds policy for duration, keys, clips, offsets, camera/model animation and undo |
| Fractional nested sampling | Unrounded signed rational mapping, source PTS/VFR and explicit media rounding; preserve hold, overshoot, rotations and cache identity |
| Audio | Original sample clock, 48k boundaries, nested/retimed clips and export synchronization |
| Render/export/SDK | Versioned binary plan and shader/SDK clock propagation, output timestamps and roundtrip gates |
| Timecode | Drop/non-drop display rules across minute/hour boundaries without dropping actual frames |
| Product/API and regression | Callable settings/API, persistence/history, native preview/frozen output, complete maintained regression and final-head CI |

The source-rate helper on the separate #88 stack does not establish rational
project FPS. It must converge with this common contract during integration,
rather than leaving two independent clock models. The open #84 I/O and #65
frame-cache dependencies and their actual coverage remain separate boundaries.
