# Issues 85/86: current-main clock integration

Status: scoped integration passed on `7f5f0c999ca45c22f33a86c8e87ddc8f5b5fb5dd`,
which includes main `c0e25eda`. Complete desktop runtime and final CI remain pending.

The complete host and desktop all-target compilation check passed in 77.985
seconds. The two media video-candidate regressions passed in 31.079 seconds,
including compilation. The actual desktop helper `--task check` passed in
14.485 seconds. The original controller session 61191 exited zero and was
consumed. The source closure remained unchanged throughout the gate.

Both Git tree comparison and file hashes establish that motion-render,
motion-core, motion-model, all panels, the native clock regression and its
Shell registration are unchanged from `b015ddad`. The separate
[complete renderer result](professional-api86-final-render-current.md) remains
bound to that earlier head; this record does not relabel it as a complete
current-main desktop run. Main integration added the original-video publication
checks, which were compiled and exercised through media-candidate controls here.

The [receipt](data/frame-rate85-current-main-check-v1.json) and
[archive](data/frame-rate85-current-main-check-v1.zip) retain exact commands,
raw output, source hashes, terminal receipt, environment and verified archive
members. Complete desktop execution still needs resolution of the original
precision-menu worker stack overflow and authorization for the separate graph
fault fixture. The older native heap failure also remains unresolved. No
assertion, ignore marker, runtime budget or quality threshold changed. No
Android acceptance or issue completion is claimed.
