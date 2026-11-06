# Depth resolution export candidates

Issue #75 remains open. `tools/export_ai_depth.py --side 266|392|518` requests
an explicit square ONNX candidate from the already attributed, externally
installed Small weights. The default remains 518. A resolution is not a
fast/standard/high quality label or a claim about runtime performance.

The independently authored export procedure verifies the pinned source revision,
unchanged source checkout and weight length/hash before importing the model.
It then checks the reference tensor/output, exported input/output names,
Float32 types, exact shapes, CPU-only provider and actual finite ONNX result.
Publication requires the original normalized maximum 2e-4 and mean 2e-5 bounds
against the actual reference execution at the requested resolution. Each output
has its own filename, processing identity, shapes and SHA-256 in `model.json`.
Existing destinations are refused. No download, dependency or asset is added.

## Current evidence and limits

Five standard-library admission tests pass, including real invalid CLI calls
with and without Python optimization. The complete tool suite reports 115
passed and one original skip (116 total). Its first run refused an unclassified
new test file; registering that file in the tool inventory resolved the check,
with no assertion changes. They cover distinct processing identities,
invalid resolution rejection before side effects or heavy imports, destination
preservation and CLI defaults/choices. They do not invoke learned inference.

## ONNX host source wiring (not yet native-qualified)

The follow-up source change extends the ONNX host and its embedded Python worker
to the same three explicit processing identities. The host sends and validates
the requested identity rather than replacing it with 518. The worker checks the
exact corresponding input/output shapes before inference, retains the declared
identity in its report and performs letterbox/remap at that resolution. The
output resource still uses the source crop dimensions. Model hash checks,
provider reporting, cancellation and ownership checks remain in place. Unknown
identities are refused before runtime imports; no provider selects a resolution.
The follow-up report validator also requires RGB Float32 input-copy bytes and
scalar Float32 output-copy bytes to match the declared model side. Source-sized
resource bytes remain a separate check. A mismatched profile cannot contribute
another resolution's transfer statistics to later measurements.

Nine focused Python admission tests pass, including real worker refusals with
normal and optimized Python, and exact landscape/portrait/single-pixel crop
rectangles with odd padding. The complete updated tool suite passes 119 tests
with one original skip (120 total). Raw outputs and source hashes are retained
in `artifacts/pr-queue/depth75-resolution-python-v2/receipt.json`. The new Rust
processing-admission test, report identity/byte-count test and changed host
implementation have only passed rustfmt parsing/checks so far; compilation and
actual inference through this host remain pending. The full backend regression
already running in another worktree does not cover this follow-up source.
The report tests use typed protocol controls, not real inference output. They
exercise each declared side, profile substitution, hash/schema/dimension
mismatches, source-resource size and both tensor-copy sizes. Their runtime
execution remains pending; the earlier Python receipt predates this additional
Rust validator and does not qualify its changed source.

Actual 266/392 exports and numerical reference runs are pending. No model hash,
quality ranking, speed, memory reduction or temporal agreement is claimed for
those candidates. Optional native WGSL admission still accepts only its
explicitly implemented 518 processing/plan identity. Qualifying the new ONNX
host paths, independently lowering/qualifying additional WGSL plans,
measuring all stages and sustained operation, and validating video quality,
generation checks, hysteresis and frozen export policy remain required.

The current full backend regression runs in the separate `reference-depth76`
worktree. Its result does not qualify these new exporter changes.
