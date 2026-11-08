# Qualified depth execution bindings (#75)

## Scope

The existing capability decision now has an explicit application route through
`Jobs::start_qualified` and `Session::configure_qualified_depth_in`. Ordinary,
measured and persistent-cache constructors retain their existing behavior.
This is opt-in host plumbing, not automatic tier selection or a device
certificate. Android and panel code are outside this change.

`policy::Bindings` retains one to eight complete admitted decisions. The caller
must independently verify the environment, protocol, source domain and external
reports. Digests identify records; they do not authenticate reports, measure
hardware or classify incoming footage. Unknown qualification or a
`PrecomputedRequired` decision cannot construct an inference binding. Routes
must have distinct exact model/geometry identities. Requested limits, actual
resolved choices, external records and every refusal remain readable through
the worker and Session getters after the profile owners are released.

## Execution boundary

After source preparation and RGB fingerprinting, the serial worker matches
the exact model and geometry to an admitted route. Before persistent-cache or
Session-spill reuse, it publishes `qualified_resolve` in-flight state and an
Info start, then resolves the actual backend identity. Backend, provider,
runtime SHA-256, plan SHA-256 and precision must all match the retained choice.
Unknown, invalid or changed identities fail without inference, cache lookup,
another backend, retries or result publication. Error context reports the
first changed field and both values within the existing bounded cause.

Matching inference results also pass the existing resource/source validation
and provider attestation. Cached results do not become new capability samples.
The existing transition, calibration and frozen-export ownership rules remain
in force; a failed settled request cannot freeze an older raw reference as a
successful settled result.

## Verification and limits

Nine worker controls cover context and route bounds, retained reports,
controlled blocking before identity resolution, all five identity fields,
missing/invalid/unbound identities, cached reuse, cancellation, original
provider/resource/backend/panic failures and a two-model router. Two Session
controls cover interactive/settled transition, metric calibration and frozen
round-trip, plus changed-runtime refusal without backend entry or a retry.
External quality, memory and sustained reports in these tests are explicitly
controlled fixtures. They do not qualify an actual learned model or GPU.

The original native command at `6d3798d9` ran all 175 host/render/desktop-media
targets with the existing GPU, AI-depth and FFmpeg features: 1,651 passed,
one failed, three original ignores and zero filtered. All 37 capability tests
and the positive Session transition/frozen control passed. The new negative
Session fixture provided only its first Idle tick at 400 ms, so it unwrapped
None before submitting a settled request or reaching the runtime guard. The
original command remains failed; the product-build stage did not run.

The complete failed command, source identities and independent target binding
are retained in `data/depth75-qualified-runtime-binding-failure-v1.json` and
its SHA-256 verified archive. The fixture now receives an initial Idle tick at
100 ms, matching the positive control. Every existing assertion, count, policy
limit and production source remains unchanged. A new child-process control
reuses the actual desktop sink to verify default Info file output before a
blocked resolver, Error file/stderr output and harness-only stdout. The two
complete capability/Session targets and skipped product build are pending on
this supplementary source; no earlier failed command is relabeled as green.

Passing these tests alone will not close #75: independent domain quality, conservative complete
memory, longer thermal behavior, actual continuous-video transitions and
current cross-platform CI remain required. The finite sensor run and actual
CPU/WGSL video measurements remain separately scoped evidence. #88's retained
tracking failures are unrelated and remain unresolved.
