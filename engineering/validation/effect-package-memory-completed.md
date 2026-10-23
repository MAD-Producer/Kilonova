# Effect package and retained shader CPU observation acceptance

Issue: #60, partial implementation. The issue remains open.

## Delivered observations

The explicit Session `resource_memory` query adds `cpu.effectPackages` and
`cpu.effectProgramShaders`. One borrowed enumeration per domain spans the
Session builder, preview renderer and editor renderer. Actual Arc identity
deduplicates owners; equal content in independent allocations counts separately.
No pointer, shader source, retained owner or unbounded context is returned.

Compressed archive Vecs, expanded file Vecs, package-owned WGSL/GLSL Strings
and independent retained program String copies contribute their actual lengths
and capacities separately. Package entries retain registry/program references;
program entries retain bounded contexts. Dropping one observed owner does not
claim to free allocations still owned by another builder. Global or escaped
owners outside this query remain excluded, rather than being reported as freed
process RAM. CPU capacity, GPU texture capacity and logical admission stay
separate; unknown process/native-driver allocations remain null.

Observation maps retain at most 1024 package identities, 4096 program shader
identities, eight contexts per shader and 160 characters per context. Unobserved
references and omitted contexts are counted; a new query starts fresh. These
queries do not load files, compile, submit GPU work, initialize graphics, modify
project/history/budgets, or run during ordinary snapshots/frame preparation.

## Actual acceptance and source boundaries

| Item | Source and actual result |
| --- | --- |
| Initial full effects/render suite | `348d9672636e7df7026501935ecdd950c7cabd05`: 559 passed, zero failed, one existing ignore; three new ownership tests actually executed |
| Integrated full media/host suite | `1a2b4bf1ba6b84fcfb4ce9506b42a98da9a234e8`: 280 passed, zero failed, two existing large-source ignores |
| Integrated focused effects memory | Same `1a2b4bf1`: four passed, including both new program shader tests |
| Integrated focused render memory | Same `1a2b4bf1`: one passed, also included in the earlier full lower suite; do not count it as an additional distinct regression |
| Provenance/diff | Both measured sources passed; no dependency, asset, panel, token, layout or interaction change |

The initial full suite predates independent program shader observation. Its
559 passes are not attributed to that later change. The integrated source is
a private merge of our own package implementation and own #442 for testing;
publication must separately record its rebase onto main. No native result is
relabeled to a later documentation or delivery commit.

The real integration test creates the headless editor renderer, shares the
Session registry, independently derives exact package/shader String capacity,
and checks owner transitions two-to-one-to-zero. It verifies unchanged project,
revisions, undo, GPU texture ledger and budget, and no extra retained Arcs. After
package owners disappear, startup program shader capacity remains until those
program owners are also cleared. This is not a native-window test.

Lower tests use real parsed repository packages and actual compiled shader text
to distinguish shared Arcs from independent equal content and spare capacity.
The observation-limit tests deliberately use tiny metadata-only fixtures: they
test bounds and unknown markers, not shader execution. Serialized observations
are checked not to contain the shader-source marker.

Three existing CPU snapshot tests retain exact equality but now add the actual
startup package/program capacities to expected totals. Original scratch payload,
clear, spare-capacity, release and public-bridge checks remain. No assertion is
removed or weakened and no new ignore is introduced. Existing ignored tests are
unaccepted. The focused test groups overlap the earlier full suites as recorded.

Build/test files and temporary paths are on E:, with one compiler, fresh targets,
debug symbols/incremental disabled in the recorded correctness environment.
These runs prove ownership correctness, not Release performance or speedups.

## Original evidence

[Exact receipts](effect-package-memory-acceptance.json) bind commands, sources,
environments, terminal exits and logs. [Original logs/receipts](data/effect-package-memory-1a2b4bf1.zip)
include all measured phases and an internal per-file inventory. Archive SHA-256:
`f592d9906a2592c9af87a9bf64de93d5e9b11a6f16e8d42534d896a08b6301e0`. No binary, new font or third-party media is included.
The separate schema-2 run on source `709de340` is now accepted: six actual
Release cases, 408 raw / 360 measured frames, source/binary hashes and independent
pixel checks. All six project/pixel hashes match the original5824 study. See
[native cost-report acceptance](desktop-performance-cpu-completed.md).

## Issue #60 dispositions and remaining work

| Item | Disposition |
| --- | --- |
| R60.1 package/program CPU | New bounded observation; actual buffers and global cross-builder dedup tested |
| R60.2 aliases/domain separation | New shared-vs-independent and release regressions; existing texture ledger retained separately |
| R60.3 cost-report integration | Source709de340 accepted six Release cases/408 raw frames; see separate native cost-report record |
| Metadata/containers/external owners | Explicit unknown/exclusion; no process RSS or physical VRAM claim |
| R60.4/A60.1 actual workloads | Earlier desktop study keeps its original source and scope; not reaccepted by these CPU correctness tests |
| A60.2 Android/native device scope | Physical Adreno/Mali, file decoding, window/deadline and pressure studies remain pending; #61 desktop-only exception does not close #60 |
| A60.3 optimization claims | No before/after speedup claimed |

Image decode workers, original-source input caches, uninstrumented project/scene/
container storage, inference weights/activations, driver/decoder allocations and
physical residency remain outside this increment. DESIGN remediation rows are
retained because no panel or visual/interaction file is edited.

## Delivery source

Own #442 merged as `38e5be36516e81aa5d00c8876ea690b00cbe388c`.
The four implementation commits were applied to that latest main in delivery
code source `51095815f4cd364c0d52d405dd85ae902ff6ba1c`. All 20 own implementation/test/tool/API files
are byte-identical to measured cost source709de340; the manifest records blobs
and SHA-256. Differences from that private source are unrelated already merged
desktop/UI files and their validation docs, listed as paths without review.
No private merge history is published. Current-head CI is required before merge;
no local result is relabeled to the delivery or later documentation commits.

Completion/acceptance records and the baseline status were updated only after
that native run. The 20-file equality proof identifies delivery code51095815
before those documentation updates; it does not claim a new native run.
