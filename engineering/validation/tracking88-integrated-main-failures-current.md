# Original local failures after integrating main (#88)

Issue #88 remains incomplete. Source
`365de8e7a4647d600af8d816850c9f944cf1a522` includes fetched main
`ac65d279c57626d7aa7e28061a1a7fdf7889c68a`. The sole textual merge conflict
was the end of NOTICE; both complete declarations were retained. There is no
panel difference against that main baseline. The integration made draft PR582
mergeable and started
[exact-source CI42601997144](https://github.com/MAD-Producer/Kilonova/actions/runs/42601997144).
That run is evaluated separately, without cancellation or retry-to-green.

## Tool preflight

The original preflight used the existing standard-library-only Python3.11
interpreter. Session22089 exited1, consumed chunk `653466`. Unittest reported
105 tests, two import errors and one existing skip. Both import errors are
`ModuleNotFoundError: yaml`; main CI explicitly installs PyYAML6.0.3. No package
or runtime was acquired, assertion weakened or skip added. No native command
started in this failed controller. Its result remains failed.

## Native workspace command

An independent native continuation reused unchanged source and the existing
pinned tools/runtime. All10 optional actual-CV controls passed. The desktop build
tool check returned0. The original full maintained-workspace command then
returned Cargo101: `rustc.exe` exited with `0xc0000005` while compiling
`motion-core`. This was a compiler-process failure before any test executable;
zero of447 metadata-expected invocations started and there are no normal test
summaries. Zero failed test assertions is not a passing regression.

Original session85743 exited1. Terminal chunk `13946e` was consumed after
rechecking all tracked source hashes and HEAD. Earlier runtime access violations
are separate failures; a compiler failure does not identify their cause.

## Independent all-target type check

The next command addressed a distinct question: whether the integrated test
sources type-check, without code generation or execution. It used
`cargo check --workspace --exclude motion-android --all-targets --locked --offline`
with the existing desktop FFmpeg, AI-depth and all maintained GPU-test features.
It was not a successful substitute for the failed complete workspace command.

Cargo again returned101. This time `rustc.exe` exited with `0xc0000005` while
emitting metadata for `ffmpeg-sys-next`. No type-check completion is claimed.
The original command, full stderr and unchanged-source hashes remain retained;
the long compiler command line is not replaced with a shortened excerpt.
Session6510 exited1; terminal chunk `52ef7f` was consumed. This observation adds
a different compiler phase and crate to the evidence. It does not prove a
source, hardware, driver, overlay or environmental root cause.

Each original controller's terminal result was consumed before its source
freeze was released. No repeated command, cleanup, foreign-process inspection,
debugger, stack/GPU/compiler override, timeout change or environment workaround
was used to obtain a passing result. The photographed tracking coverage and
error failures also remain unchanged.

## Retained evidence and limits

The [archive](data/tracking88-integrated-main-failures-v2.zip) and
[manifest](data/tracking88-integrated-main-failures-v2.json) retain all three
original controllers, complete stdout/stderr, metadata, source hashes and
consumed-session records, plus the collectors. All24 members passed full CRC
and byte-for-byte readback. Archive size922,173 bytes, SHA-256:
`92bb5cd6fd789f9b84b93c3f61c7405da4d0ce195c69c138c6e4f4370891155e`.
The archive collector exited0, consumed chunk `745d91`; it did not start a heavy
workload or create a running session. The earlier locally prepared v1 archive
is preserved, rather than overwritten to imply it included the later check.

No model/runtime binaries, photographs or third-party dataset payload are
distributed. The source observations and narrow CV/build passes do not supply
a complete native regression, a tracker quality pass or completion of #88.
