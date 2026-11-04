# Native clock readback: event-loop process isolation

The full Windows jobs for the approved clock heads `ba8e682d` and `a45bbe94`
each executed the desktop target with 1698 passes, one failure and 69 original
ignores. The single failure was `RecreationAttempt` at the new native-clock
test's event-loop construction. The original CI logs are retained, rather than
reclassified as product stack overflow or an invalid clock fixture.

The native-clock test now runs its original body in a fresh child process,
following the repository's existing native-test pattern. Its parent propagates
nonzero child status and captured diagnostics. The existing native body,
assertions, four clock/locale cases, capture waits, default GPU behavior and
document/history invariants are byte-identical. No product code or panels are
changed by this correction.

The ordered regression invokes two existing tests in one libtest process:
`attached_hidden_surface_and_factory_release_after_worker_panic` first creates
and releases the parent's native event loop; the clock test then creates its
own loop in the isolated child. Both executed and passed, with zero ignored,
in 24.19 test seconds (113.234 seconds including compilation). This checks the
ordering that the earlier standalone-only clock invocation missed.

All four fresh attached-surface GPU captures were individually inspected. Both
preview and timeline read `00:01:00:00` for 24000/1001 and 30000/1001, in English
and Chinese. The original document and history checks pass. These are native
readbacks of an intentionally empty composition, not physical scanout or
professional-footage acceptance. Existing baseline visual issues are retained.

The [receipt](data/frame-rate85-event-loop-isolation-v1.json) and
[archive](data/frame-rate85-event-loop-isolation-v1.zip) include unchanged-body
proof, exact sources, original failed CI logs, command, original output,
four PNGs, terminal review and a verified per-member manifest. No retry,
assertion relaxation, changed ignore marker or increased budget was used.

The separate [complete local attempt](frame-rate85-complete-desktop-current.md)
failed during compilation before tests, with an allocation diagnostic. Complete
desktop/UI acceptance and refreshed cross-platform CI remained pending at that
recording. The subsequent [source-bound CI receipt](frame-rate85-86-green-ci-current.md)
records the actual successful jobs and their feature limits. Earlier
native product crashes are unresolved; Android is excluded.
