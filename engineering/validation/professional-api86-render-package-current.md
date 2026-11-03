# Complete render package acceptance

The complete `motion-render` package passed on the tracked crate/manifests
closure of `73020b5cedb494da39b607747c109ad2b7a704d2`. The source hashes were unchanged throughout
the run. This is a completed package gate, not final-main or issue86 closure.

The real command was `cargo test -p motion-render --no-fail-fast --locked
--offline -- --test-threads=1`. Default mandatory GPU acceptance was enabled.
All 120 Cargo-discovered test targets and the documentation target
were matched independently against the actual Cargo invocation log. There
were 929 passed tests, 0 failures and
1 existing ignored tests. The process exited 0 after
2058.672 seconds. No filters, retries, assertion changes,
ignore edits, Android checks or second simultaneous GPU gate were used.

Approved desktop-only clock edits happened during this run. The recorded
closure freezes all tracked crate files and the root Cargo manifests, not
the entire worktree. Neither the later main251d integration nor the new
tracking-root PR580 merge is covered by this result. Fresh combined-source
regression and cross-platform CI remain required.

The previous full desktop failure and subsequent ten-test clock repair are
separately retained in `frame-rate85-panel-fixtures-current.md` and
`frame-rate85-mini-clock-current.md`. The historical native heap corruption
has not been diagnosed by this successful renderer gate. Professional
footage/matte acceptance remaining under issue86 is not implied complete.

The immutable archive is `data/professional-api86-render-package-v1.zip` with 7 members.
SHA-256: `722780bf0918ef4d78b132685c695565078126f19e606498a1f2ea3a1fe5e2ec`. Its manifest includes every raw-log, controller,
environment and source-identity report hash; every member was read back and
verified after creation. The adjacent JSON receipt records the exact count
and scope. No product code, panels, styles, assets or dependencies change in
this evidence-only increment.
