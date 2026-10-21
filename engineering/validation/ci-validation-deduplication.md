# CI validation deduplication

## Problem and scope

Feature pushes and main-targeted pull requests previously started separate full
CI runs. Their different Git refs also placed them in different concurrency
groups. Most current PRs are stacked onto other feature branches, so restricting
pushes without widening the PR trigger would remove their automatic validation.

This change reserves push validation for `main` and `v*` tags. All pull requests
are validated regardless of their base branch. Manual validation remains
available for branches without a PR. A bare feature push no longer validates
automatically; creating a PR or using workflow_dispatch is intentional policy.

Concurrency uses workflow name, event name, and full ref. Successive commits of
one PR share a group; different PRs remain isolated even when fork branches have
the same name. Manual validation cancels older manual runs for the same ref and
cannot cancel trusted push publication. PR validation cancels older runs of that
PR. Push publication retains the existing non-cancellation policy to avoid
interrupting an active preview or release upload.

The four desktop platforms, full workspace and desktop tests, native GUI checks,
timeouts, artifact uploads, publication gates, and permissions are retained.
Removing or relocating slow tests requires a separate coverage decision.

## Validation

The focused workflow policy suite initially passed all six cases. It covers
feature-push suppression, main/tag validation, stacked PR bases, manual routes,
stable PR groups across run IDs, fork branch collisions, workflow isolation, and
publication isolation. Release policy CLI and output-gate checks remain active.

Six negative controls each caused the policy suite to fail:

- Restore feature-branch push validation
- Restrict pull requests to main
- Group by head branch name
- Disable cancellation of superseded validation
- Enable cancellation of push publication
- Share manual and push groups

The actual workflow was restored byte-for-byte after each control sequence.
The initial negative-control driver stopped on a CRLF matching error after its
first mutation; its finally block restored the workflow. Normalizing only the
driver's input text allowed all six controls to complete. This tooling error is
not a product or CI failure.

Before foundation integration, the complete tools suite ran 53 tests in 34.620
seconds: 52 passed and one existing optional external-loader check was skipped.
After integrating #368, all 54 tools tests completed in 30.345 seconds: 53 passed
and the same optional check was skipped. The seventh workflow policy case keeps
#368's absolute native-artifact upload paths covered. Its merge conflict was
resolved by retaining that case and the new trigger/concurrency cases together.
No Rust assertion, timeout, ignore annotation, or return value was changed.

Local raw evidence is retained under `artifacts/pr-queue/ci-dedup-*` and
`.tools/issue-closure-review/ci-dedup-mutants.json`. These local files are not
part of the product or proof of remote CI success.

## Rollout boundaries

The branch starts from main `34d1d7ad` and incorporates the existing #368 native
validation fixes, preserving its strict assertions. The CI policy must follow
#368; its PR diff is reviewed against that foundation until it reaches main.
Neither foundation checks nor other PR checks are restarted merely to prepare
this change.

Existing queued runs keep the workflow version that created them. This patch
does not retroactively cancel or deduplicate those runs. Unchanged green PR
heads may be merged in dependency order without restarting their checks. A
branch needing another source push should first incorporate the updated
workflow once, with its actual parent integration reviewed. Cancel an old run
only after verifying its replacement head and preserving the old status. Never
record a cancelled run as successful.

PR #374 initially published head `ba95253c` against the feature-branch foundation.
The Actions API reported exactly one run for that head: `39376687346`, event
`pull_request`, initially queued. No push run was reported. This verifies the
real stacked-PR trigger entry; it is not evidence that its six jobs passed or
that superseded-run cancellation has yet completed. The final documentation
update clarifies migration without changing the validated workflow or tests.

Remote CI, first real PR-event routing, branch-by-branch migration, and final
Chinese squash merges remain separate acceptance steps. No fixed queue-drain
time or job-minute reduction has been measured by the local policy tests.

References: [GitHub concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency),
[GitHub PR events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).

## Integration after foundation merges

PR #368 and the independently validated grapheme PR #367 are now on main at
`2cb69d019e447328a8e8d926d8407afa8a83d074`. This branch merges that actual main
commit. The only conflict was the workflow test method name: the deduplication
name and full assertions were retained byte-for-byte. The workflow and its
test file are unchanged from the previous reviewed head. The incoming six
files are exactly the already merged grapheme change; they do not expand this
PR's diff against main.

All 54 tooling tests completed again: 53 passed and one existing optional
external-loader check skipped. Provenance and patch-whitespace checks passed.
Evidence: `artifacts/pr-queue/374-final-main-integration-tools.log`. Git transport
failed with a connection reset; the two GitHub-signed main commit objects were
obtained using the official Git API, reconstructed from their verification
payloads and signatures, and accepted only after their exact SHA-1 hashes and
existing local tree objects matched. This is source integration, not a CI rerun
of an unchanged head. The updated head still requires remote CI.

## Windows native-fixture failure on the policy branch

Run `39380256220`, Windows job `118487905609`, failed its desktop unit step:
611 passed, one failed and one existing ignore. The PNG fixture reported
`Present` waiting for 10.030 seconds, queue depth zero, on DX12 Microsoft Basic
Render Driver. The raw job log is retained as
`artifacts/pr-queue/374-windows-current-failure.log`. This is a real failed
validation, not a successful policy rollout.

The hidden-window fixture blocked its window-owner thread on a channel while
the render worker could synchronously send native messages to that thread.
The test-only receive helper from #348 services the native event loop while
preserving one absolute deadline. Both hidden-window fixtures use it for
Attach and Present. The original 30-second Attach, 10-second Present and
15-second playback deadlines, strict adjacent-frame progress assertions, and
all sampling modes and frame counts remain unchanged. Production event
handling, rendering and UI behavior are unchanged.

A real cross-thread Win32 WM_NULL control verifies that an ordinary blocking
receive does not service the message, and that the helper does. It also checks
timeout, disconnect and immediately available replies. The helper and the
three other test-source files are byte-identical to #348's validated source;
placing this shared fix here prevents the policy PR from depending on a
downstream PR's fixture correction. Local Vulkan validation cannot establish
success on the remote DX12 software adapter; that remains a separate CI gate.

The first local attempt on this branch did not execute a test: MSVC failed
with LNK1108 because its temporary file was on a full C drive. That log is
retained as `artifacts/pr-queue/374-message-control.log`. The next attempt
places both Cargo output and TEMP/TMP on E. No assertion or timeout was
changed to address this storage failure.

The E-only retry also stopped before test execution: Rust could not map
dependency metadata (Windows error 1455, paging file too small), and its
PowerShell host reported out of memory. A separate release build was active
in another worktree; it was not terminated or modified. The failed retry is
retained as `artifacts/pr-queue/374-e-temp-message-control.log` and is not
counted as a source regression or a passing test.

The identical four test-source files already passed actual Cargo validation
on #348: the native message control (0.27 seconds), bilingual PNG fixture
(19.02 seconds), and four-mode paced sampling fixture (40.20 seconds). Evidence
is `348-final-message-control.log`, `348-pumped-png.log`, and
`348-pumped-sampling.log` under `artifacts/pr-queue`. An independently compiled
copy with message pumping replaced by sleeping failed the positive message
control as expected; the actual helper passed. These are existing source-level
checks from #348, not a fresh complete run of #374. This branch still requires
its own replacement-head remote checks, including Windows DX12.

After the fixture integration, all 54 tooling tests completed in 53.908 seconds:
53 passed and the existing optional external-loader check skipped. Provenance,
panel style-consumer and patch-whitespace guards passed. The workflow and its
policy tests were not modified by this fixture correction.
