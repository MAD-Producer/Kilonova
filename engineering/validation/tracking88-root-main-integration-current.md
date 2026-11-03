# #88 root PR integration with main

PR #580 previously conflicted with main's layer-time changes. This merge joins
tracking head `469412c5d61be1a8dfab54d534e0b2398beec2d1` and maintained main `9d085c0f1fb63106babb48d5781b700e20da6eac`.
The downstream #587 integration is separate evidence and is not substituted for
validation of this smaller root source.

## Resolution and scope

All five textual conflicts preserve both features: tracking and layer-time
command registration, per-object undo accounting, typed error context, and the
combined model. The current-only project schema is **36**, matching the combined
shape rather than retaining either divergent version 35 or 32. No migration or
fallback loading of old schemas is added. Main's continuous-edit routing remains
intact. Later atomic picture/solve commands remain in their own dependent PRs.

The three exact point, plane and camera clock controls from #587 also run here.
They cover 3/2 stretch, forward/reverse direction, fractional local frames and
authored corrections; point bindings additionally cover either speed sign.
Expected local instants are tabulated independently. Binding identity and
original PTS metadata remain unchanged. No existing assertion is weakened.
Bindings retain explicit target property clocks; this adds no implicit following
of another footage layer's source remap.

No panel code differs from the integrated main. The large merge stat includes
main's existing changes and is not newly authored UI work.

## Source-bound verification

- Complete model/core packages: **851 passed, zero failed**,
  1 pre-existing ignored manual measurement, across
  136 target/doc-test summaries
- Seven complete renderer targets: **99 passed, zero failed**,
  0 ignored: tracking_lens, layer_time_nested, model_layers,
  video_yuv, video_instances, video_lifetime, video_allocation_budget
- Actual native Windows desktop image-relink control: **one passed**, verifying
  nonempty GPU output, equal-length wrong-source refusal, unchanged-byte relink,
  undo/redo/save/reopen, default Info start, Error file/stderr and stdout isolation
- Tools: 110 discovered, 109 passed and one existing optional native-loader skip
- Provenance, new-control formatting and staged whitespace checks passed

All recorded Rust/manifests (and shader inputs for native/render checks) remained
unchanged through their respective invocations. The first invocation failed
before testing because the shell lacked cl.exe. Its unchanged-source receipt
and raw build failure are retained. The second loaded the already installed
portable MSVC environment; no dependency was installed or assertion changed.

The [receipt](data/tracking88-root-main-v1.json) and
[verified source/log archive](data/tracking88-root-main-v1.zip) retain commands,
input hashes, raw failures, terminal results and native artifacts. Archive members
were read back and hashed before publication; resolution/control source matches
the staged LF-normalized content.

## Remaining limits

Final-head cross-platform CI is still separate. The native control is offscreen
image relink, not retained-window Surface acceptance or the later video/paired
replacement workflows. This is not a new full desktop/workspace regression and
does not qualify the failed photographed tracker or complete #85, #86, #88 or the
full 3D route. Android remains outside the maintained scope. Historical native
crashes and exact external-calibration reproducibility are not declared fixed.

The evidence publisher completed archive verification and wrote the document,
then failed while decoding the remote Chinese PR body with Windows' implicit
GBK encoding. A separate finalizer uses explicit UTF-8 and re-verifies all archive
members and staged source before preparing publication text. No validation was
rerun or reclassified because of this metadata-only failure. The native control
ran one test; the remaining 1741 binary tests were filtered, not passed.
