# Physical aligned Session placement and independent export (#75)

Status: functional physical-unit integration with analytic controls. This is not
learned-model or complete 3D-roadmap qualification. Maintained scope is desktop.

## Change and invariants

A current worker admission now allows a saved physical placement to bind its
original immutable reference while rendering the qualified candidate. The
reference/candidate identities, actual prepared RGB, source time and model
processing identities remain distinct. The renderer consumes original candidate
samples in their declared units exactly once. It does not consume the view's
reference-unit mapped samples or fabricate relative calibration anchors.

The existing relative path retains its mapped anchors and all prior assertions.
The physical path requires matching camera-Z representation, source and grid,
valid saved-reference identity and image dimensions. It preserves the mesh quota,
immutable instance cache key and original paired color ownership. Renderer entry
assumes the inference owner has already qualified the actual candidate; metadata
checks do not establish learned-model accuracy.

Session capture remains metadata-only and cannot scan planes or invoke inference.
Frozen restore checks resource/metadata integrity and reconstructs the actual
instance-bound unit proof before admitting placement. Serialized coefficients do
not replace this proof, even with a matching metadata integrity checksum. Saved
reference metadata is not rebound directly to a different candidate model.

Physical restore publishes default-Info start with request/object/source/generation
and `phase=verify_physical_units` before comparison/cancellation polling, then
flushes the existing logger. Relative restore keeps `phase=fit_reconstructed_planes`.
Primary cancellation remains intact. No new Present-path or per-pixel logging.

## Verification and preserved failures

One owned serial controller at a time; source and HEAD remain frozen through
original terminal consumption. Locked/offline native commands use the existing
E-drive cache, unchanged compiler/GPU/options, and original assertions/thresholds/
ignores. No model/runtime/dependency/dataset acquisition, panel, layout or style
change is involved. Original records remain in
`artifacts/pr-queue/depth75-physical-transition-controls-v*/`.

The first controller passed both complete render targets, then its new public
coefficient-corruption fixture failed the intended proof-gate assertion: it had
retained an old metadata digest and hit the earlier integrity guard. The fixture
was corrected to independently reproduce the original typed seal first, change
coefficients and reseal metadata. The assertion and production source remain
unchanged by that fixture correction. The original failure remains recorded.

Four new controls exercise renderer identity/meaning refusal, actual worker and
Session metric/inverse-metric profile transitions, physical-bias rejection without
raw fallback, actual GPU occlusion, three animated camera times, live/frozen/
roundtrip pixel equality after Session closure, changed source pixels, source
fingerprints, current request counts and default-level start before cancellation.
These are authored analytic fields, not external learned-model outputs.

## Remaining acceptance

The prior complete-model native Windows mapping error 1455 command is still failed;
separate component passes do not erase it. Original native shader/full-workspace
failures, learned metric protocol/domain accuracy, cross-resolution quality and
thermal qualification remain open. Physical video authoring and explicit ray-
distance conversion are not supplied by the image-only placement command.
Automatic production quality tiers and default model distribution remain unqualified.

## Source-bound terminal results

Base `7a68d87d125bff903bd58db01234288b20351595` plus tracked diff SHA-256
`ddf19819f5e04a11cbdc128d4ccf87a75b43cfca8de66e1ce172f7d94bf8f020` and the
recorded untracked test-file hashes remained unchanged through original v2
session 33779 exit 0 / terminal eb8c91 consumption. The v1 product-file manifest,
renderer patch blocks and new renderer test-file hash were independently checked
unchanged after the fixture-only correction.

- v1 complete `motion-render::depth_surface`: 52 passed; complete `scene_gpu`:
  52 passed; zero failed/ignored/filtered
- v1 complete `motion-host::depth_session`: 35 passed and the one original new
  fixture assertion failure; original session 40228 exit 1 / terminal f2e994
  consumed. The video target and complete library did not run in v1
- v2 complete `depth_session`: 36 passed; complete `depth_video`: 26 passed;
  zero failed/ignored/filtered
- v2 complete host library: 295 passed, zero failed, one original ignore,
  zero filtered

The five selected complete targets establish 461 distinct passes for the
unchanged relevant product source, with the original ignore retained. The
negative coefficient assertion was preserved. Matching-checksum wrong coefficients
are rejected by the reconstructed unit proof, rather than trusted from metadata.
The original uninstrumented native shader failure did not recur; its cause
remains unresolved. Earlier-source published CI cannot cover this increment.

A subsequent acceptance audit strengthened the same biased-profile test through
`ops::export::capture`, verifying actual public export refusal and absence of a
flat/old-reference output for both physical measures. Every earlier assertion
remains; production source is unchanged. Full `depth_session` passes all 36 tests
with zero ignores/filtering. Base `7f2443a099faf4d29dc0a05fa547a69c27cc88a5` plus
test-only diff SHA-256 `1169c131c6050a077664bf87ba7e0501459a790f6abda9d4de0cbdd8e551d25e`
was frozen through original session 70818 exit 0 / terminal c9d41e consumption.
Private receipt: `depth75-physical-transition-export-refusal-v1/`.
This does not add duplicate passes to the 461 distinct-target count above.
