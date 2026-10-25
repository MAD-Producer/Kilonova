# Pointwise fusion prototype: current delivery ledger

Issue #73 depends on #60 and #72. PR #489 merged as `40efcd35` after all
platform, Rust/GPU and audit checks passed; issue #72 is closed. This prototype
has native, failure-output and representative-cost acceptance. Integrated full
regression at `bcbe367b` passed 1,859 tests across 142 targets, with zero failures
and four pre-existing ignores. Subsequent conservative alpha/consumer guards
have targeted native and failure-output acceptance, not a second full-run claim.
Remote CI of the final submitted source remains required before closing #73.
An earlier full effects/render run at `ebd0380c` passed 716 tests across 119
targets, with zero failures and one existing optional external-image ignore.

The dependent branch started at `b90b52a1` on #72. Metadata checks on 2026-10-25
found no competing fusion/Render DAG branch or open PR. Other PR diffs were not
reviewed. No subagents or overlapping owned compilers were used.

## Scope and status

| Item | Current state | Required evidence / remaining work |
| --- | --- | --- |
| 73-P1 | Implementation-bound eligibility for current invert/tint: frozen normalized source hashes, exact definition and both compiled backends, SDK/boundary restrictions | Seven IR/backend tests pass, including forged source, compiled-backend and semantic changes |
| 73-P2 | Bounded typed stage/parameter namespaces, distinct uniform slots, 16-stage/32-cell caps, values excluded from structure keys | IR aliasing/numeric-update/bound tests and actual eligible native numeric-update reuse checks pass; operation-order change creates exactly one new portable group |
| 73-P3 | Explicit canonical linear/premultiplied and working sRGB/straight contracts for four precisions; generated quantization at the original storage boundaries | WGSL and GLSL compile; eligible native pixel comparisons pass in all four modes; alpha-changing draws in every precision remain an explicit unproved boundary |
| 73-P4 | Opt-in native tail grouping with coordinate/bounds/identity/lifetime guards, bounded retained-reason counts and a 64-entry compiler cache | Actual grouping, allocation anti-aliasing and registry-replacement checks pass; resources, alternate inputs and later unproved consumers retain original passes |
| 73-P5 | Fused/unfused actual GPU comparisons, including alpha, spaces, quantization, HDR, boundaries, blur/warp retention and forged implementations | Seven native tests and one procedural-alpha test pass: 32 parameter/order variants (13 fused, eight normal zero-opacity bypasses, 11 exact retained paths), four HDR cases (three fused, one exact retained), 32 tiny-alpha exact retained cases, resource/matte and compositing boundaries, lifetimes/registry, neighborhood and transfer-order cases |
| 73-P6 | Pass/loop/sample counts, portable/native pipeline compile counters/timers, logical traffic estimates and opt-in native command observations | Eight actual 1920x1080 mode/order studies pass with adapter metadata, separate cold/native/portable costs, command traces and logical traffic; physical bus counters unavailable, no general speedup claim |
| 73-P7 | Default-level fault observation, bounded caches/reasons, current regression, CI and completion record | Real portable/native failures pass default production file/Error-stderr/stdout-isolation acceptance; bounded scope/lock/unwind tests pass; integrated full regression passed at `bcbe367b`; final-source native/fault-output guards pass, remote CI pending |

Only one owned compiler/gate runs at a time. Issue #72's completed results are
not attributed to this new source. Receipt scopes below distinguish the integrated
full run from the subsequent focused guard regressions.

## Execution semantics to preserve

The current tint and invert implementations are single-pass sRGB/straight image
effects without padding or changed output bounds. Their native SDK path still
uses three passes: canonical color to working storage, the effect, then working
storage to canonical color. The effect's opacity blends with its original input
in that working representation. Every physical store can quantize. Fusion must
preserve these boundaries, including sRGB attachment storage for UNORM8 canonical
color, plain UNORM8 working storage, integer UNORM16, binary16 and binary32.

Only exact recognized implementations may acquire the initial operation proof.
An updated reference package cannot silently bless old IR: source fingerprints
are frozen independently of the reference package. The claimed package hash,
effect name or pointwise metadata flag cannot establish eligibility. Actual
shader representations and semantic metadata must agree as well. Unknown,
neighborhood, resource, geometry, state or changed-coordinate paths keep their
original passes and a bounded reason until separately proved.

The native integration must also prove actual pass footprints and source/input
identity; package eligibility alone cannot establish those runtime facts. It
must avoid sampling an attachment while writing it, retain admission/physical
lifetime checks, and attribute a fused group's measurements to that group.
Parameters update data; only changed structure specializes shader code. Numeric
and structural tests must inspect real compilation counters after integration.

An initial Float32 development step cannot replace the requested quantization
work or serve as completion of this issue. All four current project precisions
need explicit fused/unfused acceptance and declared tolerances. No universal
speedup or compatibility with an unproved shader family is claimed.

## Native prototype policy

`Renderer::set_pointwise_fusion(true)` explicitly selects the prototype; ordinary
rendering keeps the existing execution policy. The optimizer runs after original
effect admission and before final physical lifetime placement. It recognizes only
the exact three-pass runtime footprint of the proved image effects. An isolated
effect, changed implementation, changed coordinates, alternate image input,
resource, neighboring operation or oversized uniform layout keeps its passes.

For a recognized group, the existing raster utility with unit modulation copies
the canonical input to a separate logical value, and one fused shader writes the
original canonical output. This avoids read/write feedback and avoids an extra
unpremultiply operation on tiny alpha. The shader emulates each original working
and intermediate canonical store. The final physical attachment performs the last
canonical store. Final allocation derives from the rewritten lifetimes. Existing
unfused admission still applies: the prototype does not admit workloads that the
original budget would refuse.

Generated code specializes operation order and precision, never values or object
IDs. Numeric changes pack distinct namespaced uniform cells. A registry replacement
retires cached program IDs; the lifetime compile counter remains monotonic. Both
successful and failed portable compilations occupy a bounded cache entry. Native
pipeline failure records prevent retrying the same program on subsequent frames.
Normal optimization refusal does not enter render-fault diagnostics or emit a
per-frame warning.

The native comparison declares absolute linear-premultiplied bounds before
execution: UNORM8 0.012, UNORM16 8/65535, binary16 0.005, binary32 0.00002. The HDR
producer case uses 0.008 for binary16 and 0.00004 for binary32. The original PNG-based ordinary probes observed
maximum errors are 0.0070993127 (UNORM8), 0.0000610361 (UNORM16), 0.0034179688
(binary16), and 0.0000021979 (binary32). These are bounded probe results, not
evidence of exact byte identity for fused paths. A
failing result must be investigated, not hidden by relaxing an existing assertion.

Measurements separate portable CPU validation/GLSL generation, native pipeline
creation plus validation waits, submit/readback wall time, and native command
intervals. Null command measurements mean unsupported or pending, never zero.
Logical input/source/attachment traffic is an upper-bound estimate; it is not a
physical bus bandwidth measurement, and no 1/N acceleration is promised.

## Acceptance stages and retained failures

| Receipt | Disposition |
| --- | --- |
| `issue73-stage-effects-ir-v1` | Failed: six tests exposed the incorrect assumption that Invert uses Transparent edges; its current contract uses Clamp |
| `issue73-stage-effects-ir-v2` | Failed: five passed, two exposed the portable compiler's base loop-work multiplier of one and disabled binary16-in-f32 builtins |
| `issue73-stage-effects-ir-v3` | Failed: packing builtins need the same disabled capability; changing builtin names did not establish portability |
| `issue73-stage-effects-ir-v4` | Seven tests pass, zero ignored; generated binary16 rounding now uses independently authored binary32 bit arithmetic and power-of-two subnormal spacing, with no extra shader capability |
| `issue73-stage-native-fusion-v1` | Failed: two passed; the new test incorrectly assumed zero-opacity effects produce passes, and its sparse-alpha fixture did not retain an HDR peak after filtering |
| `issue73-stage-native-fusion-v2` | Failed: three passed; UNORM8 alpha modification magnified intermediate rounding to 0.0234066, beyond unchanged 0.012 bound |
| `issue73-stage-native-fusion-v3` | Failed: three passed; UNORM16 alpha modification magnified rounding to 0.00112917, beyond unchanged 8/65535 bound |
| `issue73-stage-native-fusion-v4` | Four tests pass after retaining integer alpha-changing draws in full; those retained paths must match every native pixel exactly |
| `issue73-stage-native-fusion-v5` | Four tests pass with a new downstream-consumer boundary and both between/after blur/warp placements retaining identical original passes |
| `issue73-stage-native-fusion-v6` | Four tests pass with actual timestamp queries explicitly enabled; all four modes retain native command intervals, words and period, alongside 32 comparison/measurement rows |

All original pixel bounds remain unchanged. Zero-opacity cases now assert the
existing no-work behavior instead of forcing fusion. The HDR fixture was made
opaque before its analytic producer so both original and optimized paths must
actually retain red above 1.1; this is a fixture correction, not a lower HDR
assertion. Integer alpha failures led to an execution guard, not a wider bound.

The alpha guard now retains the **whole draw** in every precision, including
earlier candidate groups. Initial integer-only acceptance was insufficient: the
procedural subnormal-alpha follow-up below supplies actual float counterexamples.
A later unproved consumer also prevents replacing an upstream partial group:
permitted local rounding errors cannot be assumed safe across an arbitrary
threshold or gain. Only a proved tail can currently fuse. Native eligible two-
effect draws use three passes rather than seven, including materialization; the
effect group itself uses two rather than six. This is not a whole-frame speedup.

The 32-row JSON is under the successful native receipt's `output-artifacts`.
The original v6 receipt had four fused UNORM16 variants. The current follow-up
retains its three forward non-alpha variants as well: UNORM16 now has one fused,
two zero-opacity and five retained variants. UNORM8 retains four fused, two
zero-opacity and two retained-alpha variants; floating modes each now have four
fused, two zero-opacity and two exact retained-alpha variants. The current matrix
therefore has 13 fused, eight bypass and 11 retained cases. Historical receipts
retain their original counts.
Eligible numeric updates add no native or portable compilation. Entering the
retained path for the first time can create the original native pipelines that
the optimized path had not needed; the portable group counter does not change.
Reversing operation order changes the structure key and adds one portable group.

The first v5 microprobe did not request timestamp features, so it correctly
reported unavailable rather than zero. The explicit v6 setup requests supported
features and records actual native intervals. These include markers/scheduling;
65x33 probes are not representative performance benchmarks. No physical GPU
bandwidth counter or Android device measurement is claimed.

## Follow-up acceptance and conservative quantization boundary

| Receipt under `artifacts/pr-queue/` | Actual result |
| --- | --- |
| `issue73-stage-effects-render-full-v1` | Original committed prototype: 716 passed, zero failed, one existing optional external-input ignore; source unchanged |
| `issue73-followup-scope-v1` | One test with isolated Info/Debug, nested guard, unwind and overflow child modes; logger can acquire business state and TLS observation borrows |
| `issue73-followup-file-v1` | Real generated-source parser fault and actual native shader validation fault, under the real Session and production logger, with `MOTION_LOG` absent; both files and Error stderr receive one failure; stdout receives none |
| `issue73-followup-cost-v1` | Failed: dense UNORM16 forward chain exceeds the unchanged 8/65535 bound; later modes were not executed |
| `issue73-followup-cost-v2` | Failed: all four forward cases retained before assertion; UNORM16 error 9/65535 |
| `issue73-followup-cost-v3` | Failed: eight mode/order studies retained; UNORM16 Tint-before-Invert exceeds its bound; reverse order remains within it |
| `issue73-followup-cost-v4` | Eight studies pass after retaining the forward UNORM16 draw; retained pixels match exactly, every other original tolerance remains unchanged |
| `issue73-followup-native-v1` | Five native tests pass, including the retained-order regression and reverse-order fusion; all prior pixel tolerances unchanged |

At 1920x1080, UNORM16 Tint followed by Invert produced a maximum linear error of
`0.0001373311970702673` (9/65535), above the original 8/65535 bound. A dark
intermediate rounding difference can be amplified by subsequent transfer and
inversion; the precise driver arithmetic that first creates that difference is
not isolated. This is not a proof that a smaller passing image or an arbitrary
numeric parameter range is safe. The native planner now conservatively retains
the **whole draw** whenever a proved Tint precedes a proved Invert in UNORM16,
reporting `quantized_transfer_boundary`. Alpha-changing draws in all four modes
retain their original stores with `alpha_boundary` after the procedural follow-up. No pixel bound was raised. Reversed UNORM16 chains and
all three other precisions still have native fused acceptance. The portable IR
remains able to compile all four storage modes; native eligibility adds the
conservative guard. This is an execution-safety change to the opt-in prototype,
separate from the observation changes.

The new production file test repeats each actual failed operation 70 times and
checks identical causes with no native rebuild. Replacing the renderer permits
real rendering/readback again, and 65 healthy draws reuse both caches without
additional start records. Debug-only, `gpu-tests`-gated diagnostics corrupt one
generated shader before a real parser/device validation call. Release builds
contain no injection API and no automatic corruption path. Device-poll errors
keep their existing return behavior; this test does not claim device-loss or
GPU-hang fault injection.

Cold compiler events use a dedicated bounded scope: 64 normal events plus one
first overflow Error, an omitted count, 128-character structure keys, 16 effect
IDs and 2048-character causes. Debug events are filtered before retaining data.
The host enters the scope before acquiring Session; scope exit invokes loggers
after guard release, including unwind, without a TLS borrow. Direct renderer
callers observe Info start before compilation. Host-scoped compiler phase starts
are explicitly `deferred=true`; they are **not** a pre-handler hang signal. The
existing engine pre-handler Info start and public in-flight state remain the
worker signal, with healthy Present calls filtered. A compile cache miss can
produce cold Info events; healthy cached draws do not add per-frame IO.

The explicit `pointwise_fusion_cost` example is not a performance test executed
by the ordinary unit-test harness. It measures two operation orders in all four
modes at 1920x1080: one image, two effects, three warmup draws and twelve numeric
updates, alternating execution order. It retains per-sample CPU submit+completion
waits, separate cold CPU pipeline costs, native timestamp traces, shader/loop/
sample metrics and logical traffic. Full image readback/comparison is outside the
frame timer. This is an exploratory adapter-specific study, not a cross-platform
performance guarantee. The adapter was NVIDIA GeForce RTX 4070 Laptop GPU,
Vulkan, driver 591.86. The retained forward UNORM16 case uses seven passes; the
other seven cases use three instead of seven. Physical bus bandwidth counters
are unavailable; logical bytes and command intervals are reported separately.
Android device acceptance remains outside the maintained desktop scope.

## Final consumer and subnormal-alpha guards

| Receipt under `artifacts/pr-queue/` | Actual result and scope |
| --- | --- |
| `issue73-followup-integrated-full-v1` | At `bcbe367b`: 1,859 passed, zero failed, four pre-existing ignores, 142 targets; source unchanged; desktop 963 passed/three ignores; full command includes effects/render/host/desktop with desktop FFmpeg |
| `issue73-followup-boundary-red-v1` | Expected pre-fix failure: an externally consumed producer still fused; establishes a missing eligibility boundary, not a measured pixel error |
| `issue73-followup-extreme-v1` | Failed fixture: procedural package shader path was outside the loader's shader directory; no numerical conclusion |
| `issue73-followup-extreme-v2` | Failed native pixel bounds; all 32 rows retained before assertion, with actual positive float alpha below 1e-6 |
| `issue73-followup-extreme-v3` | One test/32 cases pass after all-mode alpha guard; every native pixel matches exactly, no portable fusion compile; original bounds unchanged |
| `issue73-followup-native-v2` | Seven actual GPU tests pass; 13 fused/eight bypass/11 exact retained ordinary cases; four HDR modes and external/compositing guards pass |
| `issue73-followup-file-v2` | Final guard source passes the real production logger portable/native failure, suppression, recovery and stdout-isolation test with `MOTION_LOG` absent |

The independent procedural producer has alpha values below the PNG grid, zero,
1/255 and one, plus an HDR channel. Its alpha-changing Invert consumer exposed
maximum error 0.03515625 in Float16 (bound 0.005) and 0.03093111515045166 in
Float32 (bound 0.00002). The first arithmetic/filtering divergence has not been
isolated; these results do not establish a particular driver or FMA cause.
Retaining the full draw in all four precisions addresses the demonstrated unsafe
optimization. All 32 retained cases now have zero error; RGB-only fusion remains
covered in all four precisions, including reverse UNORM16 and HDR float cases.
The original per-mode bounds and exact retained-pixel assertions were not relaxed.

An effected image-input or matte consumer now retains its producer's original
stores and reports `external_consumer_boundary`. A raw-source reader does not
consume the effected output and does not inhibit otherwise eligible fusion.
Visible non-normal/transfer-space blends and effect-bearing adjustment backdrops
conservatively retain the frame's candidate draws with `compositing_boundary`.
The initial local proof does not establish their end-to-end error amplification.
Native tests verify finite nonempty output, unchanged pass counts, zero fusion
compilation and exact pixels for each retained path in all four precisions.
These are normal optimization refusals with bounded counts, not render faults.

The 68m49s integrated full run preceded these opt-in guards. Its four ignores
are the existing actual audio-device clock probe, two explicit Release real-
media pressure/native-window studies, and the optional external-image probe.
The later guards return after the unchanged disabled-prototype early exit; all
new enabled-path branches are exercised by the native/procedural tests, and real
fault output is rechecked. No second full integrated run is claimed. Remote CI
still gates the final PR source. No UI, panel, dependencies or assets changed.

The [source-bound evidence bundle](data/pointwise-fusion-acceptance-bcbe367b.zip)
contains 19 stage receipts, logs, matrices and selected pre/post-guard source
snapshots, with an entry SHA-256 manifest verified after archive creation. Receipt source
hashes and retained failed runs distinguish observations from proof and keep the
historical runs' original eligibility/counts visible.

## Provenance

No new dependency, third-party asset or Adobe material is introduced. Operation
references are independently authored repository shaders. Existing compatibility
identifiers are only read from current packages; no new compatibility name is
introduced. This work does not change UI or add 3D nodes. Issue #78 remains a
separate Render DAG milestone after the fusion proof and acceptance.
