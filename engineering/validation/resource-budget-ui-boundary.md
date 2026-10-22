# Host-injected UI resource admission

Issue: #61. PR: #425. This is an implementation record, not issue closure.

## Discovered failure

The Rust and GPU job in run `39817790739`, job `119960730170`, failed
`production_dependencies_keep_data_render_and_media_outside_the_editing_runtime`:
`motion-ui depends on motion-render`. The first delivery source was
`202b73e0d16b65acea00f47ae62a154079462121`. Its dependency boundary assertion is
unchanged. Provenance validation and selected crate tests had not established
this architectural requirement.

## Implementation

- Keep the toolkit's allocation contract in `motion-ui::paint::allocation`.
- Inject its implementation from the desktop host, using the renderer's existing
  `AllocationContext`, native owner, live allowance and upload retirement ledger.
- Keep admitted buffers and textures in opaque owned wrappers. Draws still use
  the existing allocated encoder and submission contract, including native GPU
  completion retention. No independent UI budget or raw allocation fallback is
  introduced on the admitted path.
- Preserve original native errors as downcastable `Error::source` values. Expose
  only explicit capacity refusals to the toolkit's bounded pending-atlas logic.
  Invalid atlas input and non-capacity native failures remain nonretryable.
- Compile the actual desktop adapter in toolkit native tests through a dev-only
  module; `motion-render` is a dev dependency of the toolkit. This avoids a
  second test-only allocator implementation. Existing exact native rejection,
  accounting, pixel and GPU retirement assertions remain in place.
- Add a native invalid-buffer/invalid-atlas regression, and assert that the
  injected capacity metadata matches the original native budget refusal.

## Acceptance still required

Run the unchanged model dependency-boundary test, native toolkit admission tests,
desktop default-output failure tests, full crate suites, configured package
limits, default and FFmpeg export paths, Release build, actual GUI/MCP acceptance,
provenance and style guards on the final source. Retain the failed CI receipt and
all source-specific prior results. Results from the earlier source cannot accept
this implementation. Android device and new Linux/Metal runtime acceptance
remain separate unproven parts of the original issue scope.

No panel, layout, style, interaction, retry policy, worker timeout or restart is
changed by this dependency fix. No third-party code or assets are added.
