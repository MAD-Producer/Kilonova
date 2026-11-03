# #85/#86: approved fixture corrections and desktop failure evidence

The five user-approved integer initializers now construct `FrameRate` values.
This removes the previous type errors; it does **not** complete desktop clock
adoption. The full desktop package still fails eight existing mini-curve
readout tests. PRs #597 and #599 remain draft and issues #85/#86 remain open.

## Source and authorized change

Parent commit `fd99f3c866935516e524033c28823c078be17c81` changes exactly five
initializers in four panel test files: `audio/tests.rs` (two),
`auxiliary/render_queue/tests.rs`, `layer_sampling/tests.rs`, and
`project/tests.rs` (one each). The original 30/60 values, assertions, limits,
ignore markers, production logic, interaction and presentation are unchanged.
DESIGN.md section 0.7 visual remediation rows are retained because this increment
only changes test setup; none is represented as a visual fix.

The executed source is **integrated child** commit
`673f9f061c08ae32fb3bfd735f6bb02de2fe1425`, not the standalone parent head.
The controller records and rechecks all Rust/WGSL/Python/workflow/manifest and
Cargo/lock hashes. Its original process exited 1 after finishing all five
phases, with `sourceUnchanged=true`. No phase was restarted on observation
timeout, no assertions were relaxed, and no ignored tests were enabled or added.

## Actually executed gates

| Command/phase | Result | Seconds |
| --- | --- | ---: |
| `cargo check -p motion-desktop --locked --offline` | Pass | 2.313 |
| `cargo test -p motion-desktop --no-fail-fast --locked --offline -- --test-threads=1` | **Fail**, exit 101 | 2907.171 |
| `python tools/build_desktop.py --task check --debug --target-dir E:/Dev/aem/target/rational-rate85-contract -- --offline` | Pass | 18.328 |
| `cargo build -p motion-desktop --locked --offline` | Pass | 5.969 |
| `python tools/ci_desktop.py --binary E:/Dev/aem/target/rational-rate85-contract/debug/motion-studio.exe --output <gate>/native-ui` | Pass | 615.875 |

This is Windows, the default GPU build, **without** the FFmpeg feature. Build,
temporary files and isolated LOCALAPPDATA reside on E. Android is retired and
excluded. Neither these results nor earlier backend subsets constitute full
maintained-workspace or final-source cross-platform acceptance.

The binary ran 1,723 tests: **1,647 passed, 8 failed, 68 existing ignores**.
All five integration targets ran and passed: 4 + 2 + 2 + 3 + 2 = 13 tests.
Thus the full package has **1,660 passed, 8 failed, 68 ignored**. Two failure
summaries printed by nested subprocesses are retained verbatim but are not
counted again as independent package failures.

## Shared cause and remaining fix

Both Ubuntu jobs on the same published heads also fail eight tests: 1,601
passed, 8 failed, 4 existing ignores in their desktop binaries. Original logs,
run/job identities and hashes are included in the archive. They are not
relabelled as results from a later patched source.

`panels/animation_controls/mini.rs` still reads `project.fps` with `as_f64()`.
Healthy snapshots now carry the authoritative numerator/denominator object, so
the consumer emits `curve.mini.readout_fps` and returns an unavailable readout.
This breaks the existing mapped/spatial/physical-unit and healthy-quiet tests.
Separately, `graph_ranges::Scope.geometry` reads fps as `u64`; rational rates
all become `None`, losing rate identity for retained graph ranges.

A separately prepared exception proposes strict clock consumption, a normalized
rate cache key and two focused regressions. It also proposes replacing the
sample-overflow fault input's obsolete numeric `fps=1e308` with a valid clock
and finite authored values whose percent conversion overflows. The original
sampling failure assertions, original cause, escalation and suppression checks
remain intact. That proposal is **not applied or executed** by this increment;
the latest user approval covered only the five initializers above. Other panel
clock readers, rational timecode behavior and their actual UI acceptance remain
separate pending work.

## Native smoke and evidence boundary

The actual rebuilt executable passed the existing desktop acceptance script:
both UI languages, styled floating windows, live node rendering and the shared
MCP session. The generated home and composition-settings captures were visually
inspected. This demonstrates successful startup and those existing flows; it
does not exercise or repair the failed mini-curve readout or establish all
fractional-rate displays and interactions.

The archive retains raw phase logs/reports, validated fixture source, controller
and environment scripts, the approved patch, Ubuntu failure logs, selected
actual screenshots, and a complete size/SHA inventory of generated PNGs. All
other images remain locally retained under the original gate directory. They
are not duplicated into Git solely to increase screenshot count.

Archive: `data/frame-rate85-panel-fixtures-v1.zip`, 112 members, 1,784,413 bytes,
SHA-256 `28b7c51a6028a057aba046cefb815ec0679364501d458ef43849031b21161053`.
Its JSON receipt and every included member were read back and hash checked.
Status: `desktop_gate_failed_original_results_retained`.

## Earlier failures are preserved

The first gate's compiler process exited `0xc0000005` in `rustc_driver.dll`
before any desktop test ran. Its later build passed; its native startup then
failed with PermissionDenied before screenshots, with the exact filesystem
operation unproven. The second gate isolates LOCALAPPDATA instead of changing
ACLs, deleting user logs or altering existing user configuration. Passing
subsequent builds/startup is not a compiler or filesystem-root-cause fix.

The separately observed professional-screen native heap corruption
`0xc0000374` also remains unresolved. This fixture gate neither reproduced nor
diagnosed that original corruption and cannot close its stability boundary.
