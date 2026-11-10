# Exact published Rust-remap/cache source: terminal CI

Draft PR #651 source `dc17b0071e79e5485882c8cce5f0be2fe8e50757` completed
[run42582989141](https://github.com/MAD-Producer/Kilonova/actions/runs/42582989141)
successfully: eight applicable jobs passed and two policy skips. The successful
jobs comprise dependency audit, Rust/software-Vulkan controls, and native desktop
tests and builds on Ubuntu, Windows and macOS Intel. The skipped jobs are the
libav job and release publication. No rerun or cancellation was dispatched by
this evidence collector.

The complete raw job logs verify the six newly added Rust-remap math, extent,
digest, cancellation, publication and identity controls, and the explicit host
memory-accounting control, exactly once in each of the three native desktop
test logs. These are named observations within the existing test totals, not
21 additional tests or a second execution.

The [archive](data/depth75-ci-dc17b007-v1.zip) and
[manifest](data/depth75-ci-dc17b007-v1.json) retain the terminal run/jobs metadata,
all eight applicable raw job logs, download stderr, collector and named-control
verification. Archive size591,969 bytes, SHA-256
`c341916373bb297ccaf9c1e0ab89dc39d16bb3eb25323db5f9606aa3cd8c0a95`.
Every member passed complete CRC, byte-length and SHA-256 readback. Original
collector session87048 exited0 and its terminal result was consumed.

This CI source precedes the later one-service sustained example. Its tests do
not install the external model/runtime or run opt-in learned-model cache/Session
experiments. CI cannot qualify an application domain, thermal behavior, memory
budget or production quality tier.

The local [full workspace](depth75-host-remap-full-regression-current.md) at this
same commit remains failed with its original four failed targets. In particular,
the Windows desktop matrix is not a Windows run of every `motion-render` and
`motion-graph` integration target: the general workspace GPU job runs on Linux.
Consequently native CI success does not erase local access violations or prove
their root cause. All original failures, assertions and deadlines remain intact.
