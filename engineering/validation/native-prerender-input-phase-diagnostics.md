# Native input phase diagnostics

Issue: [#82](https://github.com/MAD-Producer/Kilonova/issues/82)

This test-only increment diagnoses the retained strict timeline P95 failure on
`cd26ba2964569d765cafb34a89be079ac0506861`. It changes no production code, test
assertions, scenario order, sample counts, warmup, presentation or P95 criteria.
It is not a performance fix and cannot erase the previous failed acceptance.

The current retained timeline episodes have foreground P95 values of 40,209,
54,556, 38,285 and 56,987 microseconds in disabled/enabled/enabled/disabled order.
The first enabled/disabled ratio is 1.3568106642791415 and remains a failure.
Queue-wait P95 is 318 versus 298 microseconds for that pair. Much of the elapsed
time is outside the recorded serialized Engine wall time. The later disabled
episode also has substantial residual time. These observations establish neither
a CPU, scheduler nor GPU cause.

The diagnostic adds bounded timing records around native event dispatch and
pumping, action draw/apply/reply/state acceptance, and presentation draw/frame
assembly/enqueue/reply. Event dispatch is nested in pumping, so those durations
must not be summed as independent costs. Storage for 64 records is allocated
before each measured sample. Overflow increments a saturating dropped count.
Serialization and artifact writes remain outside the measured interval. No log
or file write is added to production input or Present.

## Executed evidence

The changed native test executed once on base `cd26ba29`; all 480 samples were
verified and no phase records were dropped. All 23 original assertion
expressions, scenario order, counts and strict parser were preserved.
The test itself passed; the independent original P95 assessment exited 2.
Property-drag pair ratios were `0.8681854887205127` and `1.3070832380516808`,
pooled `0.8841510358342665`. Timeline ratios were `0.9418810307385765` and
`0.8685948107504883`, pooled `0.9478755724844692`. The failing property pair
means this diagnostic is not a no-regression pass. It does not erase the
previous combined gate's timeline failure.

The per-sample attribution excludes nested event timing and never adds phase
quantiles together. In the failing property pair, the actual P95 samples took
45,727 versus 34,984 microseconds. Their action-command construction accounted
for 23,687 versus 19,455 microseconds, and presentation-command construction
for 6,957 versus 4,536 microseconds. Summed worker-queue-wait P95 was 444 versus
408 microseconds. This directs further investigation toward command generation
and surrounding work rather than assuming queue starvation alone.

Nine of 60 samples in that enabled episode had a background flight before
input; 51 had an already waiting completion. These observations do not prove
that GPU work is absent during input, explain the cause, or justify changing
admission policy without a controlled failure/fix test.

Archive: [actual phase diagnostic evidence](data/native-prerender-input-phases-v1.zip).
SHA-256: `262a01e294d0be44f045ea602754ff2ca19572b0e92c9f3e5594e94aee7438b5`. All 11 members were read back byte for byte.
The archive includes raw samples, original strict assessment, immutable assertion
proof, actual test output and bounded attribution. Diagnostic source acceptance
is separate from performance acceptance. No production code or UI changed;
PR 575 remains a draft and issue 82 remains open. Android is excluded.
