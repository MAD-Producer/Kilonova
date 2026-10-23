# Configurable decoded-image capacity (issue #61)

Current local Windows acceptance: all 15 gates pass on `03f629938635bfb7566520deacd5acf303a0880c`; see [cumulative acceptance](resource-budget-preview-completed.md)
and its source/hash/command manifest. This includes native failure/recovery,
full suites, actual GUI/MCP, default output channels and Release checking.
Remote PR checks and new Linux/macOS runtime results are separate. The user
confirmed maintained Windows/Linux/macOS desktop delivery on 2026-10-23.
Android is unmaintained, outside this delivery, and has no build/device
acceptance. Issue #61 is not closed by this local record.

Historical status before cumulative acceptance: focused actual guards D1-D9 pass on c8fb11e5, including full decode
beyond128MiB, native GPU recovery, host8/16-bit entry points and frozen PNG/movie.
The cumulative full-render run failed at an independent mask category-history
fixture before later suites executed. D10 remains pending; these focused results
do not accept a later product source or close issue #61.

## Scope and requirements

The Session MemoryPolicy version is 5. Its independent per-image CPU
`decodedImageBudgetBytes` allowance defaults to 128 MiB and accepts 4 bytes..2 GiB.
This replaces the hard result-capacity checks in the registered image decoder
and both host original-asset read APIs. It does not increase the source GPU
allowance, remove the encoded input/dimension/device/package checks or enroll
CPU allocations as native GPU capacity. No project format or packed plan ABI
change, panel edit, layout or interaction change is part of this increment.

Policy errors retain a typed `decodedImages` scope, required and budget bytes.
Malformed input and codec/IO/allocation failures retain native causes. A full
RGBA8 result reserves fallibly. Cache reads recheck the current result limit;
PNG can stream a small proxy, while JPEG requires full-source capacity too.
Cancellation never delivers an old-budget result or starts a second concurrent
decode. The existing submitted front and resident GPU textures remain valid.
Frozen outputs keep the decoded-image policy captured with their other category
allowances; their shared native admission limit remains live and independent.

## Source-bound evidence and remaining gates

| ID | Guard / evidence | State |
| --- | --- | --- |
| D1 | `image_decode_budget`: actual PNG pixels, direct-buffer refusal before mutation, cache recheck, streamed proxy under smaller allowance | Pass `03f62993` |
| D2 | `image_decode_budget`: native missing file/invalid metadata/wrong buffer identity; JPEG full-source refusal and recovery | Pass `03f62993` |
| D3 | `image_decode_budget`: actual asynchronous refusal, cancellation of old-budget results, single task slot and recovery | Pass `03f62993` |
| D4 | `image_decode_budget`: atomic policy validation, independent GPU allowance, omission selects safe CPU default | Pass `03f62993` |
| D5 | `configured_full_png_can_cross_the_previous_128_mib_result_limit`: generated 8192x4097 PNG, default refusal, actual full decode above 128 MiB with explicit allowance and all pixel checks | Pass `03f62993` |
| D6 | `image_decode_budget_native`: actual GPU upload/capture, typed CPU refusal without overwriting target, proxy then full recovery, real corrupt PNG remains codec fault | Pass `03f62993` |
| D7 | Host `cpu_asset_entry_points_follow_session_policy_without_changing_legacy_pixels_or_history`: both original-asset APIs, 8/16-bit PNG sources, low-cap refusal, configured recovery, sentinel tail/source/project/revision/undo preservation | Pass `03f62993` |
| D8 | `frozen_png_keeps_cpu_decode_allowance_and_preserves_publication_on_refusal` and `frozen_movie_keeps_cpu_decode_allowance_across_live_changes_and_session_close`: immutable CPU policy, previous publication preserved on refusal, original dimensions and successful output after live policy changes/closure | Pass `03f62993` |
| D9 | `info_start_precedes_real_decoder_access_and_does_not_require_debug`: controlled logger gate before real file decoding, default Info context, Debug completion suppressed | Pass `03f62993` |
| D10 | Workspace/all-target application-FFmpeg check, full render/host/media/application suites, frozen PNG/movie publication/pixel guards, style consumer guard | Pass `03f62993` |

Focused raw receipts: `ui-allocation-ui-v2-{decode,start,native,host-policy,png-default,movie}.exit.json`.
The decode log SHA-256 is `2ad74d348d4f8f472f3400c9a1434248d7b54d5fe8363893ec7843a9a6044b01`.
The earlier c9f65122 decode1pass5fail is retained: invalid generated asset paths
failed before decoding; test-only96022bdd fixes them without changing assertions.

Raw logs and source/hash/exit receipts belong in `artifacts/pr-queue` on E:.
No inherited success count or passing syntax check can substitute for these
actual guards on the committed source. The default decoder Info start precedes
native decoding; routine completion is Debug, never stdout. Existing actual
application file/Error-stderr/stdout-isolation tests remain required.

## Limits and remaining issue work

Per-image output/full-source allowance does not bound codec workspace, concurrent
decoded results, proxy rows, disposable disk cache, physical VRAM or process RAM.
Presenter/UI enrollment and integrated lifecycle/native results are recorded
separately in the cumulative acceptance manifest. Legacy standalone Renderer
eager asset replacement does not consume the Session CPU decode policy. Android
is unmaintained; no Android runtime or new Linux/Metal execution acceptance is
implied by common Rust signatures.
