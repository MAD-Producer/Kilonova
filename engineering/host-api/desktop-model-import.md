# Desktop owned model import

Implementation status: the worker, MCP tool and main-window dropped-file
action are connected. Their new regression suite is pending execution; this
document specifies the source contract, not a completed acceptance claim.

## Entry points

The desktop MCP tool `import_model` accepts exactly `path` and
`expectedRevision`. The path is a nonempty string of at most 4096 UTF-8 bytes;
the revision must be the current nonnegative integer from `motion_state`.
Read-only agents are refused before source access. Unknown fields, a missing
revision and a stale revision are refused without changing the document.

For example, after reading the current revision:

```json
{"name":"import_model","arguments":{"path":"E:/models/triangle.glb","expectedRevision":12}}
```

The main-window `DroppedFile` event dispatches `Action::ImportPath`; recognized
model extensions enter the same serial worker through `Shell::import_file`.
The queued UI operation binds its source document/composition generation;
superseded work returns `modelImport.cancelled=true` before opening the file.
Cancellation is normal, not a model-import error. No physical OS input
injection is claimed by the automated shell capture.

The standard project-library picker and registration/placement workflow remain
separate entry routes and do not yet admit models. A model file drop creates a
layer; it is not library-only registration. Floating-window drops are not
covered by this increment. Shared host media capabilities remain separate;
desktop support is declared under `desktop.modelImport`.

## Ownership, placement and history

Extensions `gltf`, `glb` and `obj` are matched case-insensitively. Import uses
the existing bounded CPU importer and source-relative dependency resolver.
The actual source read is bounded as well as its metadata length. The chosen
scene is the explicit glTF default, or scene zero when no default is authored;
an empty selected scene is refused. OBJ uses the documented default right-handed,
Y-up metre interpretation. No animation starts automatically.

The source and dependencies are frozen into a uniquely, exclusively created
`assets/models/model-<asset>-<stamp>.mscn` file. Each asset-directory component
is checked against the canonical project root before creating the next one.
The bundle is written, synced and closed before the ordinary prepared batch
validates and registers it. The registration and new model layer share one
undo boundary. A refused batch removes only this operation's exclusively
created file; it must not remove unrelated files. Undo retains the bundle so
redo can restore the layer. Saving and reopening does not require the original
model, MTL, buffer or texture paths.

The new layer is 3D, centered in the current composition, with 100 pixels per
metre, pivot `[0,0,0]`, one raster sample and the imported rest pose. Its timeline
starts at the current integer composition frame and ends with the composition.
There is no bounds autofit, automatic camera activation or automatic animation
selection. Ordinary transform tracks and existing model-clip commands can edit
the resulting layer. The returned `modelImport` record identifies `asset`,
`object`, `scene`, `pixelsPerMetre`, `samples`, `animation` and `owned`.

## Failures and verification

Worker Info start precedes handler entry. The importer also records Info start
before source IO with request ID and bounded source context. Failure keeps its
original typed scene/IO cause and reaches the existing default file and Error
stderr channels; stdout remains reserved for MCP. No worker timeout, restart,
retry or altered best-effort reply semantics is introduced.

New controls cover all three source formats after source deletion and reopen,
actual native red pixels, one-step undo/redo, missing dependencies, asset-ID
namespace sharing, schema/access/revision checks, superseded UI work, real
publication refusal at the existing layer limit, owned-file cleanup and
recovery. A separate native-editor capture enters through `Action::ImportPath`
and checks actual composition pixels. These controls must actually pass before
this increment is accepted; listing them here is not evidence of execution.
