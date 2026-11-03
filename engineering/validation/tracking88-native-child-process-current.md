# Native subprocess evidence for tracking acceptance

The FFmpeg-enabled full desktop binary run recorded with #583 had three failing
subprocess wrappers. Windows recorded access violations of the same executable,
but the original parent output did not establish the individual child PIDs or
exit statuses. Separate passing reruns and green cross-platform CI do not erase
that failed full gate or establish a cause.

This increment changes test diagnostics only. The mask admission, text style
independent-reopen and project precision wrappers now retain child PID, start
time, elapsed time, display status, signed exit code and hexadecimal exit code.
The existing `status.success()` assertions and pixel/log/permission assertions
remain unchanged. Child arguments, environment, blocking waits and inherited
working directory are preserved. No retries, driver changes, error suppression,
new ignore annotations or product rendering behavior are introduced.

The shared capture helper explicitly uses null stdin and piped stdout/stderr,
matching `Command::output`. It saves a small receipt before the parent checks
success and includes that receipt in a failed assertion. The original captured
streams remain available to existing checks. Receipt-write failure is secondary:
it is attached to the returned diagnostic while preserving the original process
outcome. A wait IO error includes the child PID and elapsed time.

The helper control uses actual child processes with success and nonzero exit,
verifies both captured streams and persisted receipts, and blocks a receipt path
with a directory to exercise real write failure. Windows also exercises the raw
`0xc0000005` exit-code representation using deliberate normal process exit. This
is not an injected memory access violation, crash dump or proof of the cause of
the earlier native crashes.

The [source-bound receipt](data/tracking88-native-process-v2.json) and
[source/controller/log archive](data/tracking88-native-process-v2.zip) retain the
four targeted invocations and the actual precision child's saved receipt. All
four passed, with no ignored tests: the helper control and all three original
native wrappers. The final run follows a formatting-only adjustment to the new
helper; the preceding successful run remains in local artifacts. No original
success condition, pixel oracle or timing deadline was changed. New-helper
formatting and the repository provenance guard are also checked before commit.

These isolated results do not erase the prior failed full suite, demonstrate
that the crashes are fixed, or replace another complete maintained regression.
This increment cannot on its own close #88, its native full-regression gate, or
the optional real-footage tracking quality gap. It changes no panels,
dependencies, third-party assets, UI style or Android code.
