//! Translate a frozen selection as one edit. Selected destinations can overlap
//! selected sources, but never replace an unselected authored key.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyframeValueOffset {
    pub target: KeyframeTarget,
    pub component: usize,
    pub delta: f32,
}

pub(super) trait NumericOffset: Clone {
    fn offset(&self, component: usize, delta: f32) -> Result<Self>;
}
impl NumericOffset for f32 {
    fn offset(&self, component: usize, delta: f32) -> Result<Self> {
        ensure(component == 0, "scalar keyframe has no such component")?;
        let value = *self + delta;
        ensure(value.is_finite(), "keyframe value offset overflow")?;
        Ok(value)
    }
}
impl<const N: usize> NumericOffset for [f32; N] {
    fn offset(&self, component: usize, delta: f32) -> Result<Self> {
        ensure(component < N, "keyframe component out of range")?;
        let mut value = *self;
        value[component] += delta;
        ensure(
            value[component].is_finite(),
            "keyframe value offset overflow",
        )?;
        Ok(value)
    }
}
impl NumericOffset for crate::CurveObject {
    fn offset(&self, _: usize, _: f32) -> Result<Self> {
        Err(Error::Invalid(
            "color transfer curve has no numeric value component".into(),
        ))
    }
}

pub(super) fn plan<T: NumericOffset>(
    keys: &[Keyframe<T>],
    source: &[i32],
    destination: &[i32],
    copy: bool,
    value: Option<&KeyframeValueOffset>,
) -> Result<Vec<Keyframe<T>>> {
    source
        .iter()
        .zip(destination)
        .map(|(from, to)| {
            if keys.binary_search_by_key(to, |k| k.frame).is_ok() {
                ensure(
                    !copy && source.binary_search(to).is_ok(),
                    "keyframe destination is occupied",
                )?;
            }
            let index = keys
                .binary_search_by_key(from, |k| k.frame)
                .map_err(|_| Error::Invalid("selected keyframe does not exist".into()))?;
            let mut key = keys[index].clone();
            key.frame = *to;
            if let Some(value) = value {
                key.value = key.value.offset(value.component, value.delta)?;
            }
            Ok(key)
        })
        .collect()
}

/// Call through the existing core transaction so final project-domain checks
/// also restore the model and history. All source/destination and numeric
/// component checks complete before this helper writes any channel.
pub fn transform_keys(
    project: &mut Project,
    selections: &[KeyframeSelection],
    delta_frame: i32,
    value_offset: Option<&KeyframeValueOffset>,
    copy: bool,
) -> Result<()> {
    let local = interpolation::preflight(project, selections, "transform")?;
    let value_address = value_offset
        .map(|value| -> Result<String> {
            (|| {
                ensure(
                    value.delta.is_finite(),
                    "keyframe value offset must be finite",
                )?;
                value.target.signature(project)?;
                let address = value.target.address()?;
                let mut selected = false;
                for selection in selections {
                    if selection.target.address()? == address {
                        selected = true;
                        break;
                    }
                }
                ensure(selected, "keyframe value channel is not selected")?;
                Ok(address)
            })()
            .map_err(|cause| value.target.failure("transform", cause))
        })
        .transpose()?;
    let mut plans = Vec::with_capacity(selections.len());
    for (selection, source) in selections.iter().zip(&local) {
        let result = (|| -> Result<KeyframeData> {
            let destination = selection
                .frames
                .iter()
                .map(|frame| {
                    let next = i64::from(*frame) + i64::from(delta_frame);
                    ensure(
                        (0..i64::from(project.frames)).contains(&next),
                        "keyframe destination outside composition",
                    )?;
                    project.edit_frame(selection.target.object(), next as u32)
                })
                .collect::<Result<Vec<_>>>()?;
            let value = if value_address.as_ref() == Some(&selection.target.address()?) {
                value_offset
            } else {
                None
            };
            macro_rules! planned {
                ($t:expr, $variant:ident) => {
                    KeyframeData::$variant(plan(&$t.keys, source, &destination, copy, value)?)
                };
            }
            Ok(match read(project, &selection.target)? {
                ReadChannel::Scalar(t) => planned!(t, Scalar),
                ReadChannel::Vec2(t) => planned!(t, Vec2),
                ReadChannel::Vec3(t) => planned!(t, Vec3),
                ReadChannel::Vec4(t) => planned!(t, Vec4),
                ReadChannel::Geometry(t) => planned!(t, Geometry),
                ReadChannel::ColorCurve(t) => planned!(t, ColorCurve),
            })
        })();
        plans.push(result.map_err(|cause| interpolation::failure("transform", selection, cause))?);
    }
    for ((selection, source), plan) in selections.iter().zip(local).zip(plans) {
        macro_rules! apply {
            ($t:expr, $moved:expr) => {{
                if !copy {
                    $t.keys.retain(|k| source.binary_search(&k.frame).is_err());
                }
                $t.keys.extend($moved);
                $t.keys.sort_by_key(|k| k.frame);
            }};
        }
        let result = (|| -> Result<()> {
            match (write(project, &selection.target)?, plan) {
                (WriteChannel::Scalar(t), KeyframeData::Scalar(k)) => apply!(t, k),
                (WriteChannel::Vec2(t), KeyframeData::Vec2(k)) => apply!(t, k),
                (WriteChannel::Vec3(t), KeyframeData::Vec3(k)) => apply!(t, k),
                (WriteChannel::Vec4(t), KeyframeData::Vec4(k)) => apply!(t, k),
                (WriteChannel::Geometry(t), KeyframeData::Geometry(k)) => apply!(t, k),
                (WriteChannel::ColorCurve(t), KeyframeData::ColorCurve(k)) => apply!(t, k),
                _ => {
                    return Err(Error::Invalid(
                        "keyframe channel changed after transform preflight".into(),
                    ))
                }
            }
            Ok(())
        })();
        result.map_err(|cause| interpolation::failure("transform", selection, cause))?;
    }
    Ok(())
}
