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
does not retroactively cancel or deduplicate those runs. After it is merged,
each stacked branch needs the updated workflow once, with its actual parent
integration reviewed. Cancel an old run only after verifying its replacement
head and preserving the old status. Never record a cancelled run as successful.

Remote CI, first real PR-event routing, branch-by-branch migration, and final
Chinese squash merges remain separate acceptance steps. No fixed queue-drain
time or job-minute reduction has been measured by the local policy tests.

References: [GitHub concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency),
[GitHub PR events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).
