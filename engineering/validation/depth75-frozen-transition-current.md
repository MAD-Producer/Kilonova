# Frozen aligned image depth export (#75)

## Scope and source

This increment follows Session transition PR #642, based on
`7d6e9cf9be9670e4ad3a4a210117f81fa9f07437`. It addresses the previous explicit
refusal to freeze an active, calibrated managed-image transition. It does not
complete #75 or #88. Android remains outside the maintained acceptance scope.

Capture retains the actual immutable interactive reference, currently admitted
candidate, prepared RGB fingerprint, original authored calibration, actual
provider/model descriptors, selected phase/generation and current versioned
policy/fit limits. The Session owner only clones metadata and ownership. It does
not decode, infer, hash scalar planes or fit. Pending, stale and revoked
transitions refuse capture; they cannot fall back to an older raw reference.
An active transition without an eligible calibrated 3D image still preserves
the existing explicit refusal.

## Independent reconstruction

The strict current scalar snapshot header is `MSDPTH4`. This is an internal
frozen-export format update, with no old-format migration or project schema
change. Previous validation documents referring to `MSDPTH3` describe their
historical source. All producers and readers use the same current Rust module.

Both reference and candidate allocations participate in the existing identity-
deduplicated 192 MiB resident budget. Serialized resource indices, reachability,
provider bounds, metadata size and total resident bytes are checked before plane
allocation. File-backed video plane handling and its separate 2 GiB bound are
retained. The resource count allows one additional reference per aligned static
layer, not arbitrary unused resources. Raw/temporal metadata retains its original
resource-count bound even before scalar payloads are read.

On the independent exporter, scalar metadata/raw float bits/mask/confidence are
hashed once per resource index. Alignment metadata binds both digests and the
layer's original source/calibration/request. A reader verifies these digests,
then fits and validates the actual reconstructed reference/candidate instances
using the frozen explicit limits. Persisted coefficients alone cannot construct
an instance proof. Digests detect corruption; they are not authentication against
an actor who rewrites payloads and recomputes digests. Current policy versions,
selected phase/key and original interactive reference identity remain required.
Within one bounded read, aliases of the same two immutable resource indices and
identical policy reuse the newly qualified proof instead of scanning the full
image once per layer. Content and per-layer metadata checks still precede reuse.

The composed calibration transforms decoded-value anchors into candidate units;
it preserves original metric camera observations, curve and extrapolation. The
saved authoring data continues to reject direct cross-model rebind. The renderer
independently rechecks source, grid, meaning and original metric observations.
Scene preparation remains atomic on a clone. Actual copied RGB must pass the
existing fingerprint verifier before attachment; no live Session or inference
worker is needed at export time.

## Observability and cancellation

Hashing and reconstruction fit publish Info start with request/object/source,
resource or generation and phase before entering the scan/fit, and flush the
existing logger. Routine completion stays Debug. Fit errors retain original
typed causes and Error context. Check callbacks run between bounded chunks and
inside fit; the original caller cancellation takes precedence over a generic
alignment cancellation. Healthy Present paths are untouched.

## Validation and retained failures

Eight new native controls cover:

- Roundtrip fit against new actual resource identities, shared ownership,
  unchanged original calibration and metric samples, deterministic repeated bytes
- Version/phase/generation/coefficient/RGB-metadata/provider/scalar corruption
- Combined reference/candidate resident-budget refusal with no plane payload read
- Cancellation inside actual fit and a foreign interactive reference rejection
- Actual Session selection, pending capture refusal, seek/revocation and no retry
- Default Info start visible before actual fit callback cancellation
- Actual backend failure refusing to freeze an older raw reference without retry
- Actual GPU equality with the raw calibrated oracle across an animated camera,
  occlusion, closed Session, removed original and changed/missing copied RGB

The first full host unit/depth_session run compiled and executed: 284 passed,
two failed, no ignored/filtered controls or abnormal target exits. Seven of the
eight new controls passed, including the closed-Session GPU roundtrip. Actual
controller session 84215 was consumed with exit 1. Its original logs, receipt
and all 13 exact executed source files are retained in
`artifacts/pr-queue/depth75-frozen-target-v1/`.

The failures found two product boundaries, without changing assertions:

- The original temporal metadata preflight rejected the newly enlarged global
  resource limit. The reader now permits additional references only for actual
  aligned entries, preserving the original raw/temporal bound
- The new seek control exposed fallback to the retained raw reference after
  live transition revocation. A metadata-only export-intent binding now survives
  frame-only revocation and refuses capture until an explicit raw request,
  cancellation or document-scope change clears it. Ordinary raw retained-image
  playback/export behavior is preserved; no retries or inference are added

Corrected qualification uses
`artifacts/pr-queue/depth75-frozen-target-v2-controller.py`, with the same full
targets and eight named controls. It requires the consumed failed run and its
verified source archive before starting. Only one own native/model/CV/GPU
workload may run, with the existing 10 GiB admission. Original assertions,
ignores, backend, timeouts and numerical gates remain unchanged. Parent or
pre-rebase passes are not acceptance for this increment. The corrected outcome
follows below; actual production-file evidence is pending.

The corrected controller was actually consumed as session 60024, exit 0: 286
passed (257 unit + 29 Session), zero failed/ignored/filtered, all eight named
controls, no abnormal exits, unchanged source and feature-off host check passed.
The 13 executed files and raw receipts/logs remain in
`artifacts/pr-queue/depth75-frozen-target-v2/`. These are pre-rebase results.

The branch was then aligned to freshly fetched main
`b190db953ef966d6b12d7ae98005c94ae615afa2`, including the newer renderer,
color/lens-output error handling and its native controls. The new actual output
control exercises both the PNG exporter and durable MP4 queue: an affine-scaled
settled candidate must preserve the independent raw metric occlusion oracle,
exact PNG pixels and the existing encoded per-channel mean-error limit of eight
over all 31 decoded frames. Both exports happen after the Session exits and the
original source directory is removed, with exactly three controlled inferences
and no inference during reconstruction/export. Actual session 48868 completed
with exit 0 and unchanged source: this new control and both existing PNG/MP4
controls passed. This deliberately scoped run filtered 66 unrelated unit tests;
it is not the full media suite. All 31 aligned MP4 frames passed the existing
error gate; aligned PNG pixels exactly matched the raw metric oracle. Final
aligned full regression/CI remain pending; synthetic backend checks are not
model-quality proof.

Focused receipts/controllers, original failed outputs/source, corrected executed
source and actual PNG/MP4 artifacts are retained in
`data/depth75-frozen-focused-v1.zip` (1,104,075 bytes; SHA-256
`a5a7618c17de3e604e2897067a0881f7db16414dab0a4f9225e924bc86ccfdc9`),
with all 75 members verified. This archive is scoped as above and does not claim
full regression or default production-file acceptance.

The exact merged parent PR #642 completed 1,471 host/render tests, all 153 source-
bound native/doc invocations, all 58 named controls and the desktop integration
check. Exact-head CI verified 56 host controls on each of Linux/Windows/macOS
Intel plus two renderer controls. Default production-file start/Error stderr/
stdout-isolation evidence and the actual Chinese merge commit are preserved in
`data/depth75-session-current-complete-v1.zip` (1,472,657 bytes; SHA-256
`125cac13756ae8fd4025c26b00304fabbc9be8f44ce1ce8abeeeb07c315f0360`),
with all 59 members verified. This parent evidence does not qualify the new
export source.

Complete changed-crate/desktop integration and exact-head cross-platform CI
remain required after focused qualification. No panels, layout, user interaction,
product dependency, third-party asset, model distribution or inference-quality
claim is included. Measured capability/quality selection, a versioned depth
cache, real continuous-video transition drift/jump and sustained/thermal
qualification remain open. Photographed tracking quality for #88 remains open.
