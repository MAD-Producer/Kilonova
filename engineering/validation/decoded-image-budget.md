# Configurable decoded-image capacity (issue #61)

Status: implementation candidate; actual compilation and runtime acceptance are
pending. Existing frame/export/pressure results do not validate this increment.
This document is not an issue completion claim.

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

## Required evidence (not yet executed)

| ID | Guard / evidence | State |
| --- | --- | --- |
| D1 | `image_decode_budget`: actual PNG pixels, direct-buffer refusal before mutation, cache recheck, streamed proxy under smaller allowance | Pending |
| D2 | `image_decode_budget`: native missing file/invalid metadata/wrong buffer identity; JPEG full-source refusal and recovery | Pending |
| D3 | `image_decode_budget`: actual asynchronous refusal, cancellation of old-budget results, single task slot and recovery | Pending |
| D4 | `image_decode_budget`: atomic policy validation, independent GPU allowance, omission selects safe CPU default | Pending |
| D5 | `configured_full_png_can_cross_the_previous_128_mib_result_limit`: generated 8192x4097 PNG, default refusal, actual full decode above 128 MiB with explicit allowance and all pixel checks | Pending |
| D6 | `image_decode_budget_native`: actual GPU upload/capture, typed CPU refusal without overwriting target, proxy then full recovery, real corrupt PNG remains codec fault | Pending |
| D7 | Host `cpu_asset_entry_points_follow_session_policy_without_changing_legacy_pixels_or_history`: both original-asset APIs, 8/16-bit PNG sources, low-cap refusal, configured recovery, sentinel tail/source/project/revision/undo preservation | Pending |
| D8 | `frozen_png_keeps_cpu_decode_allowance_and_preserves_publication_on_refusal` and `frozen_movie_keeps_cpu_decode_allowance_across_live_changes_and_session_close`: immutable CPU policy, previous publication preserved on refusal, original dimensions and successful output after live policy changes/closure | Pending |
| D9 | `info_start_precedes_real_decoder_access_and_does_not_require_debug`: controlled logger gate before real file decoding, default Info context, Debug completion suppressed | Pending |
| D10 | Workspace/all-target application-FFmpeg check, full render/host/media/application suites, frozen PNG/movie publication/pixel guards, style consumer guard | Pending |

Raw logs and source/hash/exit receipts belong in `artifacts/pr-queue` on E:.
No inherited success count or passing syntax check can substitute for these
actual guards on the committed source. The default decoder Info start precedes
native decoding; routine completion is Debug, never stdout. Existing actual
application file/Error-stderr/stdout-isolation tests remain required.

## Limits and remaining issue work

Per-image output/full-source allowance does not bound codec workspace, concurrent
decoded results, proxy rows, disposable disk cache, physical VRAM or process RAM.
Presenter/UI enrollment and integrated lifecycle/native acceptance remain
separate issue requirements. Android is unmaintained; no Android runtime or
new Linux/Metal execution acceptance is implied by common Rust signatures.
