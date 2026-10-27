# Native model receivers and temporal exposure

Status: the functional repair passes native acceptance with 359 tests across
14 distinct targets, zero failures or ignores, and FFmpeg/AI-depth desktop
compilation. The unchanged parent reproduces the regressions. Maintained-main
alignment and cross-platform merge-ref CI remain separate.

## Reproduced functional boundaries

The ordinary composition path prepares model rasters before its effects. The
retained receiver path previously prepared nested compositions only. A cold
model receiver therefore failed with `effect input texture Model(1) is missing`;
a motion-blur task failed at its first native receiver sample for the same cause.
The unchanged `9ffe5ea5` parent plus the new tests reproduces both failures in
`model-exposure-red-v2`: zero passed, two failed, zero ignored. Red-v1 compiled
identical fixtures but was stopped before GPU execution when the independent
calibration harness moved to its final gate. It is not a native regression run.

Preparing native inputs alone removes the missing-source error but still applies
the authored object projection and bounds to an image already rendered through
the composition camera. `model-exposure-green-v1` records maximum pixel error255
in the same two unmodified regressions; 48 other related tests pass. The retained
failure includes exact source and raw output. No assertion or tolerance changes
are made to turn either failure green.

## Final repair

Retained materialization prepares native model inputs for that exact sampled
scene before effects copy into an independently owned receiver image. Native
draw/upload statistics join existing child and effect statistics; original
errors retain receiver, sample and phase context. There are no retries.

Sampled model source dimensions and source rectangles describe the composition
raster, while the authored object size still supplies the original native world
transform and anchor offset. Effect planning starts these sources in composition
space. It does not apply the native model's world/camera transform a second time.
Other layer source dimensions and the existing scene-generator setup are retained.

The two new regressions use saved/restored projects and remove the frozen model
file after asset configuration. Cold snapshots cover OBJ and imported transform,
morph and joint animation, then compare both independently retained poses with
ordinary native rendering and the known authored motion. A separate shutter task
compares every channel against the arithmetic average of four ordinary native
subframe renders, with the existing one-byte exposure bound. Trajectory coverage
must exceed the sharp center sample, preventing a repeated-center image from
passing. Authoring remains exactly unchanged.

## Broader gate and remaining scope

The frozen broader gate covers render library units and 13 integration targets:
models, retained layer images, exposure schedules/tasks/accumulation/budgets/SDR,
nested exposure, masks/error boundaries, scene effects, snapshot execution and
ordinary effects. It also checks FFmpeg/AI-depth desktop compilation, provenance
and panel-style consumers. Results must be recorded after actual completion.

This is a functional model-render integration repair, not a UI style change.
No panels, styles, dependencies or third-party assets are edited. No delivered
movie is rendered again. Android is retired and excluded. This checkpoint does
not supply shared project mesh/plane/matte/effect/nested depth composition,
authored project calibration, automatic depth routing or imported camera
selection, and does not close #74 or #87. The separate parent full workspace and
calibration gates do not establish acceptance of these later source changes.

## Accepted coverage and evidence

The final product bytes pass all cold receiver cases and the shutter trajectory
reference. `model-exposure-green-v2` executes the library and 13 integration
targets. Its 13 non-model targets pass all 349 tests, while the model target
passes nine and detects one mistake in the newly authored reference: screenshot
`read_target` returns straight-alpha sRGB, so direct arithmetic averaging of its
RGBA bytes is invalid. The retained raw failure has maximum difference191.
The reference now independently decodes sRGB, premultiplies by alpha, averages,
unpremultiplies and re-encodes. It uses no product conversion helper. Its
original one-byte pixel bound, trajectory coverage assertion, authored-motion
checks and every other assertion are unchanged.

`model-exposure-green-v3` proves only that new reference's computation changed:
all production source hashes, the test setup and every assertion before/after
the reference block are identical. It reruns the entire model target:
**10 passed, 0 failed, 0 ignored**, including both new regressions and all eight
existing model tests. The earlier 349 tests passed against these exact product
bytes and are not rerun without a product change. Accepted non-overlapping
coverage is **349 + 10 = 359 tests across 14 targets**, zero unresolved failures
or ignores. This is an explicit two-stage closure, not a claim that the earlier
failed run was green. The final FFmpeg/AI-depth desktop check also passes;
source/provenance and panel-style checks already passed the same product bytes.
No existing repository assertion, tolerance, budget or timeout is relaxed.

Evidence: [raw failed and final outputs, baseline/final source and reference
correction proof](data/model-exposure-native-v3.zip), SHA-256
`865335c1341269990678d450b22127207e88284ff24adf9d5ac854a201b4a117`; 295,111 bytes,
68 entries, read back and verified. The exact failed
attempts remain distinguishable from compiled-only and accepted runs. Runtime
binaries are excluded. This checkpoint retains the mixed project composition,
calibration/routing and cross-platform limits listed above.


## Subsequent maintained-main combined acceptance

The calibration and receiver union now passes 496 targeted tests and all 6,144
actual inference/native pixel checks on source proven identical after rebasing
onto merged main `f6e7dbb2`. See [the exact combined acceptance and remaining
boundaries](depth-model-main-current.md). This does not claim acceptance of
later shared project consumer changes or completed cross-platform merge-ref CI.
