# Issue 88: paired publication on current main

Status: scoped integration passed on `b5d73deb31474240b95cf169a8971ee9fb677760`,
including main `c0e25eda`. Final CI is required before merging PR 583. Its older
five green checks used a different base and are retained as historical evidence.

Main integration preserved original-video identity and mapped-time validation,
alongside the atomic fresh-plate/fresh-solve operation and linked audio handling.
Three conflicted files kept the feature additions and every non-conflicting
main change. The first resolver wrote the Chinese media labels using Windows
CP936. Compilation rejected the invalid UTF-8. The repair reconstructs the
exact same conflict choices from retained original bytes, explicitly writes
UTF-8 and verifies the exact inverse encoding relationship. The original
failed output is retained; assertions and functional branches are unchanged.

The complete host/desktop all-target FFmpeg compilation check passed in 145.187
seconds. Compilation of the actual desktop FFmpeg unit binary passed in
174.375 seconds. Twenty distinct regressions passed, with zero failures and
zero ignores:

- Three video-candidate identity/audio controls
- Eight atomic edit, binding, history and mapped-clock controls
- Five actual host publication, rollback and acknowledgement controls
- Four native decoder/GPU tests covering original identity, mapped source time,
  fresh plate and solve, and coherent linked audio; their assertions exercise
  rollback, undo/redo and reopening

The four native cases executed sequentially after the complete renderer session
63700 had exited zero and its terminal had been consumed. They used actual
source fixtures, native decoding and GPU captures. No concurrent renderer,
assertion change, threshold relaxation, ignored test or retry was introduced.
The source closure stayed unchanged across these gates.

One supplemental host selector matched zero tests. That invocation remains in
the raw report and contributes no coverage. A separate corrected selector
first listed all five intended test names and then executed those five tests.
The [receipt](data/tracking88-pair-current-main-v1.json) makes this distinction;
the [archive](data/tracking88-pair-current-main-v1.zip) includes original output,
source identities, environment wrappers, repair evidence and a verified
per-member manifest. The original native pair gate session 91866 and supplemental
session 19322 both terminated successfully and were consumed.

This is scoped integration, not a complete workspace run or acceptance of the
separate point/plane tracking prototype. Actual photographed-plane quality
remains unresolved. No panel presentation, dependency or third-party asset
changed. Android is outside the maintained acceptance scope.
