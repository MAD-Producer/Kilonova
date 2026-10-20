# Desktop unit-test preferences isolation

## Scope and coordination

This batch addresses issue #135, section VII: `Shell::new` unit tests reading real desktop preferences. The baseline is main `92a2327`, after PR #346. Local worktrees, recent branch heads and open PR files were checked before starting. PR #344 owns layer render sampling; #345 owns text bounds; #347 owns borrowed input; #348 owns diamond bounds; #349 owns U/UU property filtering. This batch changes none of those implementations.

The previous constructor always selected `MOTION_UI_LAYOUT` or the platform data directory, then loaded that file. Even tests with a temporary project root could read the developer's layout, favorites, visibility preferences and saved stylesheet path. An unspecified locale could also inherit `MOTION_LOCALE` or the OS language.

## Implementation and boundary

The boundary is the desktop unit-test build (`cfg(test)`). Each test-created Shell owns a distinct temporary preferences directory for its whole lifetime; dropping the Shell removes that directory. An unspecified test locale uses English, while explicitly requested English/Chinese locales remain supported. The directory owner is the last struct field, so the other fields drop before cleanup. Normal application builds retain the existing environment and platform preferences selection.

The work does not isolate every environment variable, standalone `Workspace::default`/`Locale::default` call, system font, GPU adapter, or native audio device. It does not add dependencies or assets. It does not alter panel code, styling, shortcuts, project files, MCP contracts, logging, thread timeouts or restart behavior.

## Regression design

The regression executes the actual Shell constructor in child test processes. Both explicit `MOTION_UI_LAYOUT` and implicit platform-data paths point to valid, controlled saved preferences, with distinctive favorites, disabled timeline columns and a Chinese locale. The child also receives `MOTION_LOCALE=zh`. No process-global environment variables are mutated in the parent test runner, and no actual user preferences are touched.

Assertions require default preferences and English when the Shell locale is unspecified, independent paths for two simultaneously alive Shells, actual save/load round trips inside the owned directory, directory lifetime through each Shell's drop, an explicit Chinese locale, and byte-identical external fixture files after both runs. The fixtures are loaded and checked independently before starting the children, so invalid external JSON cannot make the isolation test pass accidentally.

## Verification

Against unchanged baseline production code, the regression failed in both child processes: the default platform path and the explicit layout override each loaded the external sentinel favorites. The parent test failed with exit code 101. Both controlled fixture files remained unchanged. Evidence: `artifacts/issue-135-config-before.log`.

With the isolation change, the same regression and assertions pass: **1 passed, 0 failed, 0 ignored**, in 16.51 seconds. Both child modes execute all constructor, persistence and lifetime assertions. Evidence: `artifacts/issue-135-config-after.log`.

The complete desktop suite passes: **594 passed, 0 failed, 1 existing native-audio test ignored**, in 345.17 seconds, with exit code 0. No existing assertion or ignore was modified. Evidence: `artifacts/issue-135-config-desktop.log`.

The normal application builds successfully without the test-only field or constructor branch. Evidence: `artifacts/issue-135-config-build.log`. Three actual hidden-window smoke runs then pass the existing design-screen verifier, present frames and return the expected locale:

| Normal application input | Expected and observed locale |
| --- | --- |
| Explicit layout override with Chinese preferences, environment locale English | Chinese |
| Default platform-data path with Chinese preferences, environment locale English | Chinese |
| Explicit Chinese layout plus command-line `--locale en` | English |

All runs leave stdout empty and produce existing default-Info `start #` records in their controlled file-log directories with `MOTION_LOG` unset. This preserves existing logging behavior; it is not a new observation added by this patch. Reports, screenshots and per-run stdout/stderr are under `artifacts/issue-135-config/native/`; `results.json` lists the cases. Runner evidence: `artifacts/issue-135-config-native.log`. The native fixture is independently authored JSON containing a valid dock with panel IDs 1 through 4 and `locale=zh`; smoke mode then uses its existing standard workspace reset. These runs verify normal-build configuration precedence and existing smoke checks, not arbitrary saved-layout geometry or physical-device input.

Local execution is on Windows. The child-process regression has Windows, macOS and Linux default-path fixtures; macOS/Linux execution remains for CI. Cargo commands use portable MSVC, `--locked`, `-j 1`, single-threaded test execution, and the dedicated target `E:/Dev/aem/target/observability-config-validation`.

```text
cargo test -p motion-desktop --locked -j 1 configuration_tests::shell_preferences_ignore_external_configuration -- --exact --nocapture --test-threads=1
cargo test -p motion-desktop --locked -j 1 -- --test-threads=1
cargo build -p motion-desktop --locked -j 1
```

The panel style guard, provenance guard, new-test formatting and staged whitespace checks pass. The panels diff is empty. No dependency or third-party asset is added.

## DESIGN.md disposition and remaining work

All twelve section 0.7 baseline rows are retained with reason: there is no panel or presentation change. Path row steps, sizing, control heights, numeric backgrounds, boolean presentation, effect row steps and controls, hover/hierarchy, timeline gutter and snap-guide styling are outside this batch. No whole-UI compliance claim or new screenshot is implied by a test-configuration change.

Issue #135 remains open. Existing dependency-boundary and presentation work keeps its own evidence. Expression sandbox reachability, additional event/model/device-health coverage and GPU execution policy are not completed by this patch.
