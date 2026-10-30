//! Read-only authored tracks. Coordinates stay in composition time; samples
//! retain the model's local time, spatial interpolation and parameter bounds.
use super::*;
use motion_core::{masks::MaskProperty, AudioChannel, Track, Tween};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::panels) enum Unit {
    Pixels,
    Percent,
    Degrees,
    Decibels,
    Rgba,
    Native { identity: String, label: String },
}
impl Unit {
    pub fn title(&self, speed: bool) -> String {
        let name = match self {
            Self::Pixels => "px",
            Self::Percent => "%",
            Self::Degrees => "deg",
            Self::Decibels => "dB",
            Self::Rgba => "RGBA",
            Self::Native { label, .. } => label,
        };
        if speed {
            format!("{name}/s")
        } else {
            name.into()
        }
    }
}
#[derive(Clone)]
enum Samples {
    Scalar(Track<f32>),
    Vec2(Track<[f32; 2]>),
    Vec3(Track<[f32; 3]>),
    Vec4(Track<[f32; 4]>),
}
#[derive(Clone)]
pub(in crate::panels) struct Data {
    samples: Samples,
    pub dimensions: Vec<usize>,
    pub frames: Vec<u32>,
    pub unit: Unit,
    pub title: String,
    channel_title: String,
    pub offset: i64,
    scales: [f32; 4],
    limits: Option<[f32; 2]>,
    discrete: bool,
}
#[derive(Debug)]
pub(in crate::panels) enum Unavailable {
    Missing,
    Unsupported,
    Limit,
    Fault(String),
}
impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Missing => "channel no longer exists",
            Self::Unsupported => "this channel has no numeric graph",
            Self::Limit => "comparison display limit is 128 curves",
            Self::Fault(cause) => cause,
        })
    }
}
fn decode<T: Tween + serde::de::DeserializeOwned>(value: &Value) -> Result<Track<T>, Unavailable> {
    // The address resolver has already selected a coupled channel or one
    // separated axis. Decode that shape directly so an untagged outer enum
    // cannot erase the original key/value failure.
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields, bound(deserialize = "T: serde::Deserialize<'de>"))]
    struct Coupled<T> {
        value: T,
        keys: Vec<motion_core::Keyframe<T>>,
    }
    let decoded: Coupled<T> = serde_json::from_value(value.clone())
        .map_err(|e| Unavailable::Fault(crate::fallback_observation::error_cause(e)))?;
    let track = Track {
        spatial_animation: None,
        value: decoded.value,
        keys: decoded.keys,
        axes: None,
    };
    track
        .validate_local()
        .map_err(|e| Unavailable::Fault(e.to_string()))?;
    Ok(track)
}
fn property_title(property: Property, locale: Locale) -> &'static str {
    locale.t(match property {
        Property::Anchor => T::Anchor,
        Property::Position => T::Position,
        Property::Scale => T::Scale,
        Property::Rotation => T::Rotation,
        Property::Opacity => T::Opacity,
        Property::Target => T::CameraTarget,
        Property::Roll => T::CameraRoll,
        Property::Fov => T::CameraFov,
        Property::Radius => T::CameraRadius,
        Property::Azimuth => T::CameraAzimuth,
        Property::Elevation => T::CameraElevation,
    })
}
pub(in crate::panels) fn load(
    state: &Value,
    target: &KeyframeTarget,
    locale: Locale,
) -> Result<Data, Unavailable> {
    let (track, offset) =
        super::super::key_selection::authored_track(state, target).ok_or(Unavailable::Missing)?;
    let value = &track["value"];
    let count = if value.is_number() {
        1
    } else if let Some(v) = value.as_array() {
        if !(2..=4).contains(&v.len()) {
            return Err(Unavailable::Unsupported);
        }
        v.len()
    } else {
        return Err(Unavailable::Unsupported);
    };
    let samples = match count {
        1 => Samples::Scalar(decode(track)?),
        2 => Samples::Vec2(decode(track)?),
        3 => Samples::Vec3(decode(track)?),
        _ => Samples::Vec4(decode(track)?),
    };
    let layer = super::super::keyframe_clipboard::layer(state, target.object());
    let layer_name = layer.and_then(|l| l["name"].as_str()).unwrap_or("Camera");
    let mut data = Data {
        samples,
        dimensions: (0..count).collect(),
        frames: track["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|k| {
                k["frame"]
                    .as_i64()?
                    .checked_add(offset)
                    .and_then(|f| u32::try_from(f).ok())
                    .filter(|f| f64::from(*f) <= last_frame(state))
            })
            .collect(),
        unit: Unit::Native {
            identity: "value".into(),
            label: "value".into(),
        },
        title: layer_name.into(),
        channel_title: String::new(),
        offset,
        scales: [1.; 4],
        limits: None,
        discrete: false,
    };
    let title = match target {
        KeyframeTarget::Property { property, axis, .. } => {
            let expected = if property.is_scalar() || axis.is_some() {
                1
            } else if *property == Property::Anchor {
                2
            } else {
                3
            };
            if count != expected {
                return Err(Unavailable::Fault(
                    "property track dimension mismatch".into(),
                ));
            }
            data.unit = match property {
                Property::Anchor | Property::Position | Property::Target | Property::Radius => {
                    Unit::Pixels
                }
                Property::Opacity | Property::Scale => Unit::Percent,
                _ => Unit::Degrees,
            };
            if *property == Property::Anchor {
                for i in 0..2 {
                    data.scales[i] = layer
                        .and_then(|l| l["size"][i].as_f64())
                        .filter(|v| v.is_finite() && *v > 0.)
                        .ok_or_else(|| Unavailable::Fault("anchor source size missing".into()))?
                        as f32;
                }
            } else if *property == Property::Opacity {
                data.scales[0] = 100.;
            }
            if axis.is_none() && count == 3 && layer.is_some_and(|l| l["three_d"] != true) {
                data.dimensions = if *property == Property::Rotation {
                    vec![2]
                } else {
                    vec![0, 1]
                };
            }
            format!(
                "{}{}",
                property_title(*property, locale),
                axis.map_or(String::new(), |a| format!(
                    " {}",
                    ["X", "Y", "Z"][a.index()]
                ))
            )
        }
        KeyframeTarget::Effect { effect, param, .. } => {
            let effect = layer
                .and_then(|l| l["effects"].as_array())
                .and_then(|es| es.iter().find(|e| e["id"] == *effect))
                .ok_or(Unavailable::Missing)?;
            let p = &effect["params"][param];
            let kind: motion_effects::ParamKind = serde_json::from_value(p["kind"].clone())
                .map_err(|e| Unavailable::Fault(e.to_string()))?;
            data.dimensions = (0..kind.dimensions()).collect();
            if count != 4 {
                return Err(Unavailable::Fault("effect track dimension mismatch".into()));
            }
            data.discrete = kind.discrete();
            let min = p["min"]
                .as_f64()
                .ok_or_else(|| Unavailable::Fault("effect minimum missing".into()))?
                as f32;
            let max = p["max"]
                .as_f64()
                .ok_or_else(|| Unavailable::Fault("effect maximum missing".into()))?
                as f32;
            if !min.is_finite() || !max.is_finite() || min > max {
                return Err(Unavailable::Fault("invalid effect bounds".into()));
            }
            data.limits = Some([min, max]);
            // The model does not persist a physical unit for effect parameters.
            // Unknown parameters share an axis only for the same effect identity
            // and parameter, never because both happen to contain numbers.
            data.unit = if kind == motion_effects::ParamKind::Color {
                Unit::Rgba
            } else {
                Unit::Native {
                    identity: format!(
                        "{}:{}:{param}",
                        effect["hash"].as_str().unwrap_or(""),
                        effect["effect"].as_str().unwrap_or("")
                    ),
                    label: format!(
                        "{} · {param} (value)",
                        effect["effect"].as_str().unwrap_or("Effect")
                    ),
                }
            };
            format!(
                "{} · {param}",
                effect["effect"].as_str().unwrap_or("Effect")
            )
        }
        KeyframeTarget::Mask { mask, property, .. } => {
            let name = match property {
                MaskProperty::Opacity => {
                    data.scales[0] = 100.;
                    data.unit = Unit::Percent;
                    "Opacity"
                }
                MaskProperty::Feather => {
                    data.unit = Unit::Pixels;
                    "Feather"
                }
                MaskProperty::Expansion => {
                    data.unit = Unit::Pixels;
                    "Expansion"
                }
                MaskProperty::Node(_) => return Err(Unavailable::Unsupported),
            };
            format!("Mask {mask} · {name}")
        }
        KeyframeTarget::Audio { channel, .. } => {
            if count != 1 {
                return Err(Unavailable::Fault("audio track dimension mismatch".into()));
            }
            data.unit = Unit::Decibels;
            format!(
                "Audio {}",
                if *channel == AudioChannel::Left {
                    "L"
                } else {
                    "R"
                }
            )
        }
        KeyframeTarget::Vector {
            item, parameter, ..
        } => {
            data.unit = match parameter.as_str() {
                "position" | "anchor" | "size" | "width" | "stroke_width" | "dash_offset" => {
                    Unit::Pixels
                }
                "rotation" | "skew" | "skew_axis" | "trim_offset" => Unit::Degrees,
                "scale" | "opacity" | "start_opacity" | "end_opacity" | "trim_start"
                | "trim_end" | "start" | "end" => Unit::Percent,
                "color" | "fill" | "stroke_color" => Unit::Rgba,
                p if p.starts_with("dash_") => Unit::Pixels,
                _ => Unit::Native {
                    identity: parameter.clone(),
                    label: format!("value · {parameter}"),
                },
            };
            format!(
                "Vector {} · {parameter}",
                item.map_or_else(String::new, |i| i.to_string())
            )
        }
    };
    data.title = format!("{layer_name} · {title}");
    data.channel_title = title;
    Ok(data)
}
impl Data {
    fn knots(&self, start: f64, end: f64) -> impl Iterator<Item = f64> + '_ {
        let offset = self.offset as f64;
        let base: Box<dyn Iterator<Item = f64> + '_> = match &self.samples {
            Samples::Scalar(track) => Box::new(graph_sampling::knots(track, start, end, offset)),
            Samples::Vec2(track) => Box::new(graph_sampling::knots(track, start, end, offset)),
            Samples::Vec3(track) => Box::new(graph_sampling::knots(track, start, end, offset)),
            Samples::Vec4(track) => Box::new(graph_sampling::knots(track, start, end, offset)),
        };
        let steps = self
            .frames
            .iter()
            .flat_map(|frame| {
                let frame = f64::from(*frame);
                [frame.next_down(), frame]
            })
            .filter(move |frame| self.discrete && *frame >= start && *frame <= end);
        super::super::graph_adaptive::merge_knots(base, steps)
    }
    pub fn adaptive(
        &self,
        interval: [f64; 2],
        component: usize,
        fps: f64,
        speed: bool,
        scale: [f64; 2],
        tolerance: f64,
    ) -> Result<Vec<(f64, f32)>, super::super::graph_adaptive::Failure<motion_core::Error>> {
        super::super::graph_adaptive::sample(
            self.knots(interval[0], interval[1]),
            scale,
            tolerance,
            |frame| self.sample(frame, component, fps, speed).map(f64::from),
        )
        .map(|points| {
            points
                .into_iter()
                .map(|(frame, value)| (frame, value as f32))
                .collect()
        })
    }
    pub fn fitted(
        &self,
        interval: [f64; 2],
        component: usize,
        fps: f64,
        speed: bool,
        size: [f32; 2],
        tolerance: f64,
    ) -> Result<
        super::super::graph_ranges::Bounds,
        super::super::graph_adaptive::Failure<motion_core::Error>,
    > {
        use super::super::{
            graph_adaptive::{self, Failure},
            graph_ranges::Bounds,
            graph_view::Range,
        };
        let [start, end] = interval;
        let x = Range::fitted(start, end, 0.05, 1.).ok_or(Failure::InvalidGeometry)?;
        let [low, high] = graph_adaptive::extrema(self.knots(start, end), |frame| {
            self.sample(frame, component, fps, speed).map(f64::from)
        })?
        .ok_or(Failure::InvalidGeometry)?;
        let initial = Range::fitted(low, high, 0.1, 1.).ok_or(Failure::InvalidGeometry)?;
        let points = self.adaptive(
            interval,
            component,
            fps,
            speed,
            [
                f64::from(size[0]) / x.span(),
                f64::from(size[1]) / initial.span(),
            ],
            tolerance,
        )?;
        let low = points
            .iter()
            .map(|p| f64::from(p.1))
            .fold(f64::INFINITY, f64::min);
        let high = points
            .iter()
            .map(|p| f64::from(p.1))
            .fold(f64::NEG_INFINITY, f64::max);
        let y = Range::fitted(low, high, 0.1, 1.).ok_or(Failure::InvalidGeometry)?;
        Ok(Bounds { x, y })
    }
    pub fn value_limits(&self) -> Option<[f32; 2]> {
        self.limits
    }
    pub fn title(&self, component: usize) -> String {
        self.component_title(&self.title,component)
    }
    pub fn readout_title(&self, component: usize) -> String {
        self.component_title(&self.channel_title,component)
    }
    fn component_title(&self, title: &str, component: usize) -> String {
        if self.dimensions.len() > 1 {
            format!(
                "{} {}",
                title,
                if self.unit == Unit::Rgba {
                    ["R", "G", "B", "A"][component]
                } else {
                    ["X", "Y", "Z", "W"][component]
                }
            )
        } else {
            title.into()
        }
    }
    pub fn scale(&self, component: usize) -> f32 {
        self.scales[component]
    }
    pub fn sample(
        &self,
        frame: f64,
        component: usize,
        fps: f64,
        speed: bool,
    ) -> motion_core::Result<f32> {
        let local = frame - self.offset as f64;
        let (value, velocity) = match &self.samples {
            Samples::Scalar(t) => (
                t.sample(local)?,
                graph_sampling::velocity(t, local, fps, |v| v as f64)?,
            ),
            Samples::Vec2(t) => (
                t.sample(local)?[component],
                graph_sampling::velocity(t, local, fps, |v| v[component] as f64)?,
            ),
            Samples::Vec3(t) => (
                t.sample(local)?[component],
                graph_sampling::velocity(t, local, fps, |v| v[component] as f64)?,
            ),
            Samples::Vec4(t) if self.discrete => {
                let index = t
                    .keys
                    .partition_point(|k| f64::from(k.frame) <= local)
                    .saturating_sub(1);
                (
                    t.keys.get(index).map_or(t.value, |k| k.value)[component],
                    0.,
                )
            }
            Samples::Vec4(t) => (
                t.sample(local)?[component],
                graph_sampling::velocity(t, local, fps, |v| v[component] as f64)?,
            ),
        };
        let value = if speed {
            if self.discrete
                || self
                    .limits
                    .is_some_and(|[min, max]| value < min || value > max)
            {
                0.
            } else {
                velocity as f32
            }
        } else {
            self.limits
                .map_or(value, |[min, max]| value.clamp(min, max))
        };
        let value = value * self.scales[component];
        if !value.is_finite() {
            return Err(motion_core::Error::Invalid(
                "comparison sample is not finite".into(),
            ));
        }
        Ok(value)
    }
}
