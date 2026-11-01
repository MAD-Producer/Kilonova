# External tracking tools

`track_plate.py` is an optional developer point/planar tracking prototype for
issue #88. It produces a tracking JSON file; it does not install a product
tracker, modify a project, or solve a 3D camera. Native admission/render and
photographed-motion qualification remain separate acceptance gates.

Use separately installed `opencv-python-headless==5.0.0.93` and `numpy==2.4.6`,
and explicitly selected local FFmpeg/ffprobe executables. Sources and licenses
are recorded in the repository's `ATTRIBUTION.md`; nothing is downloaded by the
tool. Run from the repository root:

```text
python tools/tracking/track_plate.py --source plate.webm --request request.json --output new-run --ffmpeg /absolute/path/to/ffmpeg --ffprobe /absolute/path/to/ffprobe
```

The output directory must not already exist. A minimal request is:

```json
{
  "version": 1,
  "source_sha256": "<64 lowercase hexadecimal characters for the original file>",
  "stream_index": 0,
  "tracks": [
    {"id": "feature", "kind": "point", "position": [80.0, 70.0]},
    {"id": "plane", "kind": "plane", "roi": [32.0, 24.0, 128.0, 104.0]}
  ],
  "corrections": []
}
```

`stream_index` is the global container stream index. Coordinates refer to raw,
unrotated source pixels. Original signed frame PTS and rational timebase are
retained; nominal FPS is metadata, not a replacement clock. The prototype
requires square pixels, a constant raw extent, strictly increasing PTS, at most
4096 frames/16 tracks, and source bytes no larger than 2 GiB. It refuses an
oversized source instead of silently truncating it.

Automatic observations use forward/backward optical flow. Planes additionally
require RANSAC inliers and independent held-out correspondences. A lost track
stays lost until an explicit correction. Each correction has `track`,
`source_index`, `visibility`, and `observation`. A visible observation is
`{"kind":"point","pixels":[x,y]}` or
`{"kind":"plane","matrix":[[a,b,c],[d,e,f],[g,h,i]]}`. Plane matrices map
reference source pixels to current source pixels. Lost/occluded corrections
must have a null observation. The tool retains manual-correction identity.

`report.json` publishes phases and actual child PIDs before potentially blocking
IO. `tracking.log` has Info starts at the default `MOTION_LOG=info` level; Error
also goes to stderr and stdout stays empty. Per-track consecutive automatic loss
logs Debug at occurrence 1 and Error once at 60, resets on recovery/manual
states, and retains bounded causes. Sixty occurrences is not a duration.
Child stderr is drained with retained output capped at 256 KiB per invocation.
External tools have no hard execution deadline; the phase/PID records help
diagnose a tool that never returns. Cancellation stops the owned child.

Successful output is `tracks.mstrack`, written only after checking original
source identity again. Its report deliberately says native admission is pending.
Before product use, pass the output through the actual native importer and
qualify bindings, rendering and persistence against the original source.

Reproduce the authored synthetic evaluation (FFmpeg needs `libvpx-vp9`):

```text
python tools/tracking/validate_prototype.py --ffmpeg /absolute/path/to/ffmpeg --ffprobe /absolute/path/to/ffprobe --output new-evaluation
python -m unittest discover -s tools -p test_ci_plate_tracking.py -v
```

The evaluator hides analytic motion from automatic tracking, checks independent
geometry, loss/manual recovery, and default-level Error suppression. It records
the actual encoded file identity; container bytes can differ across encodes.
Ordinary CI admission tests need neither OpenCV nor FFmpeg.
