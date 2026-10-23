# Glyph and mixer CPU memory observation acceptance

Issue: #60, partial implementation. The full issue remains open.

## Delivered behavior

The explicit Session `resource_memory` query now includes cached font glyph
coverage, audio input/resampling scratch and shared sinc tables. It reports Vec
payload separately from actual backing capacity. Arc-identical glyphs/tables
count once; independent equal-content allocations remain separate. The query
does not read/seek PCM or font files, touch glyph LRU, rasterize, mix audio,
initialize a GPU, edit the project or change undo history.

Uninitialized stores/mixers are null. Empty payloads can retain nonzero capacity;
dropping the Session owner removes that observed capacity. An externally held
glyph can outlive eviction or store release, so it remains an explicit exclusion
and is not mislabeled as freed process RAM.

## Actual acceptance

The initial lower-media source was
`9b022723d8c069fff39f09870efe07db555de48f`: 146 passed, zero failed,
two pre-existing large-source ignores. Integrated source
`8f6fe8be071c54fdf76001b94d5f2e4094b3cece` ran the complete, unfiltered command:

```text
cargo test --locked --offline -p motion-media -p motion-host -j1 -- --test-threads=1
```

Result: **279 passed, zero failed, 2 pre-existing ignored**.
All six new lower-media regressions and the real Session integration regression
executed. Existing CPU cache/public-bridge tests also ran. Provenance and diff
checks passed. No test assertion or ignore was relaxed or added.

The two existing ignored tests require large source media; this increment does
not claim to accept those paths. The real mixer fixtures use authored f32 PCM
cache files, not WAV/codec decoding. The font fixture uses the already attributed
built-in IBM Plex Mono; no new asset or dependency was added.

| Item | Evidence and disposition |
| --- | --- |
| R60.1/2 font coverage | Actual built-in glyph caching, spare capacity, alias/distinct allocations, no IO/LRU/retention mutation and 2048-entry eviction tests passed |
| R60.1/2 audio scratch/tables | Actual same/different-rate PCM sources prove shared table dedup; clear/release tests distinguish payload from backing capacity |
| R60.1/2 Session aggregation | Real stores and mixer included through plugin JSON; exact totals, unchanged project/revisions and no GPU initialization asserted |
| Observation behavior | Independent control mixer and queried mixer produce byte-identical repeated PCM blocks; resource query preserves cursors |
| Failure/rework | Original integrated source1cf1f381 failed compilation with E0603 in the new test's private module import; source8f6fe8be uses the existing public re-export, assertions unchanged; original failed receipt/log retained |
| UI | No panels, layout, style or interaction files changed |

Tests used one compiler, independent E: target, E: temporary files, debug symbols
and incremental disabled in the recorded test environment. These are correctness
checks, not release timing or optimization comparisons. The integrated source is
a private merge of our own #441 and lower-media implementation for verification;
publication code source `c1dc2f3977c0e5445af23b49679246447416d7d2` is rebased onto main
`c8504d15b91ba8118ddc5aba8e65e264b4d4df0d`, which contains merged #441. All twelve changed
implementation, test and API-document files are byte-identical to the accepted
native source; the manifest records their SHA-256 values. Provenance and diff
checks were also rerun after rebase. The 279-pass local suite remains attributed
to its actual source8f, rather than relabeled as the rebased source. Current-head
CI is pending at publication and must pass before merge; no second local native
suite after rebase is claimed.

## Reproducible evidence

[Machine-readable receipts](media-cpu-memory-acceptance.json) include exact source,
commands, environment, terminal exits, counts and original log hashes.
[Original logs and receipts](data/media-cpu-memory-8f6fe8be.zip) preserve the first media run,
the failed integrated compile and the successful integrated run. Archive SHA-256:
`bad733a0282d811aec3c1be66d08a2c5c17d897cad4b7abb795a1ca5682438ea`. Its internal inventory binds every original
file; the archive contains no binaries or third-party media.

## Remaining #60 boundaries

Font parser internals, external glyph/raster owners, OS file-backed PCM residency,
project/voice/container overhead, image decode workers, original-source preview
inputs, effect package memory, inference weights/activations and native decoder/
driver residency remain unmeasured. Known CPU capacities are not process RSS or
GPU VRAM. Existing texture capacity and CPU subtotals stay separate and
`completeCoverage=false` remains explicit. These tests do not complete real file
decode/window playback, pressure, physical Adreno/Mali or optimization comparisons.
