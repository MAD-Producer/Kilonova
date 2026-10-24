# Effect scratch ownership and packed plan v10

This document describes the implementation for
[issue 67](https://github.com/MAD-Producer/Kilonova/issues/67). It is a resource
and execution contract, not a statement that final issue acceptance or CI has
completed. Native source `2f243de3176ed4714ee9dae17f20f3b0acefdf80` introduces
this contract on main `1d1f284245fcd50391f672ef79653d09d432e219`.

## Logical values and physical storage

Effect construction retains its eight local semantic slot names. They do not
identify native allocations. A full attachment clear creates a fresh logical
value with its own format, working space, alpha convention, extent and usage.
The graph records both sampled operands before recording a write. Execution
points are inclusive: a pass cannot sample its own output allocation even when
that pass is the input's last consumer. An attachment load preserves the value
and requires an unchanged descriptor; production effect passes currently clear.

Each draw is an atomic execution unit with an explicit terminal consumer after
its last effect pass. Its final value must be copied into an independently
owned dependency snapshot or consumed by composition/adjustment before the next
draw reuses storage. Draws may be replayed or reordered, but their temporary
resources must not be interleaved. Scratch names reset at each draw boundary.
External assets, masks and independently retained image inputs remain external
tokens, not scratch values retained across draws.

Deterministic interval placement reuses storage only after the inclusive last
consumer. It compares actual byte growth with the cost of a new exact extent.
Growing a wide/tall bounding rectangle is not automatically preferable to
another allocation. Tie breaking uses stable physical IDs. Resource/allocation
counts and capacity arithmetic are bounded; an invalid graph or rejected
placement publishes no partial descriptor table.

Native storage format and usage must match. A full clear permits the same
storage to hold different working-space or alpha values in separate lifetimes;
it does not reinterpret retained pixels. Logical values still carry those
semantics independently. Rgba8UnormSrgb, Rgba8Unorm and Rgba16Float remain
separate storage classes. Raster and masked-raster host utilities output linear
premultiplied pixels directly; their inherited plugin output-mode fields do not
change that shader behavior.

## Capacity, padding and native ownership

`EffectFramePlan::scratch_allocations` is the actual physical descriptor table.
`scratch_bindings` maps each pass's operands and output to that table, and
`scratch_outputs` maps each materialized draw to its final physical output.
`scratch_bytes()` sums width × height × actual format bytes per pixel. Legacy
slot maxima and the free semantic capacity helpers are construction diagnostics
and counterfactual comparisons; they are not native capacity or admission.

Placement does not change effect padding, logical output bounds or source
coordinates. Pass viewports use their logical pixel extents. Final draw UV
fractions divide those extents by the selected physical backing dimensions.
Shaders sample using the actual input texture dimensions and logical regions.
Every pass clears the entire backing, including padding. Native snapshot
restoration clears a larger destination before copying a smaller valid region.
GLES clears explicitly disable scissoring and enable all color write channels.

Native pool replacement allocates all required new textures before moving any
old texture. A later refusal leaves the old pool, bindings, identities and
admission intact. Matching storage can retain its native identity when only the
accumulated semantic masks change. Existing command/upload allocation retention
still applies; pool reuse does not release in-flight native ownership.

Active planned capacity is not a transient frame peak. Allocation admission
also counts retained old resources and replacements during a transaction. An
empty native effect plan retains the existing warm-pool policy for up to 120 idle
preparations; zero active descriptors do not mean native memory was freed.
Retained bytes remain visible in the native ledger and application allowance.

## Wire layout

The producer and consumers accept packed plan version **10**. The header is
160 bytes. Draw records remain 128 bytes, pass records 40 bytes, and effect
uniforms 624 bytes. Existing header offsets through the compositing metadata
stay fixed. Fields are native-endian u32/f32 words on the supported consumers.

| Header byte offset | Meaning |
| ---: | --- |
| 140 | Physical descriptor table byte offset |
| 144 | Physical allocation count |
| 148 | Descriptor stride, 32 bytes |
| 152, 156 | Reserved zero |

The physical table follows the compositing records. Each descriptor has eight
u32 words:

| Word | Meaning |
| ---: | --- |
| 0, 1 | Positive backing width and height |
| 2 | Format: 0=Rgba8UnormSrgb, 1=Rgba8Unorm, 2=Rgba16Float |
| 3 | Working-space mask: bit 0=linear, bit 1=sRGB numeric values |
| 4 | Alpha mask: bit 0=premultiplied, bit 1=straight |
| 5 | Usage mask: attachment, sampled, copy source, copy destination; value 15 |
| 6 | Actual format byte size: 4, 4 or 8 |
| 7 | Reserved zero |

Semantic masks are nonempty subsets of the two defined bits. They describe
separate cleared lifetimes, not multiple valid interpretations of one retained
image. Consumers interpret each pass through its shader/uniform contract.

Pass input/source/output fields now identify physical allocations. Negative
external tokens preserve their asset/mask meaning. Draw word 27 carries the
physical final output, and words 25/26 carry the corresponding UV fractions.
Consumers must not assume a maximum of eight allocations or a final output at
slot zero. `MAX_SCRATCH_PASSES` is computed as
`MAX_LAYERS * (MAX_EFFECTS_PER_LAYER * (MAX_PASSES + 3) + 1)`, currently 22,656;
physical allocation count cannot exceed that cap.

Host `render_plan_info` publishes `scratchRecordBytes` and `maxScratchPasses`.
Its caller-buffer hint includes descriptor records and the effect/conversion
pass bound. GLES validates version, table bounds, stride, count, positive
extents, format/byte-size agreement, semantic masks, usage, reserved words and
capacity overflow before allocating. Native and GLES pools use the same
descriptor dimensions and formats. This version does not claim Android device
rendering acceptance; that validation is outside the maintained-desktop scope.

## Acceptance boundary

Planning tests alone do not prove native allocation, pixel equivalence or
consumer execution. Acceptance requires source-bound native before/after
captures, actual descriptor/ledger agreement, inclusive-lifetime checks,
padding/reorder/refusal/recovery cases and current consumer tests. A package's
presence does not prove a generator visibly contributed to a combined frame.

Any measured reduction applies to its authored fixtures and logical storage
bytes. It does not establish universal savings, latency improvement or physical
driver VRAM reduction. Project-selectable precision, additional storage formats
and a typed execution DAG remain separate subsequent issues.
