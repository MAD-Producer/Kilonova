# Tracking #88: independent point displacement

This increment continues the 3D route on `codex/88-track-import-contract` after
`dc4bdd8c`. It does not close #88 or qualify the entire 3D route. Android is
outside the maintained acceptance scope. No panels, layout, styles, interaction,
third-party dependencies, or third-party assets were added.

## Implemented runtime boundary

- Canonical source-clock, point/plane/camera document, pixel geometry, pose
  conversion and strict JSON modules now live in `motion-model::tracking`.
  Host reexports preserve the existing Rust import paths. The existing
  `serde_json/float_roundtrip` feature moved with its owner; no new crate is used.
- Required immutable decoded data accompanies each original attachment. Project
  format is **33 only**; there is no migration or older-document fallback.
  Project/history/frozen-frame clones share an Arc allocation. Byte-exact source
  JSON remains separately managed. Storage checks both its complete hash and
  decoded equivalence before load, save or package publication.
- `set_tracking_point` is an atomic, undoable core command, available through
  the existing MCP command routes. Explicit null detaches; omitted binding is
  rejected. Locked, absent and unsupported targets are rejected.
- A binding selects the managed attachment and point track, source PTS anchor,
  local composition-frame anchor, signed rational speed, source reference pixels,
  crop, rotation, interpolation, maximum source-tick gap, confidence and missing
  policy. All choices are saved; no ordinary position keys are generated.
- Sampled displacement is the transformed current point minus the transformed
  reference point, added to the ordinary authored position in the target's local
  pixel axes. Parent rotation/scale applies afterward. Existing keys and position
  expressions therefore remain manual corrections. Cross-property expression
  references include the independent displacement exactly once.
- Fractional renderer frames retain their exact binary rational time in checked
  i128 arithmetic. Source nominal fps never substitutes for original PTS.
  Reverse and zero-speed clocks are explicit. Linear interpolation needs both
  adjacent observations visible, confident and within the declared PTS gap.
  Hold uses the preceding observation only within that interval. There is no
  extrapolation or automatic repair of absent/lost/occluded observations.
- `error` reports unusable samples with object, attachment, track and frame.
  `authored_only` deliberately uses the correction track alone for those gaps;
  this user-selected disposition is not an implicit retry or a logged fault.
  Structural/clock/numeric failures still fail. Binding metadata faults include
  the binding phase. Worker replies retain request, operation and elapsed context.
- Healthy point lookup performs no filesystem IO, full-observation validation,
  keyframe baking or logging. Raw observations remain immutable and bounded.

## Evidence and qualification

Evidence directories are under `artifacts/pr-queue/tracking88-point-*`.
The initial standalone model run failed the unchanged binary64 roundtrip tests:
63 passed, 2 failed. The cause was the feature ownership error described above;
fixing the model dependency feature restored the original bit-level assertions.
The initial host run found a source-identity validation boundary mismatch:
48 passed, 1 failed. Source identity is now enforced at binding/resource admission,
while shape-only validation retains the prior unbound-metadata contract. The
original host assertions and all six moved test files remain unchanged apart
from host formatting.

The v3 model output contains **224 passed, 0 failed, 0 ignored**. Its core output
contains **479 passed, 0 failed, 1 ignored**, including completed doc-test output.
The ignored test is the pre-existing explicitly manual large-document history
measurement; no ignore was added. Both step completions were observed live.
The v3 controller later disappeared and its JSON receipt and part of the host
logs contain NUL bytes. These damaged files are retained and are not treated as
completed host or desktop gates. v4 independently records those affected gates.

Final native acceptance and durable archive identities are recorded in the
closeout receipt accompanying this document. Missing or failed entries remain
unverified; a successful increment never replaces the older failed gates.

## Remaining route requirements

- Runtime plane and externally solved camera bindings, calibrated lens/projection
  application, unit/basis/origin conversion at those consumers, and independent
  manual pose corrections beyond point displacement
- Known-camera rendered reprojection and calibrated real-footage replacement
  controls; point-driven object motion alone does not prove camera solving
- Relink identity/lease behavior, cross-document bound-layer clipboard resource
  remapping and full exposure/nested/frozen/cache qualification
- Large-document serialization and runtime throughput qualification; shared
  allocations do not eliminate the existing project-JSON size budget
- Current complete desktop/full-host regression and cross-platform CI

The earlier `tracking88-decoded-v4` full-host failures (212 passed, two native
child crashes) and `tracking88-decoded-native-v1` rustc access violation remain
unresolved. Their original EXE/PDB/WER and public fault summary are retained.
This increment does not rerun those unchanged failed targets to seek a green
result. #82's original strict P95 failures and incomplete enabled GPU coverage
also remain unresolved; no performance assertion or sample schedule was changed.

## Final increment acceptance

- v4 host tracking: **49 passed, 0 failed, 0 ignored**; desktop ffmpeg check: exit 0
- Actual ffmpeg product build: exit 0; bootstrap, editable MCP and read-only reopen:
  all exit 0, JSON-RPC stdout only, matching response IDs, no trailing output
- Original 72-frame repo-authored B/VFR video decoded and admitted with exact PTS;
  independently authored point observations drive an independently authored OBJ cube
  through an ordinary null parent. This proves the runtime route, not a tracker.
- Frame 0 null anchor: (220, 180); frame 30: (304.5070496, 236.3380432).
  Analytical expectation: (304.5070518, 236.3380345); tolerance: 0.003 pixels.
  Both frames were actually rendered at 640 x 480 and visually inspected.
- Invalid track binding leaves project/revision unchanged. A strict interpolation
  gap fails at fractional frame 0.5, then recovers after restoring its policy.
  Both failures retain causes in the default Info file log and Error stderr.
- External solve JSON deletion does not prevent undo, redo, save or read-only
  reopen. Manual correction metadata and the managed byte-exact original survive.
- Style-consumer guard and whitespace pass. No panels files changed.

The durable archive is [tracking88-point-binding-v1.zip](data/tracking88-point-binding-v1.zip).
Its [closeout receipt](data/tracking88-point-binding-closeout.json) records exact
code/archive hashes, completed gates and damaged evidence. Raw damaged v3 files
are preserved; their existence is not an exit-code claim. Current code identities
were rechecked against native acceptance before archival.
