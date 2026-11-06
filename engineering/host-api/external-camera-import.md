# External camera text import

The existing `import_tracking` MCP tool accepts either the canonical tracking
JSON contract or an explicit `colmap-cameras-images-text-v1` JSON envelope.
Neither format changes ordinary camera keys or applies a camera binding on
import. Select an existing managed image/video source, fetch current revision
and document generation, then call `import_tracking` with its explicit source
kind/id and a local document path. Source text cannot select a filesystem path
or choose a different managed source.

The model helper `tracking::document::colmap_to_vec` creates a bounded envelope
from the two UTF-8 text documents and the typed `colmap::Source`, `Frame` and
`Options` inputs. `colmap_from_slice` recovers byte-exact vendor text, conversion
options and the frame-name map. Its normalized asset is separate from those
originals. The envelope uses required fields:

| Field | Meaning |
| --- | --- |
| `format` | Exact profile `colmap-cameras-images-text-v1` |
| `cameras`, `images` | Original `cameras.txt` and `images.txt` text, JSON-escaped without normalization |
| `source.id`, `source.sha256` | Bounded source label and 32 encoded-source digest bytes |
| `source.extent` | Original width/height, before display crop/rotation |
| `source.ticks_per_second`, `source.pts` | Timebase and complete exact signed frame PTS, including VFR |
| `source.nominal_rate` | Reduced numerator/denominator metadata, not a substitute for PTS |
| `frames` | Ordered complete source-index/image-name/confidence mapping |
| `options.track_id`, `options.world_basis` | Track identity and signed-axis permutation mapping external world axes to canonical world axes |
| `options.source_meters_per_unit`, `options.target_meters_per_unit` | Explicit reconstruction scale gauge and target units |
| `options.target_origin` | Translation in target world units |

The format contains no implicit confidence, units, image-name sorting or fps
inference. COLMAP does not establish a metric scale by itself. Supported camera
models are SIMPLE_PINHOLE, PINHOLE, SIMPLE_RADIAL, RADIAL and OPENCV. Unsupported
models fail without dropping distortion. Missing registered image poses are
Lost observations with zero confidence; they are not silently filled.

The existing transactional importer verifies the selected encoded source and
decoded extent; video also requires every original decoded PTS to match. Only
then does it publish the complete envelope as the managed `.mstrack` original
with `colmap_camera_text_v1` descriptor format and normalized camera samples.
Save/load and project packaging verify document digest, format, source digest
and normalized-data equivalence. The original text and conversion map/settings
survive undo/redo and reopening after the external input disappears.

The JSON envelope is limited to 16 MiB including escaping and metadata, in
addition to the underlying conversion limits. A valid raw text pair near its
maximum may exceed the envelope limit after JSON escaping and is rejected,
never truncated. Frames and source timestamps are bounded during decoding to
4096 entries. Unknown fields and unsupported profiles are rejected. Error
locations and numeric causes survive the host import boundary; arbitrary user
payloads are not echoed into diagnostics. Default Info start and Error
file/stderr records come from the existing serial worker/import boundary.

The pure conversion tests and managed image-import controls do not establish
photographed tracking quality, full camera solving, arbitrary codec support or
the complete issue 88 acceptance. No new UI controls are included; existing
camera bindings remain explicit. Android is outside the maintained scope.
