# Maintained desktop depth resource and consumer boundary (#75)

This records the current explicit research route and its admission boundary.
It does not add a generic model-package installer, SDK compute capability,
qualified model tier, automatic application configuration or default model
distribution. Old runtime compatibility and retired Android are outside the
user-approved delivery scope.

## Separate research resources

The optional desktop `motion-depth-wgsl` executable consumes an explicitly
installed, fixed-shape research program and separately installed Float32 weight
files. The host receives absolute Python, native-worker, program and scratch
paths. The program is pinned to SHA-256
`2ec1d8012905198cfd5af1430f88d1770edbf752531fe817dfcc65e1477fef6a`.
No normal effect package can silently install that program or its weights.

The blueprint requires version1, the explicit
`static_program_only_not_executable` research capability, the pinned model and
original-weight identities, RGB Float32 `[1,3,518,518]`, and one
`relative_inverse_depth` output. That capability label deliberately does not
advertise a generic product execution capability: only the separately selected
research worker compiles this exact blueprint.

The worker checks tensor dimensions, dtypes and exact byte counts; bounded
operation/resource/alias/slot counts; original ordering and allocation contracts;
checked capacity sums and the declared total; and actual device storage,
alignment and buffer limits. Weight paths must be `weights/<sha256>.f32` and
match declared shapes and byte lengths. Canonical paths must stay inside the
resource root; bytes must match their hashes and contain finite Float32 values.
The graph is not inferred from arbitrary package text or an unknown operator.

Requested Vulkan, DX12 or Metal is explicit. Research admission refuses an
implicit backend or software adapter and then validates the compiled graph
against the requested device limits. This is backend admission, not proof of
accuracy, speed, power or a production quality tier on that backend.

Retained commands separately require schema1 and exactly the next sequence.
Each request names fresh owned request/output directories, a pinned program
and original execution attestation. Cancellation or service failure closes the
backend; the backend does not silently recreate a service or switch execution.

Source anchors: `desktop/app/src/depth_worker/graph_admission.rs`,
`main.rs::tensor_bytes`, `graph_runtime_impl.rs::run`,
`service_admission.rs`, and `crates/motion-host/src/depth/wgsl/{identity,service}.rs`.

## Effect packages and frozen non-inference consumers

Ordinary `.msfx` files continue to use their existing image-effect contract.
`motion-effects::package::resource_limit` accepts manifests, WGSL shaders,
declared PNG assets and supported editor resources, within existing package
bounds. It does not accept ONNX files, Float32 weight resources or an inference
program. The image shader validator rejects `@compute`. The current release
packager copies the normal editor binary, declared FFmpeg runtime, licenses and
application resources; it does not install a research model or advertise an
AI-depth capability in its build-info capability record.

The host's explicit capability policy keeps requested tier/domain/budgets and
actual model/geometry/backend/provider/runtime/plan/precision separate. Missing
or incompatible quality, memory, sustained or execution evidence retains a
refusal; the disposition is `PrecomputedRequired`, not inference success.
An audit-record digest alone does not authenticate a report. A caller must
validate the real records before supplying a qualification.

An explicitly supplied, source-bound frozen `DepthResource` remains the
non-inference route. The actual cache/Session observation closed the Session
and inference worker before restoring and rendering its frozen result at three
times, and refused a changed source RGB fingerprint. This demonstrates that
consumer boundary; it does not qualify the learned model or prove generic
effect-package execution.

Source anchors: `crates/motion-effects/src/{package,shader}.rs`,
`tools/package_desktop.py::package`,
`crates/motion-host/src/depth/capability/policy.rs`,
`crates/motion-host/src/depth/storage.rs`, and
`engineering/validation/depth75-host-remap-integration-current.md`.

## Existing controls and remaining product work

The original full workspace at `dc17b007` contains the complete seven graph
admission tests and two retained-command admission tests, all passed with no
filtering or ignores. Their raw output and source binding are in
`depth75-host-remap-full-regression-current.md`. Those narrow successes do not
replace that command's four failed targets. The exact-head CI is recorded
separately; no external model is installed by those CI controls.

This establishes the current versioned research-resource contract and explicit
non-inference consumer boundary. A future distributed inference package still
needs an authored installer/manifest contract, provenance, complete resource
accounting and separately qualified devices/domains before default distribution.
No generic model distribution or automatic model-selection claim is made here.
