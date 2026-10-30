# Experimental desktop prerender control

The desktop MCP tool `configure_prerender` replaces an application policy. It
requires Edit access but no expected project revision: it does not edit, save,
seek, add history, or change formal output. Default policy remains disabled.
Read-only clients can inspect `motion_state` but cannot configure background work.

```json
{"name":"configure_prerender","arguments":{"enabled":true,"window":3,"idleDelayMs":200}}
```

`enabled` is required and boolean. Optional `window` is an integer in [0,8],
default 3; zero also disables effective scheduling. `idleDelayMs` is an integer in
[0,60000], default 200. Each call replaces the policy, so omitted fields return to
these defaults. Invalid types, ranges and unknown fields fail before enqueue and
leave the prior policy unchanged. The existing seek-speed guard remains 120
frames/second. A configuration change cancels the old generation before admitting
new work; actual owners remain retained until cancellation acknowledgment.

```json
{"name":"configure_prerender","arguments":{"enabled":false}}
```

The reply contains the effective `enabled`, requested `policy`, supported profile,
quotas, holds and fault. `motion_state.desktop.prerender` additionally reports
actual cached presentations; worker in-flight observation remains independent of
the serial queue. Enabled is not a promise of a hit or completed native work.

The current experimental profile requires the shared native window, High preview,
authored camera and single-time output, without an active edit gesture or memory
pressure. Unsupported profiles hold rather than substitute a different image.
Headless `--mcp` reports `no_surface` and does not create a background native owner.
Use `--mcp-ui` to control the same Engine as the window and select High using the
existing preview control. Default policy, general prerender capability and UI
layout are unchanged; the capability remains false pending complete acceptance.

Foreground requests preempt optional work before entering the serial handler.
Disable or change the policy through the same tool at runtime. No cache directory,
credentials, arbitrary payload, network render or disk cache is accepted. CPU and
GPU allowances remain bounded (64 MiB completion CPU, 128 MiB native cache plus
shared renderer admission). They are logical allowances, not physical residency.

The component routing and actual headless stdio checks passed at source c3f91378,
including default-level file logs, stdout isolation and the exact tool catalogue.
The native pressure/lifecycle control also passed through this ToolRouter. The
paired model study passed at source 7468f7c0 after presentation-aligned scheduling
replaced the unsuccessful timer-only change. See
`engineering/validation/native-prerender-public-controls-current.md` and
`engineering/validation/native-prerender-paced-current.md` for exact source
boundaries, actual measurements and retained failures. Final combined regression,
cross-platform CI, source freshness and broader profiles remain pending. Android
is retired.
