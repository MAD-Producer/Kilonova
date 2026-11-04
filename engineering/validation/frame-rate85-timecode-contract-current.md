# Rational frame-rate entry and explicit timecode labels (#85)

Status: independently verified model API increment; project/UI adoption remains open.

## Implemented contract

`FrameRate::from_str` accepts bounded positive integers, exact decimal values,
and exact `numerator/denominator` input. Decimal 29.97 stays 2997/100; it never
silently becomes 30000/1001. Reduction precedes the existing component bound.
The strict JSON object contract is unchanged by this text-entry API.

`format_timecode` and `parse_timecode` require an explicit mode. Non-drop mode
supports maintained integer rates and their exact /1.001 counterparts. Other
custom rational clocks remain usable as frame counts but are rejected for
these timecode labels. Drop mode accepts only 30000/1001 and 60000/1001.
It omits labels, never pictures, and does not alter the render or media clock.
Omitted minute labels, wrong separators, malformed fields and signed overflow
are errors. Negative indices use a signed label magnitude. Editing durations
continue past 24 hours instead of wrapping. The 60-rate display counts each
picture; this is not a packed LTC/VITC frame-pair implementation.

## Evidence

The staged model library was tested independently from the unfinished project
port. The controller saved all unfinished model changes in a recoverable
external snapshot, restored the actual staged library, ran the tests, verified
its source hashes, then restored every saved file byte-for-byte in `finally`.

Command: `cargo test -p motion-model --test frame_rate --test timecode --locked
--offline -- --test-threads=1`.

Result: **12 passed, 0 failed, 0 ignored**, including five new tests and seven
existing exact-clock tests. New tests enumerate every permitted label in an
hour at both drop rates: **323,676 independent forward-count comparisons**,
plus minute/ten-minute/hour/day boundaries, integer controls, signed i64
extremes, exact-decimal distinctions and rejection cases. Existing assertions
and tolerances were not relaxed. The archived controller, logs, receipt and
source copies have been read back and verified byte-for-byte.

See [receipt](data/frame-rate85-timecode-v1.json) and
[evidence archive](data/frame-rate85-timecode-v1.zip).

## Provenance and limits

Behavioral reference: [SMPTE ST 12-1:2014, sections 5.2.1, 5.2.2 and 7.2](https://pub.smpte.org/pub/st12-1/st0012-1-2014.pdf).
The implementation and forward-enumeration oracle were independently authored.
No standard PDF, upstream source, proprietary binary, artwork, font or other
third-party asset is included. No dependency was added.

This increment does not change production Project/Composition FPS fields,
dialogs, panels, serialization versions, SDK/shader clocks, media export or
history. Their broader #85 adoption remains separate and uncompleted. It does
not close #85, #86, #88 or the full 3D route. Cross-platform CI must validate the
published commit separately from these local model tests.
