# Issue 86: integrated model/core regression after cold preparation

Status: scoped model/core and compilation gates passed on `7f40d752`.
Final integrated desktop/render runtime and cross-platform CI are pending.

The child branch merged the ten authorized panel clock changes and the
cold-expression preparation fix only after original gate 61610 had exited and
its terminal failure had been consumed. The complete motion-model and
motion-core package tests then passed: 880 passed, zero failed, one original
ignore, across 140 actual summaries including documentation targets. Elapsed
time was 526.875 seconds including compilation. Desktop cargo check covered
all test targets and passed in 45.391 seconds; the real build helper check
passed in 1.031 seconds. The entire recorded source closure stayed unchanged.

Existing assertions, expected values, budgets and ignore markers remain
unchanged. No test filter, retry, GPU backend substitution, Android device
claim or new timeout was used. The real cold-compilation before/after failure
proof and the 81 earlier expression tests remain separate records; they are
not added to this complete 880-test count.

The subsequent merge adds only the separately verified native readout test and
its module registration, plus documentation/evidence. Core implementation bytes
match this run; this historical result still does not claim final whole-source
desktop execution. Original precision-child stack overflow, the graph-readout
fixture authorization gap and the older professional native heap failure stay
open. A diagnostic precision pass does not resolve those failures.

The adjacent ZIP retains the exact command controller, E-drive environment
wrapper, raw stdout/stderr, source hashes, implementation snapshots and a
read-back checked per-member manifest. The receipt binds counts and timings to
the tested source. No issue-completion or whole 3D-route completion is claimed.
