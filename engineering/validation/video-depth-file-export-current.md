# File-backed frozen video depth (#87)

This increment removes the requirement that every authored video depth sample
remain in the 128-entry live cache when an export is frozen. It continues #87;
it does not close the 3D roadmap or enable background composition prerendering.

## Implementation

- Snapshot capture retains immutable file-plane handles and metadata from the
  current document/revision archive, checking object, source, model, geometry,
  indexed interval and saved calibration. Capture does not read entire planes
- Frozen PNG/movie sampling still decodes the actual indexed source RGB and
  verifies its prepared fingerprint before attaching the corresponding plane
- Temporal restoration uses one anonymous file, bounded at 4,096 samples and
  2 GiB. Static resources retain their separate 128-entry/192 MiB bound. Wire
  metadata is bounded at 8 MiB; all resource references and aggregate capacities
  are checked before reading plane payloads
- The strict current snapshot format is `MSDPTH3`. File planes stream in 64 KiB
  chunks with SHA-256 integrity checks. Restoration materializes one bounded
  plane at a time, rather than retaining the whole movie's planes in RAM
- Cancellation preserves the original protocol reason. Failed appends roll back
  only their new tail. A secondary rollback failure does not replace that reason
- Session scalar storage now lives outside the project directory. Applications
  can explicitly configure its cache directory with `configure_depth_in`; the
  default uses the system temporary directory. Validation redirects all temporary
  storage to E. A frozen file handle survives Session/project ownership removal
- Non-Present start/phase records precede storage IO, with existing worker state
  and contextual failure reporting. Healthy Present calls remain quiet

## Evidence and limitations

The first real-codec test failed on Windows when deleting the original project:
an open scalar file was inside that directory. The product ownership fix moved
the anonymous store outside it. The original `original.close().unwrap()` assertion
remains; it was not weakened. The red run is retained in
`artifacts/pr-queue/video-depth-file-export-focused-v1`.

Focused v2 passed seven snapshot-wire tests, nine storage tests and the actual
150-frame codec test. The source-bound full v6 related regression passed 238 distinct tests,
zero failed or unexecuted. The inherited ignored media case was separately
executed and passed on the same source. Desktop FFmpeg/AI depth and no-depth
checks, style and provenance checks passed. This is not current-main full-workspace
or cross-platform acceptance.

[Archived evidence](data/video-depth-file-export-native-v1.zip), SHA-256:
`c65d532b31d66768f83df270898824ba4d861fa775d53cab8098d762cd2e9b93`

All 401 archive entries were read back and verified by SHA-256.

The real five-second H.264 fixture authors and independently verifies 150 actual
decoded frames, observes 128 live versus 150 stored samples, freezes the evicted
first frame, closes the Session and deletes the original project. PNG and a
persistent-restored MP4 then render without another inference call. All 150
decoded output frames are compared with their corresponding native references,
including independent foreground/background occlusion checks. The backend call
count remains 300 across staging and restaging; export adds none.

This fixture establishes beyond-cache ownership and independent output, not
sustained long-duration inference/playback throughput or trained model quality.
It uses Unorm8; the existing short four-precision codec tests remain regression
evidence. Complex effects/masks/mattes/nesting/exposure, intersecting transparency,
DOF and final current-main full-workspace/cross-platform acceptance remain open.
Android remains outside the maintained delivery scope.

No panels, styles, UI interactions, dependencies or third-party assets change.

The full v4 controller stopped after all seven selected model tests passed: its
expected-count minimum was incorrectly eight. The v5 controller checks the
seven test declarations against the source, retains v4 logs and successful
exit codes, and resumes the remaining checks on identical sources. No test
assertion was changed for this verifier correction.

V5 completed every check but its final packaging step lacked the imported v4
render artifacts. V6 imports those artifacts with per-file SHA-256 readback
verification, ties them to the identical source map and successful media exit,
and performs the final comparison checks. These controller corrections did not
rerun or alter product assertions; their original records remain archived.
