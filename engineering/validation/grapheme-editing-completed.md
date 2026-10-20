# Extended grapheme boundaries in text editing

## Scope and coordination

This batch implements the editing part of issue #129 section VI against main `2a9cd061`. Before creating `fix/ui-grapheme-editing`, eleven open PR file lists, eighteen recent local branches and dirty intended paths in all worktrees were inspected. No implementation overlap was found in `widgets.rs` or the new regression file. Shared attribution changes in the FFmpeg-source PR concern its zlib URL; historical manifests and dependency graphs were inspected and retained untouched. No other worktree was edited.

The existing editor counts Unicode scalar values. In `Ae` + U+0301 + `B`, one left or right step can stop between the letter and combining mark; Backspace/Delete can discard only part of the visible character. Modifier/ZWJ emoji, regional indicators, Hangul Jamo and Indic conjuncts have the same boundary problem. This is a functional editing defect, not a logging-only change.

## Implementation

The private cursor tracks extended grapheme indices. Navigation, boundary lookup and deletion use extended grapheme segmentation. Insertion and deletion re-segment the resulting text and move the cursor to the first boundary at or after the edited byte position. Counting the inserted string independently is insufficient: a combining mark can join the preceding character, and a ZWJ or regional indicator can also join the following character. Original UTF-8 text is preserved without normalization.

Segmentation uses the unmodified `unicode-segmentation` 1.13.3 release already present in Cargo.lock. The UI crate declares direct use; no package version or checksum changes. Authors, source, release, checksum, license and modification scope are recorded in ATTRIBUTION.md. The crate's [versioned API documentation](https://docs.rs/unicode-segmentation/1.13.3/unicode_segmentation/trait.UnicodeSegmentation.html) describes the extended segmentation argument; the [Unicode text segmentation standard](https://www.unicode.org/reports/tr29/) defines the boundary model. No implementation tables or third-party visual assets were copied into the repository.

No panel, layout, style token, field dimension, transaction contract or public editor API changes. Existing ASCII, selection replacement, repeated deletion, commit, cancellation and IME behavior retain their original tests and assertions.

## Regression evidence

- Nine independent input regressions operate on the actual editor through public input events and compare exact output strings, without using the segmentation library to derive expected output
- Original production code: **2 passed, 7 failed**, cargo exit 101; evidence: `artifacts/issue-129-grapheme-before.log`
- Fixed production code: **9 passed**, exit 0; evidence: `artifacts/issue-129-grapheme-after.log`
- Replacing insertion re-segmentation with a naive increment by the inserted string's grapheme count: **7 passed, 2 failed**, exit 101; the combining-mark and following-ZWJ cases fail; evidence: `artifacts/issue-129-grapheme-naive-mutation.log`
- Mutation/capture scripts restore production source byte-for-byte with SHA-256 verification in `finally` blocks

## Visual evidence

The actual TextEditor, current stylesheet and UI GPU Painter render both original and fixed behavior into 800 by 400 PNGs at 2x DPI. These are offscreen captures of real widgets, not full native-window screenshots or proposed designs. The local capture helper is retained at `.tools/issue-closure-review/129-grapheme-capture.rs`; its temporary integration entry is removed before the final complete suite and commit.

- `artifacts/issue-129-grapheme-before.png`: Backspace leaves `Cafe`; moving right inserts the separator between `e` and its combining mark
- `artifacts/issue-129-grapheme-after.png`: Backspace removes the complete final grapheme and leaves `Caf`; moving right inserts after the complete grapheme
- The ASCII control crop is identical in every RGBA byte; the complete captures differ in the edited fields
- Actual capture logs: `artifacts/issue-129-grapheme-capture-before.log` and `artifacts/issue-129-grapheme-capture-after.log`
- The unchanged native UI gallery also builds and passes hidden-window smoke runs at 900 by 600 and 1600 by 1000, both at the monitor's actual 1x scale. Both JSON reports have `ok: true` and verify focus/Tab, mixed mouse buttons, one-transaction numeric Undo, floating cancellation/presentation and layout save/reopen. Reports and native-window PNGs use `artifacts/issue-129-grapheme-native-900x600` and `artifacts/issue-129-grapheme-native-1600x1000` as stems; their dock reports confirm the requested logical sizes. These gallery smoke cases verify existing interactions, not additional Unicode shaping support

## Acceptance and limits

- Final complete `cargo test -p motion-ui --locked -j 1 -- --test-threads=1`: **133 passed, 0 failed, 0 ignored**, nineteen result summaries including doc tests, exit 0; evidence: `artifacts/issue-129-grapheme-complete.log`
- `cargo check -p motion-desktop --locked -j 1`: exit 0 with 21 warnings; evidence: `artifacts/issue-129-grapheme-desktop-check.log`
- Complete `cargo test -p motion-desktop --locked -j 1 -- --test-threads=1`: **612 passed, 0 failed, 1 ignored**, exit 0; the existing ignored test requires an actual native audio output device and is not claimed as executed; evidence: `artifacts/issue-129-grapheme-desktop-tests.log`
- New regression formatting, patch whitespace and desktop style-consumer checks pass; provenance passes for 1,525 tracked paths

The UI and desktop suites completed after source restoration and removal of the temporary capture entry. Remote CI remains required before merge. No full nineteen-panel compliance claim is made; this patch adds no panel or visual-style change. Native verification covers the available 1x monitor; the actual editor comparison captures use offscreen 2x rendering.

Issue #129 remains open. Glyph positioning, complex-script shaping, bidirectional layout, font fallback and grapheme-based ellipsis are separate rendering concerns; this change does not claim to implement them. Caret measurement still uses the existing glyph advances. The captures expose that existing combining-mark positioning limit instead of claiming typographic correctness. This batch adds no restart, retry, error suppression or altered existing assertion.


## Native CI integration refresh — 2026-10-20

To clear the existing PR queue, this branch now includes the reviewed #368 head
`1381cc1`, including the already merged #364 Intel budget. The PR's Rust
implementation and regression files are byte-identical to the previous head;
the incoming CI/test infrastructure changes are tracked in
`native-ci-failure-evidence.md`. Main's zlib source attribution update is retained
where applicable. No existing assertion, timeout or ignore policy is relaxed.

This is a real baseline change, not a rerun of an unchanged failed head. The
sampling fixture avoids crossing its original 100-second loop during a slow
run while retaining strict adjacent-frame progress checks. PNG diagnostics
publish in-flight, queue and GPU context on the original timeout; adding that
evidence is not described as fixing the underlying PNG timeout. Evidence paths
are absolute and fall inside the always-upload directory.

The prior per-PR validation recorded above remains historical evidence. A local
combined-tree acceptance run is in progress in `artifacts/pr-queue-complete-tests.log`;
it is not claimed as a full successful run or a replacement for this head's CI.
The local combined timeline-shortcut suite passed all 39 tests and the tools
suite passed 54 of 55 checks with one existing optional external-loader skip.
Those combined results include other PRs, not just this individual branch.

The branch is prepared for stacking on #368, then returning to main after the
parent merges. Actual publication and final remote checks must be verified
separately. No new optimization PR, dependency, asset or visual change is added.
