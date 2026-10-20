# macOS Intel desktop CI execution budget

## Scope and coordination

The desktop job's 60-minute cap repeatedly cancels the macOS Intel matrix entry before all validation and cache cleanup can complete. This change sets that entry to 90 minutes while retaining 60 minutes for the other desktop platforms. This is an infrastructure budget adjustment, not a product timeout, retry, test exemption, or proof that a cancelled build passed.

Before implementation, twelve open PR file lists, eighteen recent local branches and dirty intended paths across all worktrees were inspected against main `a6b382d3`. No active work overlapped this timeout change. Two historical October 10 worktrees modify the same workflow solely to add Linux `libasound2-dev`; their exact diffs were inspected and preserved untouched. The FFmpeg source-host repair belongs to a separate PR and does not overlap this patch.

## Failure evidence

These actual jobs completed with conclusion `cancelled` and the annotation `The job has exceeded the maximum execution time of 1h0m0s`:

- [Diamond bounds PR run, attempt 2](https://github.com/MAD-Producer/Kilonova/actions/runs/39272126749/job/118375433772): release build and runtime checks completed, but the job did not complete within its cap
- [Diamond bounds branch run, attempt 2](https://github.com/MAD-Producer/Kilonova/actions/runs/39272111279/job/118375439878): release compilation was cancelled
- [Desktop configuration isolation branch run, attempt 2](https://github.com/MAD-Producer/Kilonova/actions/runs/39277836117/job/118375446522): terminal cancellation and the timeout annotation were independently confirmed through the job and check-run APIs

These results do not establish a compiler or application failure. Repeating the same unchanged 60-minute job is not treated as acceptance.

## Implementation and acceptance

Only `jobs.desktop.timeout-minutes` changes in the parsed workflow. It uses the existing `matrix.os` value, preserving matrix fields and therefore check names. GitHub's [official context availability reference](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts) permits `matrix` at this job key.

- Parsed the original and modified YAML with the existing pinned PyYAML 6.0.3 and compared the complete objects after restoring this one field: equal
- Confirmed all four existing matrix entries, triggers, steps, test commands, publication gates, dependencies and permissions are retained
- Ran the existing `python -m unittest discover -s tools -p 'test_ci_*.py' -v`: 51 cases, 50 passed and 1 existing external Rust-loader validation skipped, exit 0
- Added no dependency, asset, UI change, reduced test assertion, or product behavior change

Actual macOS Intel execution under the new budget remains pending remote CI. This document records the completed local configuration validation; it does not claim remote success or justify merging a PR whose required checks have not passed.
