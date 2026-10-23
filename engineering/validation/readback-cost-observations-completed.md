# Readback cost observations: completed increment

Issue #60, maintained desktop scope. This increment accounts for actual readback work; it does not close all requirements of the issue.

## Observed operations

Per-renderer image observations begin before health checks and GPU work. They distinguish in-flight operations that have not returned, submitted image payload, padded buffer address span, actual mapped buffer length, returned image bytes, completed and failed calls. A real 3 x 2 image read twice produces 48 payload bytes, 536 addressed-span bytes, 1,024 mapped bytes and 48 returned bytes. Real device destruction preserves the primary error and records failure; a new renderer starts a new counter lifetime.

Per-timer timestamp observations distinguish encoded query resolves/copies, mapping requests, actual callback success/error, mapped bytes and valid/invalid samples, including work whose native sequence has no recorder entry. Cancellation is exercised through a real `unmap` and failed mapping callback, followed by successful remapping. Encoded but discarded commands are distinct from submitted mappings; unresolved zero words are invalid samples. Actual composition and presentation passes produce valid native timing samples. Collecting twice never doubles a sample.

The API publishes bounded saturating counters and a sticky loss-of-exactness flag. Missing observations remain null rather than invented zero. Image counter lifetime is the renderer lifetime; timestamp counter lifetime is the timer lifetime, and restarting host profiling replaces the timer. These are cumulative, individually atomic observations, not exclusive frame traffic, physical bus transfers or driver residency. Actual captures performed outside a recorded interval are not relabeled as frame costs. The old hardcoded `previewImageReadbackBytes: 0` placeholder has been removed.

See [API and lifetime definitions](../host-api/readback-cost-observations.md). Existing assertions, return values, retries, backend policies and UI behavior were preserved.

## Actual validation

The combined desktop source is `7e694cce803afd4b30951b1ea083249b09450005`, based on `62bf0f28cb339d67039d75abfe51c83127c9b92c`. Actual default-feature `cargo check -p motion-desktop` passed. The exact Seek regression passed, and the complete `motion-desktop --features ffmpeg` suite passed: **855 passed, 0 failed, 3 ignored**. Required new tests actually executed; an ignored study is not used as regression acceptance.

The actual render/host acceptance source is `6230bdb9dcdfdab9f5dd62abf2b1adac2e8ed520`: complete `motion-render` and `motion-host` suites passed **676 passed, 0 failed, 1 ignored**. All six new readback/callback cases and four existing sequence regressions were verified in actual successful raw output. All renderer/host and dependency inputs are identical at the combined source, proven by an exhaustive whole-tree diff; this is explicitly attributed reuse, not a combined-source rerun. The ffmpeg desktop check also passed at the readback source.

[Source-bound correctness evidence](data/native-cost-delivery-7e694cce.zip) is 165226 bytes with SHA-256 `3c6b347bf161cd46d41b2ff7dee9b3291b0c44cd6ee6653b04e72b741c4ad3f2`. It contains original raw gate logs, exact test-execution proofs, exhaustive input equivalence, source bundles/patches and a SHA inventory. The original native pressure measurement and archive remain at source53; no delivery performance rerun is claimed. CI on the final documentation head is a separate gate required before merge.

The three ignored desktop entries are the native audio-output-device check, the explicit Release native-window cost study and the new explicit Release application-budget study. The budget study was actually executed separately at source53 (1 passed, 0 failed, 0 ignored); the native-window study belongs to the separately published source-df measurement. This correctness run does not claim native audio-output-device acceptance. The sole ignored render/host entry requires an external `MOTION_IMAGE_SOURCE` fixture and is not used as acceptance for the new readback paths.


## Remaining boundaries

No physical VRAM exhaustion, complete GPU stage coverage, Android device acceptance, new playback FPS baseline or optimization gain is claimed. The separate previously retained non-unwinding full-suite abort remains unresolved; passing later suites does not prove its cause or a fix. Application-budget recovery measurements are documented independently in [the pressure completion record](native-video-pressure-completed.md).

## Requirement dispositions for this increment

The R1–R4 and A1–A3 identifiers follow the original issue's requirement order. A documented boundary is not a fixed defect or whole-issue completion.

| Item | Disposition | Evidence and remaining limits |
| --- | --- | --- |
| R1: category/ownership ledger | Existing coverage retained | Readback operations are observed separately from the existing buffer-capacity ledger; no new inference/driver allocation coverage is claimed |
| R2: logical/CPU/external distinction and aliases | Existing boundaries retained | Payload, addressed span, mapped length and returned bytes stay separate; none are relabeled as physical bus traffic or driver residency |
| R3: per-frame cost observations | New actual image and timestamp readback counters | Six new real/counter cases plus four existing sequence regressions; cumulative renderer/timer snapshots, saturation and null availability are explicit; exclusive frame attribution and complete GPU-stage time remain incomplete |
| R4: reproducible scenarios | Actual regression fixtures added | Real 3 x 2 image reads, destroyed-device failure, cancelled mapping callbacks and actual composition/presentation queries; broad performance scene families are not reaccepted by these correctness fixtures |
| A1: fixed source and original evidence | Accepted for this increment's actual sources | Source623 render/host and source7e desktop gates are separately archived with source bundles and exhaustive input equivalence |
| A2: GPU/driver/API and platform scope | Native correctness evidence; no broad timing claim | Real local GPU APIs are exercised; the pressure archive separately records its exact NVIDIA/Vulkan measurement; Android validation is explicitly excluded |
| A3: unmeasured improvement claims remain hypotheses | Preserved | New counters establish observability rather than an optimization gain; no FPS or percentage improvement is inferred |
