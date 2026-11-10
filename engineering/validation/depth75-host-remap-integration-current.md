# Actual host-remap cache and Session controls (#75)

Two explicit real-hardware tests exercise the new
`Wgsl::new_retained_rust_remap` path. They share the existing retained-mode
control bodies; the old entry points still select their original constructor
and assertions. The new execution mode additionally requires different plan
and runtime identities. The existing 60-second request waits, 300-second outer
test deadlines, output hashes, cache counts and rendering thresholds remain.
No existing assertion was weakened and no product behavior changed here.

## Original execution

Source was `042e518e` plus the two archived test files. Original controller
session 40138 compiled offline and ran each new opt-in control once, explicitly
using `--ignored --exact --test-threads=1`. Each passed 1, failed 0, ignored 0
and filtered 4 other independent controls. Cache and Session durations were
49.73 and 24.63 seconds respectively. The terminal exit-0 tool result was
consumed and exact source hashes verified before releasing the source freeze.

The cache control proves one actual host-remap inference, an in-instance hit,
a reopened persistent hit without another inference, an actual cold-mode miss
and a second distinct cache file. Reopening constructs a fresh retained GPU
context but completes zero native inputs. All complete depth outputs match
the original expected SHA-256. Actual identity-only resolution of the other
retained mode completes zero inputs and has distinct runtime and plan hashes.
Its failed Python remap cohort was not rerun.

Corrupting this control's own host-remap cache file returns the original
`content SHA-256 differs` cause, executes no replacement inference and records
that cause in both the default file log and console stderr. Every owned service
workspace is removed after reaping its worker. Cache corruption is deliberately
injected test data, not an unexplained test failure.

The Session control decodes the actual PNG source into the original RGB bytes,
installs real inferred depth, verifies a same-revision cache hit and applies an
authored 8 m/4 m calibration edit. The source revision change requires a second
inference and refuses stale pending depth. It freezes, serializes and reloads
the depth snapshot, drops the Session and verifies worker workspace removal,
then renders frames 0, 1 and 30 through the actual renderer. The three renders
have identical full hashes, 627 blue control pixels and 717 pixels changed
from the flat control. A changed PNG is refused with its original prepared-RGB
fingerprint cause; restoring it verifies again. Calibration values are authored
test values, not measured metric-depth quality evidence.

## Evidence and remaining boundaries

The [manifest](data/depth75-host-remap-integration-v1.json) and
[archive](data/depth75-host-remap-integration-v1.zip) contain 19 authored source,
controller, consumer, fixture, receipt and raw-log members: 26,597 bytes, SHA-256
`db86ac3f436cde187a585ff6ab03b754c143e4e7091c594ab6cf3ff8b17342b2`.
All member CRCs, lengths, hashes and bytes were read back. Models, runtimes,
executables, image/tensor/cache payloads and frozen project payloads remain
local and are not redistributed.

Both controls explicitly remain unqualified. They establish cache and frozen
render plumbing, not measured quality, concurrent whole-memory admission,
sustained thermal stability or cross-quality transition policy. The original
Python/native crashes and broad regression failures remain unresolved. Full
current-source regression and cross-platform CI remain required. No Android,
panels or UI changes are included. #75 remains incomplete.
