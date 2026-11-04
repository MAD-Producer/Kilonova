# Issue 88: explicit external camera text conversion

Status: ten complete conversion-target tests and the host check passed on the
recorded source, based on main `351d09b2`. This is a pure conversion increment,
not completed issue 88 or a managed-source import UI.

The `colmap-cameras-images-text-v1` profile consumes `cameras.txt` and
`images.txt`. It accepts SIMPLE_PINHOLE, PINHOLE, SIMPLE_RADIAL, RADIAL and
OPENCV, retaining all their supported lens coefficients. Other models, including
fisheye and rational FULL_OPENCV, are rejected rather than approximated.

The caller supplies encoded-source identity, extent, signed exact PTS, timebase,
nominal rational rate, one exact image-name mapping per source frame,
confidence, world basis, scale gauge, target units and origin. No metadata is
guessed from filenames, camera identifiers or nominal fps. Source IDs are not
verification of encoded media: the existing source connector remains required.

Documented Hamilton world-to-camera poses are inverted to camera-to-world,
then passed through the existing explicit basis/unit converter. Camera-local
axes stay right/down/forward. COLMAP and the canonical contract both measure
pixels from the upper-left image edge; no half-pixel offset is added.
Unregistered mapped images produce Lost samples with zero confidence and no
observation. Extra registered photographs remain in the retained originals
and are counted separately. Missing poses are not interpolated or held.

Both original byte documents and conversion options are returned to the caller.
The adapter performs no file IO, logger writes, project edits, tracking solve,
camera-key baking or retries. Typed failures retain input, line and frame
locations and underlying numeric/asset causes without echoing payload text.
The consuming import boundary must log actual failures with request context.

Combined text is bounded to 16 MiB; each data/keypoint line to 1 MiB; cameras
to 4096; images to 16384; mapped source frames to 4096; image names to 1024
UTF-8 bytes without whitespace/control characters. Duplicate identifiers or
names, dangling camera references, invalid metadata, malformed feature triples,
truncated records and unsupported models fail the entire conversion.

Remaining: managed project attachment of both vendor originals and options,
transactional source verification/import, actual exported reconstruction
acceptance and independent held-out reprojection. Existing canonical
camera/lens binding acceptance does not prove these new adapter boundaries.
No panels or other UI files change. Android remains excluded.

## Executed evidence

`cargo test -p motion-model --test tracking_colmap --locked --offline --
--test-threads=1` passed 10 tests with zero failures, ignores or filters.
`cargo check -p motion-host --locked --offline` passed, including the public
host reexport. This is not a complete model/workspace test run.

The tests exercise independent quarter-turn/translation and five hand-computed
lens projections; signed nonuniform PTS and fractional rate retention;
canonical encode/decode; exact original bytes; missing and extra registrations;
unit/basis conversion; reflected poses; malformed, duplicate and truncated
records; unsupported models; input/name limits; redacted errors and original
typed causes. Inputs are independently authored numerical fixtures, not copied
upstream reconstructions. No photograph or solver is represented by these tests.

Original validator session 36918 exited zero. Compilation and tests took
103.907 seconds, with all recorded source hashes unchanged. Controlled caches
and temporary files used E. The controller expected Cargo's build lock to
serialize with the running desktop gate; Cargo released it during runtime,
so the CPU-only conversion/check actually overlapped the unchanged desktop
test binary. Only one own GPU workload executed. The original controller's
expectation is retained and corrected in the independent receipt.

The [archive](data/tracking88-colmap-adapter-v1.zip) retains exact source,
commands, raw output, source closure and environment, with every member read
back and hash-verified. The [receipt](data/tracking88-colmap-adapter-v1.json)
records counts and limits. Exact-head cross-platform CI remains required.
