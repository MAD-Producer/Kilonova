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

Actual 266/392 exports and numerical reference runs are pending. No model hash,
quality ranking, speed, memory reduction or temporal agreement is claimed for
those candidates. Existing host ONNX and optional native WGSL admission still
accept only their explicitly implemented 518 processing/plan identity. Accepting
new candidates into the host, independently lowering/qualifying WGSL plans,
measuring all stages and sustained operation, and validating video quality,
generation checks, hysteresis and frozen export policy remain required.

The current full backend regression runs in the separate `reference-depth76`
worktree. Its result does not qualify these new exporter changes.
