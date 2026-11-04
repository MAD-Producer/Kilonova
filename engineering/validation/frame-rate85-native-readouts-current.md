# Issue 85: native rational-clock readouts

Status: native clock readback and visual review passed. Complete integrated
desktop/render regression and final cross-platform CI remain pending.

The actual Shell, engine worker, attached native GPU surface, text atlas and
UI capture path render four cases: 30000/1001 at frame 1800.75 and 24000/1001
at frame 1440.75, each in English and Chinese. One new executable regression
passes. Each image is 1280 by 900 and contains over 400 distinct colors.
The saved project, fractional frame, revision and undo/redo state stay unchanged.
Both preview and timeline visibly read `00:01:00:00` in every image; project
metadata retains the exact fraction. These four images were individually
inspected, not inferred from successful compilation or a presentation counter.

The earlier OS ImageGrab attempt completed its transport/state checks, but all
four images contained exactly two black/white colors. Visual review rejected
them. Those original images and their original report remain unchanged in the
archive. Native GPU readback proves the clock chrome independently; it does not
identify or fix the OS capture/compositor cause. Empty projects do not validate
media decoding, precision conversion, timing menus or professional footage.

The archive separately retains the complete historical combined gate on
`162edfed`: render 957 passed, zero failed, one original ignore; desktop 1697
passed, one failed, 69 original ignores across all seven test targets. The
precision-menu child reported a motion-engine stack overflow. The original
controller exited 1 and was consumed; helper-check was not reached. The
later ten panel changes and cold-compilation fix were absent from that run.

A single debugger run of the exact preserved failed EXE later passed its
precision test without reproducing an exception. No stack limit, assertion,
implicit-layer setting or retry loop changed. This diagnostic pass does not
resolve the original stack overflow. Offline owned-PE metadata shows large
static-frame candidates; approximate public symbols do not prove a faulting
callstack. No original crash dump was captured. The older native heap failure
also remains unresolved.

The parent Ubuntu CI failure at `9ea85107` independently confirms the known
graph-readout fixture's invalid numeric clock. Its separate proposed fixture
change was unapplied pending authorization at that point. The subsequent
[authorized fixture repair](frame-rate85-readout-fixture-current.md) passes
both fresh-process checks with the original assertions. No existing assertion, expected
value, budget or ignore marker was altered to obtain these results.

DESIGN section 0.7 baseline rows remain unchanged: only arithmetic and separate
acceptance code were added. No product layout, style, dimensions, hit areas,
assets, dependencies or Android acceptance changed. This record does not close
issue 85, issue 86 or the 3D route. The adjacent JSON receipt and ZIP include raw
outputs, controllers, source snapshots, individual images and a read-back
verified SHA-256 manifest.
