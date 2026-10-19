//! Authoring defaults share the compiler's typed fallback contract. Coordinate,
//! image and distance-field sources stay explicit; this is not an implicit cast.
use crate::*;
use std::collections::BTreeMap;

pub fn authoring_input_defaults(operation: &NodeOp) -> BTreeMap<String, InputValue> {
    let numeric = |ty, value| match ty {
        ValueType::Vec2 => InputValue::Vec2([value; 2]),
        ValueType::Vec3 => InputValue::Vec3([value; 3]),
        ValueType::Color => InputValue::Color([value, value, value, 1.]),
        ValueType::Bool => InputValue::Bool(value != 0.),
        ValueType::DistancePixels => InputValue::DistancePixels(value),
        _ => InputValue::Float(value),
    };
    let mut values = BTreeMap::new();
    let mut add = |port: &str, value| {
        values.insert(port.into(), value);
    };
    match operation {
        NodeOp::Blend { .. } => {
            add("backdrop", InputValue::Color([0.1, 0.3, 0.8, 1.]));
            add("source", InputValue::Color([0.95, 0.45, 0.15, 1.]));
            add("factor", InputValue::Float(1.));
        }
        NodeOp::Noise { .. } => {
            add("offset", InputValue::Vec2([0.; 2]));
            add("scale", InputValue::Float(8.));
            add("seed", InputValue::Float(0.));
        }
        NodeOp::Gradient { mode } => {
            add("start", InputValue::Vec2(if *mode == GradientMode::Linear {
                [0., 0.5]
            } else {
                [0.5, 0.5]
            }));
            add("end", InputValue::Vec2([1., 0.5]));
            add("falloff", InputValue::Float(1.));
            add("a", InputValue::Color([0., 0., 0., 1.]));
            add("b", InputValue::Color([1.; 4]));
        }
        NodeOp::Math {
            operation,
            value_type,
        } => {
            use MathOp::*;
            let typed = |v| numeric(*value_type, v);
            match operation {
                Abs | Sin | Cos | Sqrt | Floor | Fract | Length | Normalize => {
                    add("value", typed(0.))
                }
                Clamp => {
                    add("value", typed(0.));
                    add("min", typed(0.));
                    add("max", typed(1.));
                }
                Lerp => {
                    add("a", typed(0.));
                    add("b", typed(1.));
                    add("factor", InputValue::Float(0.5));
                }
                Smoothstep => {
                    add("edge0", typed(0.));
                    add("edge1", typed(1.));
                    add("value", typed(0.5));
                }
                _ => {
                    let identity = if matches!(operation, Multiply | Divide) {
                        1.
                    } else {
                        0.
                    };
                    add("a", typed(identity));
                    add("b", typed(identity));
                }
            }
        }
        NodeOp::Splat { .. } => add("value", InputValue::Float(0.)),
        NodeOp::CombineVec2 | NodeOp::CombineVec3 => {
            add("x", InputValue::Float(0.));
            add("y", InputValue::Float(0.));
            if matches!(operation, NodeOp::CombineVec3) {
                add("z", InputValue::Float(0.));
            }
        }
        NodeOp::CombineColor => {
            for port in ["r", "g", "b"] {
                add(port, InputValue::Float(0.));
            }
            add("a", InputValue::Float(1.));
        }
        NodeOp::Select { value_type } if *value_type != ValueType::Image => {
            add("condition", InputValue::Bool(false));
            add("true_value", numeric(*value_type, 1.));
            add("false_value", numeric(*value_type, 0.));
        }
        NodeOp::ColorMath {
            operation: ColorOp::Mix,
        } => add("factor", InputValue::Float(0.5)),
        NodeOp::SdfCircle | NodeOp::SdfRoundedRectangle => {
            add("center", InputValue::Vec2([0.; 2]));
            add("radius", InputValue::Float(18.));
            if matches!(operation, NodeOp::SdfRoundedRectangle) {
                add("half_size", InputValue::Vec2([18.; 2]));
            }
        }
        NodeOp::SdfLine | NodeOp::SdfCapsule => {
            add("a", InputValue::Vec2([0.; 2]));
            add("b", InputValue::Vec2([32., 0.]));
            add(
                if matches!(operation, NodeOp::SdfLine) {
                    "width"
                } else {
                    "radius"
                },
                InputValue::Float(1.),
            );
        }
        NodeOp::SdfSmoothUnion => add("smoothness", InputValue::Float(4.)),
        NodeOp::SdfStroke => add("width", InputValue::Float(1.)),
        _ => {}
    }
    values
}
