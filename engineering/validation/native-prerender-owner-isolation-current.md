# Native prerender owner isolation (#82)

Status: implementation and two actual controls authored; execution pending.
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
