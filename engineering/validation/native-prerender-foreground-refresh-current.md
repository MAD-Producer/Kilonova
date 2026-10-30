# Foreground refresh before optional native work (#82)

Status: the native admission and pixel control passes. This is a component
checkpoint, not completion of #82 or #87.

An acknowledged edit can leave the Engine queue empty while the UI builds its
next Present. With zero idle delay, the background owner previously started
rendering the new revision before that foreground redraw. The runtime now holds
new admission as `foreground_refresh` until a successful foreground presentation
acknowledges the authored revision. Reconciliation, obsolete-result rejection,
cancellation polling and lease acknowledgment still run before this gate. An
initial session without a presented frame may warm; playback advancing time alone
does not change the authored revision.

## Actual controls

The source was rebased onto main `ab779cee3c7f61f46ae8dad7b4b5e331b7298ceb`.
The renderer conflict preserves both viewer snapshot additional usages and
preview-cache `COPY_DST`. Three existing prerender test fixtures now provide the
new default viewer-inspection field; their assertions are unchanged.

On rebased head `2d2832da277e3da39f52e2914016697faa430c71`, with the new test but
without the admission fix, the actual native child and parent each failed 0/1/0.
After the real color edit, two replacement frames warmed while Present was
deliberately withheld: revision 1, started 4, resident frames 2, no runtime fault.
The failure was the `await foreground_refresh` deadline, not a compiler error or
native crash. The first compile attempt missed the three new fixture fields and
did not execute. The subsequent collector initially misclassified duplicated
child/parent summaries; separate retained outputs verified the genuine negative
control without rerunning it. These unsuccessful collection records are retained.

With the fix, the **same test** actually passes 1/0/0 with default logging:

- Warm the original red revision: started 2
- Commit green without Present: old cache cleared; started remains 2
- Remain idle for 200 ms: hold remains active and started remains 2
- Read the retained foreground: all 256 pixels remain opaque red
- Successfully Present: all pixels become opaque green without a stale cache hit
- Resume optional work: started becomes 4; a subsequent warm frame is green and
  increments actual cached presentation consumption

Run the explicit Windows native test with `--ignored --exact --test-threads=1`:

```text
engine::prerender::tests::foreground_refresh::native_edit_waits_for_foreground_refresh_before_warming_its_revision
```

No panels, styles, hit areas, user interaction or existing assertions changed.
Tool inventory and clean-room provenance checks passed on the rebased source.

## Evidence and remaining gates

Archive: `data/native-prerender-foreground-refresh-v1.zip`.
SHA-256: `3714ba8118900bb9c18cca9b535404fc50d113db6f0e688fd8806dadf39d20a1`.
It contains source hashes, separate negative results, fixed execution receipts,
default-level worker logs and actual retained-front pixels. It excludes binaries,
PDBs and private native-crash forensic material.

This does not establish P95 nonregression, automatic same-path source freshness,
the full workspace gate or current-source cross-platform CI. Earlier input heap
corruption and full-regression access violations remain unresolved. Original-binary
diagnostics with and without implicit Vulkan overlays both completed execution;
neither reproduced the fault and neither replaces original acceptance. Android
is retired and excluded. No disk cache was added or claimed as validated.
