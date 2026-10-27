//! glTF transform clips. Sampling is explicit and never changes layer Euler tracks.
use crate::{
    accessors::{Budget, Reader},
    invalid, unsupported, Result, Scene,
};
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationSample {
    pub animation: usize,
    pub seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Animation {
    name: Option<String>,
    duration: f32,
    channels: Vec<Channel>,
}
impl Animation {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn duration_seconds(&self) -> f32 {
        self.duration
    }
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
enum Path {
    Translation,
    Rotation,
    Scale,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
enum Interpolation {
    Step,
    Linear,
    Cubic,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
enum Values {
    Vector(Vec<[f32; 3]>),
    Rotation(Vec<[f32; 4]>),
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Channel {
    node: usize,
    path: Path,
    interpolation: Interpolation,
    times: Vec<[f32; 1]>,
    values: Values,
}

pub(crate) fn decode(reader: &Reader<'_>, budget: &mut Budget) -> Result<Vec<Animation>> {
    let mut out = Vec::with_capacity(reader.root.animations.len());
    let mut descriptors = 0usize;
    let mut keys = 0usize;
    for (id, source) in reader.root.animations.iter().enumerate() {
        descriptors = descriptors
            .checked_add(source.channels.len())
            .and_then(|n| n.checked_add(source.samplers.len()))
            .filter(|&n| n <= reader.limits.max_items)
            .ok_or_else(|| invalid("animation", id, "channel/sampler count limit exceeded"))?;
        if source.channels.is_empty()
            || source.samplers.is_empty()
            || source.name.as_ref().is_some_and(|name| name.len() > 256)
        {
            return Err(invalid(
                "animation",
                id,
                "empty clip or name exceeds 256 bytes",
            ));
        }
        let mut channels = Vec::with_capacity(source.channels.len());
        let mut targets = BTreeSet::new();
        let mut duration = 0f32;
        for (channel_id, channel) in source.channels.iter().enumerate() {
            let object = format!("{id}/channel={channel_id}");
            let path = match channel.target.path.as_str() {
                "translation" => Path::Translation,
                "rotation" => Path::Rotation,
                "scale" => Path::Scale,
                _ => {
                    return Err(unsupported(
                        "animation",
                        object,
                        "only TRS targets are supported",
                    ))
                }
            };
            let node = channel
                .target
                .node
                .ok_or_else(|| unsupported("animation", &object, "target without a node"))?;
            let target = reader
                .root
                .nodes
                .get(node)
                .ok_or_else(|| invalid("animation", &object, "target node out of range"))?;
            if target.matrix.is_some() || !targets.insert((node, path)) {
                return Err(invalid(
                    "animation",
                    &object,
                    "matrix target or duplicate node/property",
                ));
            }
            let sampler = source
                .samplers
                .get(channel.sampler)
                .ok_or_else(|| invalid("animation", &object, "sampler index out of range"))?;
            let interpolation = interpolation(&sampler.interpolation)
                .ok_or_else(|| unsupported("animation", &object, "unknown interpolation"))?;
            let input = reader.accessor(sampler.input, "SCALAR", false)?;
            keys = keys
                .checked_add(input.count)
                .filter(|&n| n <= reader.limits.max_animation_keys)
                .ok_or_else(|| invalid("animation", &object, "total decoded key limit exceeded"))?;
            let times = reader.animation_floats::<1>(sampler.input, false, budget)?;
            if times[0][0] < 0.
                || times.windows(2).any(|t| t[0][0] >= t[1][0])
                || input.min.as_deref() != Some(&times[0])
                || input.max.as_deref() != Some(&times[times.len() - 1])
                || (interpolation == Interpolation::Cubic && times.len() < 2)
            {
                return Err(invalid(
                    "animation",
                    &object,
                    "invalid key order, bounds or cubic key count",
                ));
            }
            duration = duration.max(times[times.len() - 1][0]);
            let output = reader
                .root
                .accessors
                .get(sampler.output)
                .ok_or_else(|| invalid("animation", &object, "output accessor out of range"))?;
            let expected = times
                .len()
                .checked_mul(if interpolation == Interpolation::Cubic {
                    3
                } else {
                    1
                })
                .ok_or_else(|| invalid("animation", &object, "output count overflow"))?;
            if output.count != expected {
                return Err(invalid(
                    "animation",
                    &object,
                    "output count does not match interpolation",
                ));
            }
            let values = if path == Path::Rotation {
                let mut values = reader.animation_floats::<4>(sampler.output, true, budget)?;
                // Integer quantization perturbs unit length; retain tangents unchanged.
                let tolerance = if matches!(output.component_type, 5120 | 5121) {
                    0.02
                } else {
                    0.001
                };
                for (index, value) in values.iter_mut().enumerate() {
                    if interpolation != Interpolation::Cubic || index % 3 == 1 {
                        let norm: f64 = value.iter().map(|&v| f64::from(v).powi(2)).sum();
                        if (norm - 1.).abs() > tolerance {
                            return Err(invalid("animation", &object, "non-unit rotation key"));
                        }
                        *value = normalized(value.map(f64::from))
                            .ok_or_else(|| invalid("animation", &object, "invalid rotation key"))?;
                    }
                }
                Values::Rotation(values)
            } else {
                Values::Vector(reader.animation_floats::<3>(sampler.output, false, budget)?)
            };
            channels.push(Channel {
                node,
                path,
                interpolation,
                times,
                values,
            });
        }
        out.push(Animation {
            name: source.name.clone(),
            duration,
            channels,
        });
    }
    Ok(out)
}

fn interpolation(value: &str) -> Option<Interpolation> {
    match value {
        "STEP" => Some(Interpolation::Step),
        "LINEAR" => Some(Interpolation::Linear),
        "CUBICSPLINE" => Some(Interpolation::Cubic),
        _ => None,
    }
}

impl Scene {
    /// Samples owned transform data; endpoint clamping follows glTF 2.0 Appendix C.
    /// The returned locals retain authored matrices on nodes without an active channel.
    pub fn sample_animation(&self, sample: AnimationSample) -> Result<Vec<[f32; 16]>> {
        if !sample.seconds.is_finite() {
            return Err(invalid(
                "animation_sample",
                sample.animation,
                "time must be finite",
            ));
        }
        let animation = self.animations.get(sample.animation).ok_or_else(|| {
            invalid(
                "animation_sample",
                sample.animation,
                "clip index out of range",
            )
        })?;
        let mut poses: Vec<Option<Transform>> = vec![None; self.nodes.len()];
        for (id, channel) in animation.channels.iter().enumerate() {
            let failure = |reason| {
                invalid(
                    "animation_sample",
                    format!("clip={}/channel={id}", sample.animation),
                    reason,
                )
            };
            let pose = match poses[channel.node] {
                Some(pose) => pose,
                None => self.nodes[channel.node]
                    .rest
                    .ok_or_else(|| failure("target has no authored TRS"))?,
            };
            let pose = poses[channel.node].insert(pose);
            match &channel.values {
                Values::Vector(values) => {
                    let value = channel.vector(values, sample.seconds);
                    if value.iter().any(|v| !v.is_finite()) {
                        return Err(failure("vector interpolation overflow"));
                    }
                    match channel.path {
                        Path::Translation => pose.translation = value,
                        Path::Scale => pose.scale = value,
                        Path::Rotation => return Err(failure("rotation storage mismatch")),
                    }
                }
                Values::Rotation(values) => {
                    pose.rotation = channel
                        .rotation(values, sample.seconds)
                        .ok_or_else(|| failure("zero or non-finite quaternion"))?;
                }
            }
        }
        self.nodes
            .iter()
            .zip(poses)
            .enumerate()
            .map(|(id, (node, pose))| {
                let Some(pose) = pose else {
                    return Ok(node.local);
                };
                let local = Mat4::from_scale_rotation_translation(
                    Vec3::from_array(pose.scale),
                    Quat::from_array(pose.rotation),
                    Vec3::from_array(pose.translation),
                );
                if !local.is_finite() {
                    return Err(invalid(
                        "animation_sample",
                        format!("clip={}/node={id}", sample.animation),
                        "local transform overflow",
                    ));
                }
                Ok(local.to_cols_array())
            })
            .collect()
    }
}

impl Channel {
    fn segment(&self, seconds: f64) -> (usize, usize, f64, f64) {
        let next = self.times.partition_point(|v| f64::from(v[0]) <= seconds);
        if next == 0 {
            return (0, 0, 0., 0.);
        }
        if next == self.times.len() {
            return (next - 1, next - 1, 0., 0.);
        }
        let previous = next - 1;
        let dt = f64::from(self.times[next][0]) - f64::from(self.times[previous][0]);
        (
            previous,
            next,
            (seconds - f64::from(self.times[previous][0])) / dt,
            dt,
        )
    }
    fn interpolate<const N: usize>(
        &self,
        values: &[[f32; N]],
        a: usize,
        b: usize,
        t: f64,
        dt: f64,
    ) -> [f64; N] {
        if self.interpolation == Interpolation::Cubic {
            let (t2, t3) = (t * t, t * t * t);
            std::array::from_fn(|c| {
                (2. * t3 - 3. * t2 + 1.) * f64::from(values[3 * a + 1][c])
                    + dt * (t3 - 2. * t2 + t) * f64::from(values[3 * a + 2][c])
                    + (-2. * t3 + 3. * t2) * f64::from(values[3 * b + 1][c])
                    + dt * (t3 - t2) * f64::from(values[3 * b][c])
            })
        } else if self.interpolation == Interpolation::Step || a == b {
            values[a].map(f64::from)
        } else {
            std::array::from_fn(|c| {
                (1. - t) * f64::from(values[a][c]) + t * f64::from(values[b][c])
            })
        }
    }
    fn vector(&self, values: &[[f32; 3]], seconds: f64) -> [f32; 3] {
        let (a, b, t, dt) = self.segment(seconds);
        self.interpolate(values, a, b, t, dt).map(|v| v as f32)
    }
    fn rotation(&self, values: &[[f32; 4]], seconds: f64) -> Option<[f32; 4]> {
        let (a, b, t, dt) = self.segment(seconds);
        if self.interpolation == Interpolation::Linear && a != b {
            let value = Quat::from_array(values[a]).slerp(Quat::from_array(values[b]), t as f32);
            normalized(value.to_array().map(f64::from))
        } else {
            normalized(self.interpolate(values, a, b, t, dt))
        }
    }
}
fn normalized(value: [f64; 4]) -> Option<[f32; 4]> {
    let length = value.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !length.is_finite() || length <= f64::MIN_POSITIVE {
        return None;
    }
    Some(value.map(|v| (v / length) as f32))
}
