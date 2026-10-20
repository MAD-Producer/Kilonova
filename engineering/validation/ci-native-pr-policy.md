# Native PR validation and release policy

## Requested scope

The PR policy retains Linux, Windows, and macOS Intel native tests. Apple Silicon,
FFmpeg preparation, video checks, release packages, native GUI captures, and
desktop artifact uploads run on main/tag pushes instead. Manual dispatch retains
the full validation matrix, without publication privileges.

| Event | Workspace checks | Native desktop | FFmpeg/backend/packages | Publish |
| --- | --- | --- | --- | --- |
| Pull request, including stacked and fork PRs | Ubuntu | Ubuntu, Windows, macOS Intel | Skipped | Skipped |
| Main push | Ubuntu | All four platforms | Full chain | Current main preview only |
| Version tag push | Ubuntu | All four platforms | Full chain | Valid semantic version only |
| Manual dispatch | Ubuntu | All four platforms | Full chain | Skipped |
| Feature branch push | No workflow | No workflow | No workflow | No workflow |

There are four active PR jobs. The backend and publisher jobs remain in the
workflow with skipped status on PRs. No repository protection rules are changed.

## Preserved coverage

- Workspace checks run every supported non-desktop package. The existing native
  desktop job runs motion-desktop, motion-host, motion-ui, and motion-desktop-media
  with motion-host/diagnostics enabled. Their union still tests every workspace
  member except the already excluded Android crate. This removes duplicate
  package tests from the checks job without changing Rust assertions or timeouts.
- Every PR native platform runs cargo test and the desktop shell check. Windows
  retains its GPU output test. PR desktop steps do not prepare FFmpeg, create
  release binaries, decode videos, package, capture native GUI images, or upload
  desktop artifacts. Effect-cost evidence from workspace checks remains enabled.
- Main/tag retain both video decoding checks, all four packages, native Linux GUI
  validation, and the existing publisher's same-run, checksum, platform, runtime,
  immutable-version, and stale-preview guards. Publication stays after successful
  full validation; it is not moved to a privileged workflow_run trigger.

## Cache integrity

Pinned actions/cache restore/save actions cache libclang wheels separately from
FFmpeg downloads and the prepared prefix. Keys partition operating systems and
architectures; the runtime key includes the complete preparation recipe hash.
No broad restore key is used. Only push runs save these caches.

Restored runtimes are reused only after checking the recipe, platform, deployment
target, absolute prefix, complete file inventory, development headers, libraries,
licenses, architecture, and provenance. Absolute prefix matching prevents stale
pkg-config paths after relocation. Missing, partial, corrupt, or mismatched caches
reenter pinned preparation. Archive SHA-256 checks and both real video checks
remain mandatory. A corrupt immutable remote cache may require cache deletion or
a new recipe key to avoid repeated rebuilds; it cannot produce an unchecked hit.

## Concurrency and old branches

The existing concurrency behavior is retained and its comment corrected:
cancel-in-progress=false preserves a running push, but GitHub's default concurrency
queue still replaces an older pending run in the same group. Main previews are
latest-wins; version tags use separate ref groups. Cancelled and failed runs must
be reported separately. See [GitHub concurrency semantics](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).

A main-only change does not update a feature branch's workflow. Old branch
migration must preserve product files and current branch heads, use normal
fast-forward updates, retain open PR branches and active worktrees, and verify
the resulting push filter. No blanket rebase, force push, or cache deletion is
part of this policy change. Branch migration results are recorded separately.

## Validation evidence and limits

- Local tools suite: 65 tests, 64 passed and one existing Rust-loader conditional
  test skipped. The conditional case is run separately with an existing real
  effect_tool executable; its result is recorded in the PR.
- Route tests cover feature pushes, stacked/fork PRs, main, version tags, manual
  dispatch, publication denial, native platform selection, package-test coverage,
  cache save permissions, and video checks independent of cache hits.
- Cache tests exercise reuse without download/build, corruption of libraries,
  headers, licenses, pkg-config and manifest, missing inventory, wrong platform,
  changed recipe/deployment target, relocation, and reentry into pinned downloads.
- actionlint validates the actual workflow. Provenance, generated-icon and diff
  guards are run locally. No panel, application behavior, or Rust test is changed.
- Actual hosted routing and timing require the new PR run. Four-platform release
  execution and warm-cache savings require a subsequent main/tag run; local
  policy tests do not establish either result. No runtime speedup is claimed.
- The separate main expression-budget test failure remains outside this change.
