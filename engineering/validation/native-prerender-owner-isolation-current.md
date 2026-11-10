# Native prerender owner isolation (#82)

Status: actual owner-isolation controls accepted at `d1c65167cdd75051e211b3928b18e1cbc3b4535d`.
This is not Engine/Session cache-consumer or full-workspace acceptance.

Session-side capture now clones project, registry, policy, depth metadata,
platform, shared allocation budget and paths. It performs no source opening,
canonicalization, package preflight or source copying. Background preparation
publishes Freeze before invoking the existing validated source-opening policy,
copying and hashing. Movie-export source-retention semantics are unchanged.
Prerender source bytes become authoritative at background preparation; removal
or mutation before opening is not an owner-capture snapshot guarantee. The
coordinator still must reject obsolete document/source generations.

A window-backed capture retains its actual foreground adapter handle. The
background thread initializes a separate device and queue on that adapter;
it does not transfer a Session, Renderer or GPU target. A headless Session
without Graphics can still select a headless adapter. Repeated device creation
from an adapter is a native-wgpu path, not a browser WebGPU contract.

Two new controls exercise missing-source metadata capture followed by an actual
background Freeze refusal and healthy recovery; and exact adapter identity,
distinct native device/queue ownership and real native pixels after Session
closure. The latter supplies the adapter explicitly for a headless Session;
it does not replace window-backed capture or multi-adapter machine acceptance.
Existing assertions are unchanged. No panels, layouts, dependencies or assets
are changed.

Engine interruption/scheduling, Session consumption, output-profile matching,
invalidation, default logger evidence and foreground P95 remain open. The
capability stays disabled. Android is excluded.

## Executed controls

The combined producer/worker filter passed 14 tests with zero failures and one
manual exclusion. The excluded all-four-precision control ran explicitly and
passed with zero exclusions. Full motion-desktop-media regression passed
86 tests with zero failures and two manual exclusions; those focused counts
overlap. The other manual exclusion is the existing keyframe timing proof,
which was not rerun here. Desktop ffmpeg/host ai-depth checking and provenance
passed. Source remained unchanged. This is not full renderer/workspace or
cross-platform acceptance of the constructor extraction.

Evidence: `data/native-prerender-owner-isolation-v1.zip`; SHA-256 `6aa96f0e08be17006ed700a5d2aaef7c5ea3b09dce55c1816a4059f8e72dcb11`;
206165 bytes; 16 entries, all read back and hash-checked.
