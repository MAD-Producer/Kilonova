# 3D route: combined professional-compositing integration

This checkpoint combines the external-camera/depth/rational-clock branch and
its three professional-compositing test targets on main `e314cc92`. The source
is merge `e8ff7a1ac1f91b6ec80507a778d91aaef0aff827`. Both parents remain in
history. The only conflicts were adjacent attribution/provenance entries;
both entries are retained. Product and panel code are unchanged by the merge.

## Executed on the combined source

The original package loader, synthetic key, photographed key/holdout and
photographed screen tests execute again, with no assertion, annotation, budget,
ignore, stack or backend changes. Their source and executable identities are
checked before and after execution. Cache and temporary files use E: and this
integration worktree's dedicated target. Execution is serial at default Info.

```text
cargo test -p motion-effects -p motion-desktop-media --test chroma_key_sample --test professional_key --test professional_screen_photo --no-run --message-format=json --features motion-desktop-media/ffmpeg,motion-desktop-media/ai-depth --locked
```

The command links all three requested executables in 1m55s. Each exact executable
then runs with `--test-threads=1 --nocapture`:

| Complete selected target | Passed | Failed / ignored / filtered | Observed execution seconds |
| --- | ---: | --- | ---: |
| `chroma_key_sample` | 1 | 0 / 0 / 0 | 0.875 |
| `professional_key` | 2 | 0 / 0 / 0 | 15.250 |
| `professional_screen_photo` | 1 | 0 / 0 / 0 | 23.047 |

The controller finishes in 162.703 seconds with all source hashes unchanged.
This is four executed tests across three targets, not complete workspace
regression. The optional depth feature is linked; these examples do not run
an ONNX model. [The separate real-inference record](depth74-inferred-render-current.md)
retains its own source and numerical scope.

All 61 newly generated PNGs have identical decoded RGBA **and identical PNG
bytes** to their retained earlier counterparts. That count includes sources,
goldens, native frames, intermediate mattes and frozen outputs, rather than
61 independent shots. The actual screen frame 15 and photographed raw-key
frame 502 were visually inspected again. Since these exact PNG bytes are
already in the retained key and screen archives, the new portable receipt
references their archive/member/hash identities instead of duplicating them.
Every reference was opened and hash-verified.

The [prior key evidence](professional-key86-current.md) still retains its two
dark-clothing classifier failures and the subsequently authored holdout. The
[screen evidence](professional86-screen-photo-current.md) retains its fixed
three-byte budget, two-pixel edge exclusion and static manually aligned screen.
Matching these outputs does not improve those known image-quality limits,
qualify automatic tracking, or establish HDR/other-precision output.

## Main alignment and historical CI

Before the merge, source `90a8d7b1` passed the complete maintained-workspace
**all-target compilation check**, with FFmpeg and optional host/media depth
features, in 328.344 seconds. Main was fetched again before this record and
remained `e314cc92`; all new-main video warmup and text edits are included.
That check is not a workspace test execution. The merge adds the three targets
executed above; it does not change product source from that checked baseline.

PR #612's earlier head `e4bb4650` completed
[run 41980136720](https://github.com/MAD-Producer/Kilonova/actions/runs/41980136720)
with eight successful jobs: audit, Rust/GPU, three desktop test platforms and
three desktop builds. All eight complete job logs and run/job metadata were
retrieved and hashed. Both original key tests appear exactly once as passed
in each Windows, Ubuntu and Intel macOS test log. The workflow's FFmpeg/libav
and publication phases were skipped by its PR policy. Those old-head results
are not transferred to this combined source or to the new photographed screen.

## Remaining delivery gates

The latest combined source still requires full maintained-workspace execution
and its own terminal cross-platform CI. Earlier full desktop/UI and host/media
native access violations remain retained and unresolved. The next whole-workspace
attempt stopped with linker disk exhaustion before executing any tests; neither
this selected pass nor an isolated successful rerun repairs those failures.

The separate PR heads `5c6c654b` (#606) and `b1643aa1` (#612) had live CI when this
checkpoint was recorded. This combined source has not been pushed. Existing
runs are preserved. The intended delivery uses one combined source for final
integration rather than counting two divergent green heads as one result.

Issues 74, 85, 86 and 88 remain open. Android is retired and excluded by the
user's scope. This checkpoint does not close the full 3D route or accept the
separate tracker prototype's failed real-sequence quality gate.

## Portable evidence

[Archive](data/3d-professional-e8ff7a1a-integration.zip) and
[per-member manifest](data/3d-professional-e8ff7a1a-integration.json) retain the
combined-source controller, original stdout/stderr, source/executable identities,
public edit/project/frozen receipts, exact image references, old-head CI logs,
main compilation-check evidence and the executed test/sample sources. Every
archive member was read back and compared with its original size and SHA-256.

Archive: 830,016 bytes, SHA-256
`81d049477f7d475c12a256ee6a13523ccdc88954ebe7082e43496eb59597c6fd`.
No executable, model weights, runtime or new product asset is included.
