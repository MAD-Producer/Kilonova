# Issue #85: rational project adoption under implementation

This continuation is on `codex/85-rational-project-adoption`, based on the
independently reviewable clock foundation in PR #591 (`2080125b`). It is not
part of that PR's published diff. The complete 3D route and #85 remain open.
Android is retired and excluded. No panel files have been changed. Composition
dialog clock parsing and field formatting have changed; final UI acceptance is
pending and is not covered by the library or native-media results below.

The reviewable evidence archive is
[`data/frame-rate85-project-adoption-v1.zip`](data/frame-rate85-project-adoption-v1.zip),
with its [receipt](data/frame-rate85-project-adoption-v1.json). It contains 50
members, 442104 bytes, SHA-256
`051b626fd799af856404e5f2ef058fcd045b65b2e02fcbfd62925d2a6264ad82`.
All member hashes were verified after writing. It deliberately retains failed
attempts alongside successful steps and does not certify complete acceptance.

## Clock reader, cache and test-adoption follow-up

The follow-up is published on the same draft PR #597. Its evidence archive is
[`data/frame-rate85-clock-followup-v1.zip`](data/frame-rate85-clock-followup-v1.zip),
with a [receipt](data/frame-rate85-clock-followup-v1.json): 61 members,
449013 bytes, SHA-256
`1290e511794658e38d4a2af7d1094422a17750367e2bb709bac1d93fa09c54de`.
All archived member hashes were checked after writing. Original unsuccessful
attempts and initial PR CI logs are retained alongside the successful gates.

The desktop now shares a borrowed project-clock reader across settings,
recent-project recording and the audio acceptance setup. It requires exactly
two unsigned components, reduces them through the native `FrameRate`
constructor, checks exact composition bounds, and preserves native rate errors
as error sources. It neither clones the JSON clock object nor substitutes
30 fps on failure. Diagnostic messages name invalid fields without including
arbitrary supplied values. The reader does not log internally; callers retain
their existing error handling and context.

Executed verification, with frozen sources unchanged during each gate:

| Gate | Executed result | Scope and limit |
| --- | --- | --- |
| Desktop reader | 5 passed, 0 failed, 0 ignored | Actual production module compiled with a direct rustc harness and existing native dependencies; not the desktop main test binary |
| Complete motion-media rerun | 187 passed, 0 failed, 2 existing manual ignores | All original package targets, including the unchanged declared-size rejection tests; no low-space preflight or assertion relaxation |
| FFmpeg desktop check | Passed, 8.578 seconds | Production application compilation; not unit tests or interactive acceptance |
| Rational render clock | 4 passed, 0 failed, 0 ignored | Bundle/native/packed clocks, retained preview-plan identity and actual GPU output |
| Existing exposure/nested exposure tests | 17 passed, 0 failed, 0 ignored | Original tests executed with GPU tests explicitly enabled; all numerical bounds retained |
| All motion-render test targets | Compile passed | Includes the benchmark example marked as a test; does not prove complete runtime regression |
| Benchmark example fixture tests | 2 passed, 0 failed, 0 ignored | Original fixed feature and workload contracts, with the 30/1 typed expectation |
| All maintained non-desktop-app test targets | Compile passed | Workspace test compilation with FFmpeg enabled; Android excluded by scope, desktop app excluded pending panel adoption |
| Host preview-input tests | 11 passed, 0 failed, 0 ignored | Includes actual PNG decode/cache and a new rational metadata/local-clock seek regression |

The new plan-cache regression compares complete cached and fresh packed bytes
through 28 evaluations. It distinguishes 30000/1001 from 30000/1003, accepts
60000/2002 as the same canonical clock as 30000/1001, refreshes subframe shader
clocks on cache hits, and verifies exact packed-byte recovery after seeking
back. Build/hit counts independently require rate changes to invalidate the
retained plan and unchanged canonical rates to reuse it.

The GPU regression executed on an NVIDIA GeForce RTX 4070 Laptop GPU through
Vulkan. At five integer/rational rates it compared 20 cached preview captures
against fresh formal captures with exact RGBA equality, required nonempty and
time-varying output, and verified exact pixel recovery after seeking back.
Twenty generated PNGs and their rate/frame metadata are archived. The fixture
is repository-authored; no external media is used. This proves the tested
preview-plan geometry/time path, not the host's complete retained-frame or disk
cache, all renderer formats, or cross-platform output equivalence.

The host regression reuses the same actual decoded PNG across 20 requests at
five rates and several subframes, checking source/local clock identity,
canonical serialized rate metadata, independent seconds calculations and
unchanged project state. The image Arc remains shared while request sequence,
revision and clock metadata advance. It does not claim actual video decoding;
the separate native-media tests below cover that boundary.

Initial PR #597 CI at `9f18c452` exposed the old integer `exposure_scenes`
fixture. Broader local test compilation also found the equivalent
`nested_exposure` fixture, the benchmark's integer-rate expectation, and the
host preview-input expectation. These now require explicit canonical integer
rates, retaining 30/1 and 60/1 exactly; original sampling, output, cost, duration
and error assertions are not relaxed. The first broader render and maintained
test checks remain failed in their archived receipts. Their later checks pass
after those adapters; they are not relabeled retroactively.

Foundation PR #591 head `2080125b` completed CI successfully in
[run 41564820599](https://github.com/MAD-Producer/Kilonova/actions/runs/41564820599).
That result is not proof for the larger adoption diff. The five panel fixture
type errors reproduced by initial Linux/Windows CI remain outside this patch,
as do actual panel clock adoption and the main desktop test execution. Final
adoption-head CI and interactive acceptance remain required.

## Implemented and executed boundaries

- Project, composition, composition view, settings, animation recipes and
  source sampling use the canonical `FrameRate` type
- The current project format is 33 on this baseline, and animation-recipe
  format is 2; missing, numeric, zero and malformed rates are not migrated
- Composition settings require an explicit `preserve_frames` or
  `preserve_seconds` policy; keys, integer clip bounds, work areas and
  reference offsets use direct rational boundary conversions
- Output frame PTS uses exact nearest-microsecond conversion; subframe
  source clocks quantize only at their video leaf; reverse traversal still
  addresses the preceding decoder microsecond
- Unretimed nested audio retains a reduced rational origin through the
  hierarchy before selecting absolute PCM edges
- Render Scene and original-input clocks carry both rate components;
  expression `frameRate` remains a numeric approximation and the exact
  `frameRateRatio` is exposed separately
- Preview-plan identity includes canonical numerator and denominator
- Composition bundle version 2 stores numerator at node+64 and denominator
  at node+68 while retaining the 80-byte node and original f64 sample frame;
  host export capabilities report the actual version constant
- Audio mixer duration and imported audio/video clip ends use explicit
  rational floor/ceil conversions without an integer FPS cast

Actual `cargo test -p motion-model --locked --offline --no-fail-fast --
--test-threads=1` exited 0: **223 passed, 0 failed, 0 ignored**, across 45
target summaries including the documentation target. Sources were hashed
before/after and unchanged. The test also exercises every original integer
frame timestamp up to frame 35999 at eight rates, mixed-rate nested audio,
unrounded fractional media coordinates, persistence, explicit retiming and
the unchanged curve/hold/rotation/spatial tests. Raw evidence is currently
retained externally at `artifacts/pr-queue/frame-rate85-full-model-v1`.

Actual `cargo check -p motion-core -p motion-render -p motion-host
--no-default-features --locked --offline` exited 0 after correcting four
render, four media and one host type errors. The original unsuccessful
checks remain in `frame-rate85-model-adoption-v2/runtime-adoption-check-v*.log`.
This library check does not verify optional features, the desktop binary,
SDK consumers, exports or the full workspace.

Fixture changes follow the intentional wire/type change: integer-rate
constructors replace raw typed fields; expected query rates become canonical
objects. Zero-rate rejection now occurs before a typed document exists, so
the old invalid-rate tests also check wire/constructor rejection and retain
context checks with an out-of-contract positive rate. Numeric sampling
expectations, tolerances, collision/refusal behavior and source data are not
weakened. One intermediate selected test failed due to an incorrect import;
its receipt is retained separately, followed by the successful twelve-test
selected run and full model run.

## Remaining gates

### Current complete model/core regression

The first combined `motion-model` / `motion-core` package run failed one
unchanged assertion: an invalid original composition clock was accepted when
resizing a full work area with `preserve_frames`. The typed-rate port had
removed the original invalid-clock guard without checking the composition's
legal range. Production `WorkArea::resized` now checks both clocks against the
composition range before the full-range shortcut. The original assertion and
atomicity check remain intact; the failure receipt is retained.

The second run used the same complete package command, with no failed target
filtered out: **774 passed, 0 failed, 1 existing ignored manual performance
test**, 132 target summaries, exit 0, 451.141 seconds. Sources were unchanged
during execution. Evidence: `artifacts/pr-queue/frame-rate85-model-core-v2`.
This supersedes the earlier model-only result for the same production model
and core adoption source. It is not complete rendering, host or desktop proof.

### Native media, muxing and optional depth

`frame-rate85-native-media-v2` explicitly enabled `ffmpeg,gpu-tests` and ran
the four actual desktop-media integration targets `libav`, `layer_time_audio`,
`layer_time_video` and `rational_export_clock`: **18 passed, 0 failed, 0 ignored**,
exit 0, 51.812 seconds, frozen sources unchanged. The first controller run had
correctly refused to count three disabled GPU tests as complete evidence; its
receipt remains available under `frame-rate85-native-media-v1`.

The native tests include VFR source-picture selection at fractional composition
frames in both seek directions, existing frozen GPU PNG/nested/exposure
exports, authored PCM comparisons, and real MP4 encoding/demux at 24,
24000/1001, 30000/1001 and 60000/1001. Four retained MP4 files and four timing
receipts verify native packet PTS/durations and exact 48 kHz audio tails. An
initial 24 fps test found 73968 samples instead of the required 74000: the
muxer's default 1 kHz movie clock truncated the edit-list audio duration.
Setting `movie_timescale=48000` fixes that actual failure without changing the
assertion. Container edit-list video duration can still be quantized to that
movie clock; exact native video packet timing is a separate verified property.

Optional AI-depth video target selection now returns typed failures through
request, cache, admission and synchronization guards instead of turning clock
errors into misses. The original owned depth state is restored on a failed
synchronization guard. Feature check passed; the two actual clock unit tests
passed with no ignored tests (`host-depth-clock-test-v1.log`). This is not full
AI-depth package or model-quality acceptance.

### Host/API and remaining UI boundary

The actual host command/snapshot/history/save/reopen test and eleven existing
failure-boundary tests passed: **14 passed, 0 failed, 0 ignored**, exit 0,
14.672 seconds, frozen sources unchanged, `frame-rate85-host-contract-v2`.
The original failed v1 check records an unadapted integer test fixture, which
was changed to an explicit typed integer constructor without changing its
overflow oracle. The host now advertises canonical rational presets and a
required explicit timing policy. The composition request envelope remains
version 1; its FPS field format is reported separately as version 2.

The later runtime gate re-ran the host tests with export clock-layout metadata:
**14 passed, 0 failed, 0 ignored**. It also passed all six original bundle
boundary tests and the two actual rational bundle/native+packed shader clock
tests: **8 passed, 0 failed, 0 ignored**. Native f32 clock bits match the written
packed uniforms for local subframes at three fractional rates; a seek back to
the original fractional frame reproduces the original complete packed bytes.
This tests actual plan construction and packing, not rendered GPU equivalence.

Both combined runtime attempts remain failed overall. In v1, fourteen compiler
errors identified old media test callers that had not unwrapped the new checked
clock result or adapted their explicit integer-rate fixture. The original
numeric/time/PCM assertions remain unchanged. A separate v1 desktop compile
ended with rustc `STATUS_ACCESS_VIOLATION`; no root cause or driver attribution
is claimed. V2 completed the full media suite with **184 passed, 3 failed,
2 pre-existing ignored manual tests**. The first failure is a declared-size
boundary test that expects to reach the length-mismatch branch; the production
copy preflight checks available space first, and E: had less than 512 MiB free
(the test also declares a 2 GiB source). The remaining two failures are poisoned
test mutexes after that assertion. The adequate-space follow-up rerun above
passed the complete package; neither the preflight nor the existing assertions
have been weakened. The original v2 receipt remains failed.

The v2 desktop check identified one visibility error in the new dialog timing
helper. After correcting that helper to `pub(super)`, a fresh actual
`cargo check -p motion-desktop --features ffmpeg --no-default-features --locked
--offline` passed in 8.26 seconds. The original unsuccessful attempts remain
retained. This compile is not a desktop unit-test, UI, or full-workspace result.

The actual version-2 writer/nested-frame test passed with exit 0 and frozen
source identity (`frame-rate85-bundle-v1`, 144.547 seconds including the cold
build). A second run also executed all six existing bundle boundary tests:
**7 passed, 0 failed, 0 ignored**, 11.985 seconds, with sources unchanged.
Evidence is retained at `artifacts/pr-queue/frame-rate85-bundle-v2`.

The old writer oracle still emits version 1 and only accepts integer rates.
Tests transform exactly the declared version word and previously reserved
denominator words before the original complete byte comparisons. All other
bytes, reference/parent/video bindings, typed failures, limits, recovery and
the original seven-allocation reduction assertion remain checked. This is
test-only treatment of the intentional format change, not a legacy reader
or a relaxation of numerical/output thresholds. Fractional clocks and exact
f64 node times are separately verified with the real Scene and new writer.

Desktop audio output, analysis, frozen movie/export manifests, media imports,
render queue and workspace metadata have been adapted. Recent-project FPS is
an approximate display-only number; the actual project clock remains canonical
and exact. Composition dialogs parse exact decimal/ratio input and explicit
drop/non-drop duration labels. The existing frame-rate row now contains a
visible timing-policy dropdown for settings, with separate draft selection and
Apply. Menu Enter commits the choice without applying the composition; Escape
closes the menu. Corresponding interaction tests have been added, but are not
yet executable under the unmodified panel test fixtures, and actual UI images
are still required. No interactive acceptance is claimed from compilation.

Necessary panel clock consumers remain unmodified under the user's original
scope restriction. The pending scope question points to the concrete, checked
proposal `target/silent-failure-validation/frame-rate85-panel-adoption-proposal.patch`
(four test files, five type initializations). The latest all-desktop-tests check
stopped on those five errors. Runtime panel readers that currently fall back to
30 fps also need authorized adoption, meaningful tests and actual UI images.
No desktop test pass or complete interactive rational-project support is claimed.

The SDK clock reference documents project/rate/bundle version boundaries,
explicit rounding and the f32 shader precision boundary. Documentation alone
does not establish each consumer's implementation or output equivalence.

Remaining #85 requirements include final API/SDK/shader consumer validation,
interactive drop/non-drop timecode display and timing-choice acceptance, integer
and fractional cache behavior under rendering, complete end-to-end desktop
output, complete maintained-package regression and final-head cross-platform CI.
The #88 stack has separate format changes; integration must establish one
current format and shared clock rather than adding migration fallbacks.
No issue closure or full-route completion is claimed here.
