# SDK parity matrix

What the desktop host supports today, compared with the Android build. "Shared"
means the same code in `motion-host` runs on both platforms.

Legend: **yes** shipped · **partial** works with a documented limit · **no** not
yet implemented on desktop.

## Desktop editor entry-point audit

Shared engine support and an editor entry point are distinct. This table names
the current desktop route and keeps unfinished UI work explicit.

| Shared capability | Desktop entry | Remaining UI work |
| --- | --- | --- |
| Stereo audio envelopes | Window → Audio; dB, animation and key navigation | Timeline audio tracks, audio-only playback and scrub policy |
| Composition creation | Composition → New Composition and Ctrl+N | Composition organisation and project-panel folders |
| Five-channel colour curves | Engine API; effect curve rows currently show a placeholder | Interactive effect colour-curve editor |
| Layer copy/paste | Engine command surface | Desktop layer clipboard and Edit menu |
| Native plugin editors | Generic effect parameters and source/bind controls | Complete declarative slot layouts |
| Preview/memory policies | Shared API and runtime diagnostics | Preferences and full preview-settings UI |

## Editing

| Capability | Desktop | Notes |
| --- | --- | --- |
| Keyframes, tracks, curve editing | yes | Shared (`motion_core::animation`, `curve`) |
| Undo, redo, gestures, coalescing | yes | Shared (`motion_core::editor`) |
| Bezier and elastic curves | yes | Shared (`motion_core::curve`) |
| Axis separation (XYZ) | yes | Shared (`Command::SeparateDimensions`) |
| Camera: position and orbit paths | yes | Shared (`motion_core::camera`) |
| Observer, top and side views | yes | Shared |
| Layer clips: move, trim, split | yes | Shared |
| Masks | yes | Shared (`motion_core::masks`) |
| Vector shapes and paths | yes | Shared (`motion_core::vector`) |
| Adjustment layers | yes | Shared |
| Nested compositions, precompose | yes | Shared (`motion_core::composition`) |
| Expression workspace | yes | Shared QuickJS runtime |
| Multi-select and batch edit | yes | Timeline Ctrl/Shift selection, clip movement, duplicate/delete and atomic effect application |
| Copy/paste layers with parents | partial | Shared duplicate/batch editing; desktop layer clipboard still pending |

## Effects and plugins

| Capability | Desktop | Notes |
| --- | --- | --- |
| 59 core effect packs | yes | Byte-identical `.msfx`; identity is the SHA-256 of the package |
| 6 scene effect packs, 1 emitter pack | yes | Same registry, same byte pinning |
| Effect chains, ordering, enable | yes | Shared |
| Parameter animation | yes | Shared |
| Five-channel colour curves | yes | Shared (`motion_core::color_curves`) |
| Per-instance parameter ranges | yes | Shared (`PlanBuilder::preflight_project`) |
| Effect pack install/enable/uninstall | partial | Shared (`Registry`); .msfx import picker exists, full pack management UI pending |
| SDK 5 native plugin editors | partial | Engine side shared; slot widgets not drawn yet |
| SDK 2 HTML plugin editors | no | Needs WebView2 or WKWebView |

## Media

| Capability | Desktop | Notes |
| --- | --- | --- |
| Audio import (MP3, FLAC, AAC, ALAC, Vorbis, Opus, WAV, AIFF) | yes | Shared Symphonia path |
| Waveform generation | yes | Shared |
| Deterministic mixing, 48 kHz stereo | yes | Shared (`motion_media::mixer`) |
| Stereo dB envelope editing | yes | Timeline → expanded Audio group: numeric fields, animation, key navigation, drag/delete; audio, video and composition references |
| Static audio gain in dB | yes | Timeline Audio rows: numeric typing/scrubbing, unity/silence and independent mute for audio/video; Audio panel links to timeline |
| Audio reference markers | yes | Layer context menu → Audio → Mark audio references; cancellable background energy-rise analysis, manual correction, clip-relative save and atomic undo |
| Stereo channel separation | yes | Layer context menu → Audio → Separate left/right channels; independent L/R tracks with exact original stereo sum |
| Video import (H.264, H.265, VP8, VP9 in MP4/MOV/MKV/WebM) | partial | libav behind `--ffmpeg` |
| Video preview and PTS prefetch | partial | Shared scheduling, libav decode |
| Project package import and export | yes | Shared (`motion_core::storage`) |
| PNG still export | yes | Shared |
| MP4 export with AAC audio | partial | File → Export MP4 uses the existing libav encoder when built with FFmpeg; disabled otherwise |
| Device decoder inventory query | yes | Fixed list rather than `MediaCodecList` |

## Known gaps and why

**MP4 export.** The desktop libav encoder and File menu route exist. Builds without
the FFmpeg feature report no video encoder and disable the entry. The shared
frozen frame/audio plan does not imply that every distributed build includes
the encoder or its runtime libraries.

**Text layers.** Layer → New Text uses the desktop font dialog and platform text
rasterisation. Full character/paragraph styling controls remain a separate UI
integration task.

**Native plugin editor widgets.** `PluginEditorSession` is shared and the
protocol is declarative, so the engine half is done. `motion-ui` still needs to
render the slot kinds: parameters, layer source, image sprite, seed, transform,
preview, timeline and note.

**Audio timeline.** Gain, stereo envelopes and reference markers are available
in expanded timeline Audio rows. Audio-only transport and audible scrubbing still
need desktop UI integration; waveform generation does not imply a waveform UI.
Energy-rise references assist manual editing; they do not estimate BPM or guarantee
a musical beat grid. Markers and isolated channels use project format 18;
older projects retain their version until these features are used.

**Plugin pack management UI.** Shared install, enable and uninstall operations
exist. The effect library already imports .msfx through a native file picker;
complete enable/uninstall management still needs editor controls.

## How to keep this honest

`crates/motion-host/src/ops/snapshot.rs` builds the `capabilities` block that both
UIs read. When a host cannot satisfy a capability it must report it there rather
than omitting a field, so this table can be regenerated from a running build
instead of being maintained by hand.
## Desktop workspace and automation

Basic desktop panels now support typed numeric transform editing, animation toggles, timeline scrubbing, clip movement, undo/redo, project open/save and package export. Panels can resize, dock, combine tabs or float in native windows, including the composition preview. Chinese and English UI preferences persist separately from projects.

Composition → New Composition uses the existing Ctrl+N dialog and shared
composition command. It adds a composition without replacing the project.

The stdio MCP adapter exposes state, atomic revision-checked editing, seek, history and save. --mcp-ui shares the displayed project and undo history with the client; --mcp is headless. Node authoring and msfx export from nodes remain a design proposal.
