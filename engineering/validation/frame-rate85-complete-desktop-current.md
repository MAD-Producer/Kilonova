# Integrated desktop/UI regression: compiler failure

The approved readout fixture passed its unchanged original regression, including
two fresh default-log hover processes. That scoped pass remains valid. It does
not constitute complete package acceptance.

The full desktop/UI attempt used `a45bbe94`, including main `77f61452`. The
all-target check passed in 103.422 seconds. The following unfiltered, sequential
`cargo test -p motion-desktop -p motion-ui --no-fail-fast --locked --offline --
--test-threads=1` did not reach a test executable. While building the desktop
test program, rustc reported `memory allocation of 34603024 bytes failed` and
terminated with the reported `0xc0000409, STATUS_STACK_BUFFER_OVERRUN` status.
The allocation diagnostic is the available evidence; the status name alone
does not establish a stack-buffer bug in the product.

The controller and cargo processes were absent on resumption, and original
session 21238 was no longer accessible. Its final controller/cargo return code
was not recovered. The original running receipt is preserved unchanged; the
additive review records this observation explicitly. No test counts are inferred
from metadata, and no previous test executable is relabelled as this build.

The [receipt](data/frame-rate85-desktop-compiler-v1.json) and
[archive](data/frame-rate85-desktop-compiler-v1.zip) retain original raw logs,
commands, environment, source hashes, target inventory and a verified manifest.
All recorded source hashes still match. No retry or modification of assertions,
ignore markers, budgets or stack size was used to turn the failure green.

Full desktop/UI acceptance remains pending, as do final CI and the separately
retained earlier native product stack-overflow and heap-corruption failures.
This report does not diagnose or resolve those failures. Android is excluded.
