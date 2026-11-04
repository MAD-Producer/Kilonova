# Rational project clocks after tracking integration

Main `acaab00b74df3f61b278408050040d19133074be` includes the independently delivered tracking root
PR580. Integrating it with the rational project clock exposed three real
compile errors: point, plane and calibrated-camera sampling still passed
the project FPS object as an integer numerator with denominator 1. The first
combined-source check exited 101; its frozen source report and original
compiler output are preserved in this archive rather than replaced.

All three sampling boundaries now pass the canonical project numerator and
denominator to the existing exact source-PTS clock. No rounded decimal FPS,
whole-frame conversion, source timestamp rewriting or numeric fallback is
introduced. Existing missing-track, visibility, confidence and gap semantics
remain unchanged. The merge keeps both FrameRate and tracking-camera error
variants. The combined current-only project schema is version 37: main was
36, while the unmerged rational branch was 33. No migration is added.

Three new tests sample a physical 1/48 second at 30000/1001, 60000/1001,
2997/100 and 24/1. Independent point/plane midpoint coordinates and calibrated
camera projection verify the original negative-origin VFR source timestamps.
All three complete integration targets passed: camera 8, plane 10, point 10,
total 28 passed, zero failed or ignored. Existing assertions and budgets were
not edited. A desktop cargo check passed after the three production changes.

These shell commands exited zero as observed. The successful check preceded
the three test-only additions; exact changed source files and raw output are
included. Fresh clean-commit model/core, renderer, desktop and cross-platform
acceptance remain pending. The preceding 929-test render gate predates this
integration and is not presented as validation of these combined sources.
Historical native heap corruption is still unresolved. Android remains outside
the maintained scope. No panels or visual behavior change in this increment.

Archive: `data/frame-rate85-tracking-integration-v1.zip` (16 members), SHA-256
`ffd44ff28a8fc7ba5b6177a19a3a7dc47fcbd2525a681bb1f4da21942fd20cd0`. Every archived member was read back and hash-verified.
