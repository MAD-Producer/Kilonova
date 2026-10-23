# Font and audio CPU memory observations

Issue: #60. `motion_media::fonts::FontStore::memory_info()` and
`motion_media::AudioMixer::memory_info()` return explicit metadata. Neither
method reads files, rasterizes text, mixes samples or changes cache order. They
are not called by ordinary playback. `CpuMemoryInfo` separates known payload
lengths from actual Vec backing capacities and includes ownership references.
These are CPU allocation observations, not process residency or GPU memory.

The font observation counts actual glyph coverage Vecs retained by the store.
Arc aliases count once; equal contents in independent allocations remain
separate. Font IDs, code points and sizes identify the bounded cache references,
without retaining glyphs or exposing pointer values. The original booked glyph
payload counter remains a separate cache-policy instrument. Parsed font
internal storage is unknown; evicted glyphs or returned rasters still held by
callers are outside the store's ownership snapshot. A zero cached subtotal does
not prove that those external references have released their memory.

The mixer observation includes input byte scratch, f32 sample scratch and
resampling tables. Each table element is `[f32; 64]`; its bytes are measured
using the actual element size and Vec capacity. Multiple sources sharing the
same Arc table count it once within the mixer. Identifiers are local to the
snapshot; source asset IDs describe references. Metadata never reads or seeks
PCM file cursors. PCM disk file size and OS file-cache residency are not counted
as mixer RAM. Project/voice/container storage and external allocations remain
outside this instrument.

Details are bounded by existing ownership limits: at most 2048 glyph cache
entries and at most the validated project's 256 media assets. No persistent
observation-key map or strong resource reference is retained. Parser, allocator,
Arc and container overhead remain explicit exclusions; `unknownBytes` stays null
and `completeCoverage` stays false.

Validation must exercise real built-in glyph caching and eviction, owned spare
capacity, shared versus distinct buffers, real PCM cache files, shared and
distinct sample-rate tables, byte-exact mixing after diagnostic queries and
capacity release. No new font/audio asset or dependency is introduced. Session
query integration and its acceptance are tracked separately while the existing
Session CPU resource PR is pending.
