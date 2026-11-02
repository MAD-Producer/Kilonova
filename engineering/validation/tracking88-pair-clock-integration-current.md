# #88 paired replacement on the integrated layer-time baseline

PR #583 integrates parent #581 `7cf168a45b948ae8ed15384320dbc90c10ef0699`, including maintained
main `9d085c0f` and its layer stretch/reverse/source mapping. The only textual
conflict is in `Command::composition_transaction`: continuous spatial, local
curve and handle edits retain main's early-return policy, while
`ReplaceTrackedVideo` retains its atomic composition transaction. Both command
families and current-only project schema 36 remain present.

The new core control is reused unchanged from PR #587's combination proof. It
stretches a real video layer's property clock by 3/2, then replaces its original
video and camera solve. It verifies preserved authored layers, a changed camera
track, distinct revisions, exact full-project undo/redo through both edits, and
current-schema serialization/reopening. No existing assertion is changed. This
is a core authoring/history proof, not a native video timing proof by itself.

## Executed gates on frozen integrated source

- model-core: 859 passed, 0 failed, 1 ignored; 137 summaries, 0 filtered
- host-lib: 169 passed, 0 failed, 0 ignored; 1 summaries, 0 filtered
- host-integration: 15 passed, 0 failed, 0 ignored; 2 summaries, 0 filtered
- media-video: 14 passed, 0 failed, 0 ignored; 1 summaries, 88 filtered
- paired-native: 1 passed, 0 failed, 0 ignored; 1 summaries, 1746 filtered
- audio-native: 1 passed, 0 failed, 0 ignored; 1 summaries, 1746 filtered
- mapped-native: 1 passed, 0 failed, 0 ignored; 1 summaries, 1746 filtered

The paired native control crosses actual FFmpeg import, original-source SHA and
PTS validation, GPU capture, real staged-resource rollback, successful atomic
publication, undo/redo and reopening. The audio control additionally verifies the
native linked audio/PCM/waveform resources and independently timed audible
pulses. The inherited mapped-video control independently compares four explicit
clocks (stretch, reverse, linear source remap and hold) to native reference images.
Their combined mapped-pair boundary is proven by the core control; no claim is
made that these separate native tests exercise every mapped-pair combination.

MOTION_LOG is unset for all commands. Native paired and audio file logs contain
Info admission starts, and actual failure records appear in both file and
stderr, with those records absent from test stdout. This is test-process stdout
isolation, not an MCP JSON-RPC transport proof. Each native test executes in its
own process because the logger initializes once. All native commands explicitly
enable ffmpeg; their filtered test counts are not counted as passes.

The [receipt](data/tracking88-pair-clock-integration-v1.json) and
[read-back verified archive](data/tracking88-pair-clock-integration-v1.zip) retain
the executed commands, all relevant source hashes, merge resolution/new-control
snapshots, raw output and generated native images. Source remained unchanged
throughout execution; snapshots are checked against staged LF-normalized blobs.
Existing manual ignores are retained and reported, with no new ignore added.

No panel differs from maintained main or the integrated parent. No dependency,
asset, UI, layout, interaction, retry or worker timeout is authored here. The
large merge stat mostly inherits main's independently authored layer-time work.

## Remaining acceptance

This result does not establish a fresh complete desktop package or workspace
regression. Historical desktop package results on a different source revision
remain historical, including the earlier native crashes whose root cause is
still undetermined. Final-head cross-platform CI, retained native Surface
acceptance on this source, the photographed tracker quality gate, prerequisites
#85/#86 and full #88/3D-route acceptance remain separate work. Android is retired
and excluded. This PR is already formal; these bounded checks do not close #88.
