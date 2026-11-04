# Runtime regression after current main integration

Frozen combined source `2927799f1fd2f3af06f9f5eb83c52323b91a00f2`, including main `351d09b2ffebb4ea1b445d2dc16f2920a9075207`, passed
desktop/FFmpeg check, seven selected render targets, host unit tests and complete
desktop/UI tests including UI documentation tests. Accepted commands exited zero
and every captured source hash remained unchanged.

| Actual scope | Passed | Failed | Original ignored | Filtered | Targets |
| --- | ---: | ---: | ---: | ---: | ---: |
| render | 100 | 0 | 0 | 0 | 7 |
| host | 176 | 0 | 0 | 0 | 1 |
| desktop-ui | 1977 | 0 | 72 | 0 | 31 |

Check/render/host results come from the first attempt on exactly the same source.
Its subsequent desktop/UI compilation failed: the UI `path_antialiasing` rustc
process exited one without a specific diagnostic, before any runtime test.
The raw failure is retained, not described as a source error or an established
compiler crash. An initial diagnostic observer then failed before compiler
launch because of its own text decoding. A separate exact-command experiment
with UI test `codegen-units=4` compiled successfully; no runtime acceptance is
claimed for that diagnostic alone.

The complete desktop/UI second attempt uses test-only `codegen-units=4` for
both desktop and UI. Cargo invocations were matched against metadata. No
assertion, execution deadline, original ignore, stack, adapter, backend or
production compiler profile changed. Earlier check/render/host counts are
included once, and historical parent model/core counts are not added.

All controlled caches and temporary files used E, with default Info and adapter
selection. One own GPU workload executed. A later CPU-only camera-adapter
conversion/check ran against a separate source worktree while the static
desktop test binary continued; no second GPU test was launched. It does not
change this captured source closure or establish freedom from machine load.

This evidence covers main integration source, including video lens rendering
and node geometry. It does not prove repair of historical compiler/native
failures, original CI expression admission timing, photographic quality or the
entire 3D route. Android is excluded. Final exact-head cross-platform CI and
remaining issue-level acceptance stay separate requirements.

The [archive](data/frame-rate85-aligned-runtime-v2.zip) retains both attempts,
diagnostic records, source hashes, metadata and raw output. Every member was
read back and hash-verified; the
[receipt](data/frame-rate85-aligned-runtime-v2.json) binds scope and limits.
