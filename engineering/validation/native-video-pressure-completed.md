# Native video application-budget study: completed measurement

Issue #60, maintained desktop scope. Android acceptance is excluded by the user.

## Exact sources and evidence

The actual native measurement uses `53f2b24ef3bfc8c97811a3cd6748f8e0949cbfa0`. The repository Release build exited 0 with its normal optimization profile; the exact ignored native study then actually ran: **1 passed, 0 failed, 0 ignored**. No later delivery-source performance rerun is claimed.

[Original evidence](data/native-video-pressure-53f2b24e.zip) is 2668559 bytes with SHA-256 `028f7f502e203b2391422a4de2267fb88680235024666760bdecf345ef8e6aa3`. Its 90 original evidence entries plus inventory retain process streams, source bundle/patch, compiled-workspace provenance, original profiler bytes, screenshots, source/fixture identities and the earlier failed execution. Every included entry was CRC/SHA-256 checked. Generated videos and executables stay local with hashes recorded. The primary validator was rerun without rebuilding and matched the original acceptance exactly.

Adapter: **NVIDIA GeForce RTX 4070 Laptop GPU**, backend **Vulkan**, driver **NVIDIA 591.86**. Four independently encoded 3840 x 2160 sources, one 1280 x 800 native window and a 1280 x 720 composition use the same session/renderer/device. Environmental desktop load, thermals and power are uncontrolled.

## Actual budget transition and recovery

| Phase | Completed source frames | Source texture allowance | Final quality | Associated root GPU samples |
| --- | ---: | ---: | --- | ---: |
| `normal` | 3 | 256 MiB | level 0 | 3 / 3 |
| `limited` | 3 | 32 MiB | level 1 | 3 / 3 |
| `recovery-1` | 121 | 256 MiB | level 0 | 121 / 121 |

All **127** completed frames retained their exact source identities and associated root GPU samples. Lowering the source allowance from 256 to 32 MiB produced an actual refusal and level-1 output at 640 x 360. Restoring 256 MiB retained the level until 120 real successful warm frames and the production cooldown made a probe eligible. The recorded remaining cooldown was 21999 ms; native message pumping waited 22034330 us, then one actual probe recovered level 0 and 1280 x 720 output. No clock, recovery threshold, return value, retry or backend was changed.

Before rendering after each allowance change, the retained front was read through the real renderer and compared byte-for-byte with its previous full image: 3,686,400 bytes for the full front and 921,600 bytes for the limited front. These oracle readbacks occur outside profiling. All four independently decoded reference centers matched screenshots in every phase; all recovered native decoders returned full 3840 x 2160 frames. Project and history state stayed unchanged.

## Failure retained and correction verified

The original `bc70785a` Release build passed, but its native study failed after source frame zero. Its raw worker seek did not advance `Shell.frame`; accepting an edit snapshot intentionally does not change that field. Source53 calls the ordinary Seek action instead. The unchanged identity assertion and a real shell/worker seek regression both pass. The earlier native failure remains failed and its original executable, primary cause and logs are retained. Cached library reuse at source53 is justified by the actual previously fresh ten-workspace-artifact build and an exact two-file driver/documentation difference; the corrected application executable was actually rebuilt.

## Measurement limits

This is actual application source-texture allowance pressure, not physical VRAM exhaustion, driver residency or operating-system memory pressure. Timings describe explicitly stepped source frames, not playback or display FPS. Percentiles and cold/limited/recovery samples are preserved with their actual counts; no optimization gain or controlled before/after comparison is claimed.

Root GPU timestamps exclude some earlier video conversion, nested rendering, vector/MSAA preparation and the later native overlay. Unknown/decreased/first counter intervals remain unknown; overlapping conversion scopes must not be added. The old `previewImageReadbackBytes: 0` metadata is an uninstrumented placeholder, not evidence of zero image readback. The manual front reads above are real nonzero image reads; complete readback observations are a separate increment. The archived original metadata is not rewritten.

The separate earlier full-render-suite non-unwinding abort remains unresolved; later passing suites do not establish its cause or a fix. This measurement does not close all of issue #60.

## Delivery validation

The combined desktop source is `7e694cce803afd4b30951b1ea083249b09450005`, based on `62bf0f28cb339d67039d75abfe51c83127c9b92c`. Actual default-feature `cargo check -p motion-desktop` passed. The exact Seek regression passed, and the complete `motion-desktop --features ffmpeg` suite passed: **855 passed, 0 failed, 3 ignored**. Required new tests actually executed; an ignored study is not used as regression acceptance.

The actual render/host acceptance source is `6230bdb9dcdfdab9f5dd62abf2b1adac2e8ed520`: complete `motion-render` and `motion-host` suites passed **676 passed, 0 failed, 1 ignored**. All six new readback/callback cases and four existing sequence regressions were verified in actual successful raw output. All renderer/host and dependency inputs are identical at the combined source, proven by an exhaustive whole-tree diff; this is explicitly attributed reuse, not a combined-source rerun. The ffmpeg desktop check also passed at the readback source.

[Source-bound correctness evidence](data/native-cost-delivery-7e694cce.zip) is 165226 bytes with SHA-256 `3c6b347bf161cd46d41b2ff7dee9b3291b0c44cd6ee6653b04e72b741c4ad3f2`. It contains original raw gate logs, exact test-execution proofs, exhaustive input equivalence, source bundles/patches and a SHA inventory. The original native pressure measurement and archive remain at source53; no delivery performance rerun is claimed. CI on the final documentation head is a separate gate required before merge.

The three ignored desktop entries are the native audio-output-device check, the explicit Release native-window cost study and the new explicit Release application-budget study. The budget study was actually executed separately at source53 (1 passed, 0 failed, 0 ignored); the native-window study belongs to the separately published source-df measurement. This correctness run does not claim native audio-output-device acceptance. The sole ignored render/host entry requires an external `MOTION_IMAGE_SOURCE` fixture and is not used as acceptance for the new readback paths.

## Requirement dispositions for this increment

The identifiers below follow the four implementation requirements (R1–R4) and three acceptance requirements (A1–A3) in issue #60. A disposition does not establish that the whole requirement is finished.

| Item | Disposition | Evidence and remaining limits |
| --- | --- | --- |
| R1: category/ownership ledger | Existing coverage retained | This study consumes the existing ledgers; it does not add allocation categories or prove complete ownership coverage |
| R2: logical/CPU/external distinction and aliases | Existing boundaries retained | The source allowance, decoded sources and retained fronts are distinguished; physical driver residency and unknown decoder allocations are not measured |
| R3: per-frame cost observations | New pressure-phase evidence; root GPU coverage retained | 127 completed source frames and 127 associated root samples; readback accounting is the separate new increment; full GPU-stage intervals and exclusive frame readback costs remain incomplete |
| R4: reproducible scenarios | New native four-video application-budget transition | Normal → actual refusal/limited quality → production warm/cooldown recovery in one session/device; other scene families are not reaccepted by this study, and physical/OS memory exhaustion is not claimed |
| A1: fixed conditions/source and raw records | Accepted for the measured source | Source53, fixed composition/window/video identities, exact runtime/fixture hashes and original 90-entry archive; delivery correctness is attributed separately |
| A2: GPU/driver/API and platform scope | Accepted for this Windows/Vulkan measurement | RTX 4070 Laptop GPU, NVIDIA 591.86; other desktop timing measurements are not claimed here; Android validation is explicitly excluded by the user |
| A3: unmeasured improvement claims remain hypotheses | Preserved | No percentage saving, acceleration factor, comparative FPS or controlled optimization gain is inferred |
