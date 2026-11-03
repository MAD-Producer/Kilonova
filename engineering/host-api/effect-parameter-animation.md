# Effect parameter animation contract

This API describes the stored effect parameter schema in the current composition.
It does not certify package availability, an unlocked layer, an enabled effect,
node compiler equivalence, or successful GPU rendering. These remain separate
instance and runtime conditions.

## Query

`Session::snapshot()` publishes `capabilities.effect_parameters` and an
`effectParameterCapabilities` array. Each entry identifies a layer and effect
instance, then maps stable parameter IDs to their kind, implementation flag,
and animation support. The array includes stored instances outside the current
clip interval; `sampledEffects` describes only the sampled scene.

```json
{
  "layer": 7,
  "instance": 1,
  "parameters": {
    "p0001": {
      "kind": "float",
      "implemented": true,
      "animation": {
        "keyframes": true,
        "expressions": true,
        "interpolation": "numeric"
      }
    }
  }
}
```

The response is derived from `EffectParam::animation_support()`, which is also
used by expression target admission. There is no additional command or package
format version. Parameter IDs and types come from the actual stored schema;
clients must not infer them from labels or assume every package has the same IDs.

| Stored kind | Keyframes when animatable and implemented | Expression target | Interpolation |
| --- | --- | --- | --- |
| Float, Vec2, Vec3, Color | Yes | Yes, unless a color-curve track is attached | Existing numeric track and easing semantics |
| Bool, Enum | Yes | No | Hold; no fractional category values |
| Curve | Yes | No | Existing color-curve LUT interpolation |
| Non-animatable or unimplemented parameter | Not advertised | No | `null` |
| Image input / resource binding | Outside this parameter-track schema | Not advertised | No continuous resource interpolation |

An unimplemented parameter can still accept some default-preserving edits in
the existing editor. That does not make it an implemented animation feature.
This query does not change those editing semantics. A disabled or locked
instance retains the same schema description; editing or sampling availability
must be checked separately. In particular a locked-layer write still fails.

## Editing and clocks

Use the existing `effect` actions, with composition-frame coordinates. The
editor maps key positions to the layer-local track using the clip offset.
Sampling uses the existing layer clock; this API does not replace independent
time mapping or add fractional key storage. Snapshot sampling may use a
fractional composition frame.

For the repository-authored brightness/contrast fixture in the linked test,
layer 7 has offset 10, effect instance 1, and Float parameter `p0001`:

```json
[
  {"op":"effect","object":7,"action":{"kind":"animate","effect":1,"param":"p0001","frame":10,"enabled":true}},
  {"op":"effect","object":7,"action":{"kind":"set","effect":1,"param":"p0001","frame":20,"value":[100,0,0,0]}}
]
```

This stores local keys at 0 and 10 and samples 55 at composition frame 15.5.
One command batch is one undo step. An invalid later command rolls the entire
batch back, preserving the original model and history. The executable example
also tests Bool and Enum boundaries, color-curve midpoint sampling, undo/redo,
storage save/load, and a newly created session after reopen.

## Existing property references

Cross-parameter expressions already use the existing `propertyValue` runtime.
Both `capabilities.property_expressions.cross_property_references` and
`capabilities.effect_parameters.cross_parameter_expressions` report that support.
This change does not introduce another expression language or evaluation path.

Generate references with `motion_core::expression_reference_source` rather than
assembling descriptors from labels. Object and effect IDs inside JavaScript
reference descriptors are canonical decimal strings, preserving full `u64`
identity. Reference sample time is composition seconds, then each referenced
track applies its own layer clock and offset. Numeric IDs in ordinary editing
commands retain their existing JSON command representation.

Existing dependency cycle detection also rejects time-shifted recursion. The
runtime bounds dependency depth and calls and shares the existing expression
execution/frame budgets. Referenced failures preserve their original causes,
including when JavaScript tries to catch an error. Discrete and curve targets
are still rejected; no new continuous interpolation is provided for resources.

## Executable evidence and limits

The API example is implemented as actual host commands in
[`parameter_animation.rs`](../../crates/motion-host/tests/layer_time_mapping/parameter_animation.rs),
using repository-authored procedural solids and built-in effect definitions.
There are no external media assets or third-party fixtures. Run:

```sh
cargo test -p motion-host -p motion-core --test layer_time_mapping --test expressions --test expression_references --no-default-features --locked --offline -- --test-threads=1
```

The example's media platform deliberately fails if a decoder or GPU surface is
requested. It proves real host command admission, sampled state, references,
history and persistence, not GPU pixel equivalence or final video export.
The matrix exercises actual built-in Float, Vec2, Color, Bool, Enum and Curve
parameters. Vec3 follows the shared model predicate but has no built-in fixture
in this gate and is not claimed as package or GPU acceptance.

See the [validation record](../validation/professional-api86-parameter-contract-current.md)
for executed results, original failed attempts and remaining issue #86 scope.
The separate [professional API matrix and native camera example](professional-compositing-api.md)
adds actual GPU/frozen PNG/movie proof for one procedural camera fixture; it
does not change the limits of the parameter-only host gate above.
