# Rational clocks with the merged tracking tool chain

Both clock/API branches now include main `b4133761`, the squash merge of #584.
Frozen combined source `6f845f1829b773d1dc3b17d65111dc68140a3ddd` passed the
FFmpeg host/desktop all-target check. No merge conflict or product correction
was required. Four Rust tests and ten real stdio controls subsequently passed;
these are distinct scopes, not fourteen Rust tests or a full desktop run.

The default-profile attempt failed while rustc generated the desktop test
program, before any test executed. Cargo exited 101; rustc exited `0xc0000005`.
The Windows event identifies rustc with an unknown faulting module. Unlike the
earlier allocation failure, this stderr has no allocation diagnostic. Available
memory recorded afterward is context, not an established cause. The original
failed controller, output and Windows events remain in the archive.

One diagnostic build changed only the desktop test package's `codegen-units`
to 4, limiting its code-generation partitioning. The
[Rust compiler documentation](https://doc.rust-lang.org/rustc/codegen-options/index.html#codegen-units)
describes that setting and LLVM parallel processing. No production build
profile, source, assertion, threshold, timeout or ignore marker changed.
Successful compilation does not diagnose or repair the default compiler AV.

| Actual execution | Result | Evidence boundary |
| --- | --- | --- |
| MCP schema and native router module | 3 passed, 0 failed/ignored | Two strict-contract tests and actual native replacement/history test |
| Windows Surface regression | 1 passed, 0 failed/ignored | Actual child process, FFmpeg decode, committed preview front, invalidation/reuse, rejected solve, undo/redo and reopen |
| Application build | Passed | Normal dev profile; test-only codegen override does not apply |
| Actual executable JSON-RPC | 10 controls passed | Edit and reopened read-only processes, 22 and 7 replies respectively |

The stdio controls verify all five advertised actions, stale/unknown input
rejection, cancellation/release, native preparation without publication, wrong
original rejection, a one-revision paired commit with stable IDs, history/save,
new-process reopen and read-only mutation denial. At the default log level,
pre-handler Info starts and the original failure appear in the file, Error also
appears on stderr, and stdout remains entirely JSON-RPC. Sources and the built
binary were checked against their frozen hashes throughout.

Actual before/after/undo Surface images were visually reviewed. The original
test additionally checks the other retained, rejected, redo and reopened
frames. This is committed preview-front capture, not physical screen scanout
or photographed tracking quality. Its authored project uses 24/1; it does not
replace the separate fractional-rate matrix.

The [receipt](data/frame-rate85-tracking-tools-v2.json) and
[archive](data/frame-rate85-tracking-tools-v2.zip) retain the original default
compiler failure, event/memory observations, successful commands and outputs,
unchanged source closure, runtime identities, native captures, stdio transcripts
and a read-back-verified member manifest. Both build controllers and the wire
controller reached terminal status and were consumed. No new third-party asset
or dependency was added.

Current-head full regression and CI remain required. The separate historical
product stack/heap failures and professional real-footage acceptance remain
unresolved. Neither this diagnostic build nor the preceding scoped passes
complete issues 85, 86, 88 or the full 3D route. Android is excluded.
