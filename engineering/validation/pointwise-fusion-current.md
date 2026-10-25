# Pointwise fusion prototype: current delivery ledger

Issue #73 depends on #60 and #72. This prototype now has native acceptance
evidence, but full regression, fault-output verification, integration and remote
CI are unfinished. The dependent branch started at `b90b52a1` on the issue #72
branch while that branch's final full regression ran. Issue #72 now has a green
full workspace receipt and PR #489. This dependent branch must be
integrated against merged main before its own PR. Main was freshly fetched as
`6bdb692a` on 2026-10-25. Open-PR and local-branch metadata showed no overlapping
fusion or Render DAG work; unrelated PR diffs were not reviewed.

## Scope and status

| Item | Current state | Required evidence / remaining work |
| --- | --- | --- |
| 73-P1 | Implementation-bound eligibility for current invert/tint: frozen normalized source hashes, exact definition and both compiled backends, SDK/boundary restrictions | Seven IR/backend tests pass, including forged source, compiled-backend and semantic changes |
| 73-P2 | Bounded typed stage/parameter namespaces, distinct uniform slots, 16-stage/32-cell caps, values excluded from structure keys | IR aliasing/numeric-update/bound tests and actual eligible native numeric-update reuse checks pass; operation-order change creates exactly one new portable group |
| 73-P3 | Explicit canonical linear/premultiplied and working sRGB/straight contracts for four precisions; generated quantization at the original storage boundaries | WGSL and GLSL compile; eligible native pixel comparisons pass in all four modes; integer alpha-changing draws remain an explicit unproved boundary |
| 73-P4 | Opt-in native tail grouping with coordinate/bounds/identity/lifetime guards, bounded retained-reason counts and a 64-entry compiler cache | Actual grouping, allocation anti-aliasing and registry-replacement checks pass; resources, alternate inputs and later unproved consumers retain original passes |
| 73-P5 | Fused/unfused actual GPU comparisons, including alpha, spaces, quantization, HDR, boundaries, blur/warp retention and forged implementations | Four tests pass: 32 parameter/order variants (20 fused, eight normal zero-opacity bypasses, four exact retained integer-alpha paths), four HDR producer cases, plus physical lifetime/registry and neighborhood boundary cases |
| 73-P6 | Pass/loop/sample counts, portable/native pipeline compile counters/timers, logical traffic estimates and opt-in native command observations | Actual 65x33 microprobe measurements retained for all four modes; physical bus counters unavailable; larger representative studies remain pending, no speedup claim |
| 73-P7 | Default-level fault observation, bounded caches/reasons, current regression, CI and completion record | Pending; unsupported optimization boundaries are normal, not render failures |

Only one owned compiler/gate runs at a time. Issue #72's completed results are
not attributed to this new source. Current full effects/render regression is
still required before a delivery claim.

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
producer case uses 0.008 for binary16 and 0.00004 for binary32. Observed ordinary
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

The integer guard retains the **whole draw**, including earlier candidate groups.
A later unproved consumer also prevents replacing an upstream partial group:
permitted local rounding errors cannot be assumed safe across an arbitrary
threshold or gain. Only a proved tail can currently fuse. Native eligible two-
effect draws use three passes rather than seven, including materialization; the
effect group itself uses two rather than six. This is not a whole-frame speedup.

The 32-row JSON is under the successful native receipt's `output-artifacts`.
UNORM8/UNORM16 each have four fused, two zero-opacity and two retained-alpha
variants; floating modes each have six fused and two zero-opacity variants.
Eligible numeric updates add no native or portable compilation. Entering the
retained path for the first time can create the original native pipelines that
the optimized path had not needed; the portable group counter does not change.
Reversing operation order changes the structure key and adds one portable group.

The first v5 microprobe did not request timestamp features, so it correctly
reported unavailable rather than zero. The explicit v6 setup requests supported
features and records actual native intervals. These include markers/scheduling;
65x33 probes are not representative performance benchmarks. No physical GPU
bandwidth counter or Android device measurement is claimed.

## Provenance

No new dependency, third-party asset or Adobe material is introduced. Operation
references are independently authored repository shaders. Existing compatibility
identifiers are only read from current packages; no new compatibility name is
introduced. This work does not change UI or add 3D nodes. Issue #78 remains a
separate Render DAG milestone after the fusion proof and acceptance.
