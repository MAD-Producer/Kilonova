# Public experimental prerender controls (#82)

Accepted component source: `c3f9137874ca5fafe231b15f2beb5d7ba65571f6`, based on main
`5549621703fd81917ec9320ce9fb8bbb4082840e`. Android is retired. This does
not close #82 or qualify general prerender capability, which remains false.

## Public entry and observability

`configure_prerender` replaces a bounded application policy without editing the
authored project. It is disabled by default, denied to ReadOnly clients, requires
an enabled boolean, and accepts window 0..8 and idleDelayMs 0..60000. Invalid
arguments preserve the current policy. State reports requested policy and actual
holds, faults and cached presentations separately. Headless enable creates no
native background owner; the native High/authored/single-time profile is explicit.

All eight MCP tests passed, including two new controls. The actual ffmpeg/ai-depth
desktop binary then served separate writable and read-only headless stdio runs.
Each produced exactly eight parseable JSON-RPC replies and the exact eleven-tool
catalogue. Valid enable/disable, invalid policy, permission refusal and unchanged
project/revision/frame/history/document generation were checked. MOTION_LOG was
unset. Actual default Info start logs were retained separately from stdout/stderr.
These transport checks are not native cache or performance evidence.

## Native pressure and document ownership

The explicit ignored native pressure/lifecycle control passed 1/0/0 through the
public ToolRouter. It warmed and actually consumed red cached pixels, injected a
typed preview-budget refusal, waited for real worker acknowledgment, and checked
no new dispatch during the stable pressure hold. Foreground pixels stayed exact.
After explicitly releasing the fixture it required two NEW completed jobs before
warm consumption. Switching to a same-size green project and reopening the red
project kept correct pixels and authored documents. This is not physical adapter
low-memory qualification or proof of automatic recovery from hardware pressure.

## Failures and remaining work

The subsequent paced v1 study failed the unchanged actual-hit criterion: the
first enabled episode consumed 7/60 cached frames; disabled P95 was 6759us and
enabled P95 8925us. It stopped before the second pair, so this is not a complete
paired result. Original samples and failure logs remain in
`artifacts/pr-queue/prerender-paced-v1`. The later successful presentation-aligned
correction is recorded separately in `native-prerender-paced-current.md`, with
the same actual-hit and paired P95 requirements.

The earlier exact-source full workspace gate at 32f2aa29 failed in the model
composition candidate oracle process (Windows 0xc0000005). Its relationship to
this feature is unconfirmed. The unchanged
binary passed its exact failing test and all three tests on diagnostic reruns;
the cause remains unconfirmed. Original failure/WER and reruns remain in
`artifacts/pr-queue/prerender-protection-v1`. Neither this archive nor those
diagnostic passes constitute a green full workspace regression.

Sustained playback/P95 passed separately at 7468f7c0. Final-source full regression, original blocked/idle/gap
controls, external same-path source freshness, broader profiles and cross-platform
CI remain. No disk cache or disk-full/corruption acceptance is claimed. The shared
window catalogue smoke is updated in code but is not executed by this receipt.
No panels, visual styles or user interaction layout changed.

Archive: `data/native-prerender-public-controls-v1.zip`.
SHA-256: `140cf51919c4a3e677de7c0bae51b7a25585964b32667a2d8db24f77f4b1a20d`. All 31 members were read back byte for byte.
