//! Model-side display units; anchor coordinates are weighted by source size.
use super::*;
pub(super) trait Numeric {
    fn components(&self) -> Option<Vec<f64>>;
}
impl Numeric for f32 {
    fn components(&self) -> Option<Vec<f64>> {
        Some(vec![f64::from(*self)])
    }
}
impl<const N: usize> Numeric for [f32; N] {
    fn components(&self) -> Option<Vec<f64>> {
        Some(self.iter().map(|v| f64::from(*v)).collect())
    }
}
impl Numeric for crate::CurveObject {
    fn components(&self) -> Option<Vec<f64>> {
        None
    }
}
pub(super) struct Metric {
    pub label: String,
    pub numeric: bool,
    pub temporal: bool,
    pub dimensions: usize,
    pub scale: [f64; 6],
}
impl Metric {
    fn measure(&self, values: &[f64]) -> Result<f64> {
        ensure(
            self.numeric && values.len() >= self.dimensions,
            "property-unit speed is unavailable for this keyframe type",
        )?;
        let value = if self.dimensions == 1 {
            values[0] * self.scale[0]
        } else {
            values
                .iter()
                .zip(self.scale)
                .take(self.dimensions)
                .fold(0f64, |norm, (v, s)| norm.hypot(v * s))
        };
        ensure(value.is_finite(), "keyframe velocity magnitude overflow")?;
        Ok(value)
    }
    pub fn difference<T: Numeric>(&self, a: &T, b: &T) -> Result<f64> {
        let a = a
            .components()
            .ok_or_else(|| Error::Invalid("keyframe has no numeric velocity".into()))?;
        let b = b
            .components()
            .ok_or_else(|| Error::Invalid("keyframe has no numeric velocity".into()))?;
        self.measure(&a.iter().zip(b).map(|(a, b)| b - a).collect::<Vec<_>>())
    }
    pub fn tangent<T: Numeric>(&self, value: &T, factor: f64) -> Result<f64> {
        let value = value
            .components()
            .ok_or_else(|| Error::Invalid("keyframe has no numeric tangent".into()))?;
        self.measure(&value.iter().map(|v| v * factor).collect::<Vec<_>>())
    }
}
pub(super) fn metric(project: &Project, target: &KeyframeTarget) -> Result<Metric> {
    let mut result = Metric {
        label: "value/s".into(),
        numeric: true,
        temporal: true,
        dimensions: match read(project, target)? {
            ReadChannel::Scalar(_) => 1,
            ReadChannel::Vec2(_) => 2,
            ReadChannel::Vec3(_) => 3,
            ReadChannel::Vec4(_) => 4,
            ReadChannel::Geometry(_) => 2,
            ReadChannel::ColorCurve(_) => 0,
        },
        scale: [1.; 6],
    };
    result.numeric = result.dimensions > 0;
    let layer = project.layers.iter().find(|l| l.id == target.object());
    match target {
        KeyframeTarget::Property { property, .. } => match property {
            Property::Anchor => {
                let layer = layer.ok_or(Error::Missing(target.object()))?;
                result.scale[0] = f64::from(layer.size[0]);
                result.scale[1] = f64::from(layer.size[1]);
                result.label = "px/s".into();
            }
            Property::Opacity => {
                result.scale[0] = 100.;
                result.label = "%/s".into();
            }
            Property::Position | Property::Target | Property::Radius => {
                result.label = "px/s".into()
            }
            Property::Scale => result.label = "%/s".into(),
            _ => result.label = "deg/s".into(),
        },
        KeyframeTarget::Audio { .. } => result.label = "dB/s".into(),
        KeyframeTarget::Mask { property, .. } => {
            if *property == crate::masks::MaskProperty::Opacity {
                result.scale[0] = 100.;
                result.label = "%/s".into();
            } else {
                result.label = "px/s".into();
            }
        }
        KeyframeTarget::Effect { effect, param, .. } => {
            let p = layer
                .and_then(|l| l.effects.iter().find(|e| e.id == *effect))
                .and_then(|e| e.params.get(param))
                .ok_or_else(|| Error::Invalid("keyframe timing effect parameter missing".into()))?;
            result.numeric = !p.kind.discrete() && p.curve.is_none();
            result.temporal = !p.kind.discrete();
            result.dimensions = p.kind.dimensions();
            if p.kind == motion_effects::ParamKind::Color {
                result.label = "RGBA/s".into();
            }
        }
        KeyframeTarget::Vector { parameter, .. } => {
            result.label = match parameter.as_str() {
                "position" | "anchor" | "size" | "width" | "stroke_width" | "dash_offset" => "px/s",
                "rotation" | "skew" | "skew_axis" | "trim_offset" => "deg/s",
                "scale" | "opacity" | "start_opacity" | "end_opacity" | "trim_start"
                | "trim_end" | "start" | "end" => "%/s",
                "color" | "fill" | "stroke_color" => "RGBA/s",
                p if p.starts_with("node:") || p.starts_with("dash_") => "px/s",
                _ => "value/s",
            }
            .into();
        }
    }
    Ok(result)
}
