//! Portable normalized timing, independent of property dimensions and units.
//! The selected key owns its outgoing interval. A terminal key has no interval.
use super::*;
use crate::{CurveSpace, Easing};

pub const CURVE_CLIPBOARD_VERSION: u32 = 1;
pub const MAX_CURVE_CLIPBOARD_BYTES: usize = 16 * 1024;
const FORMAT: &str = "motion_studio_easing";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveOrdinateUnit {
    NormalizedProgress,
    ProgressPerNormalizedTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveTimeUnit {
    NormalizedInterval,
}

/// No source addresses, physical speeds, property values or authored times are
/// transferred. A vector's channels share timing; separated axes are explicit
/// scalar destinations. All numeric channels supported by the typed keyframe
/// editor accept this dimensionless definition, including effect parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurveClipboard {
    pub format: String,
    pub version: u32,
    pub time_unit: CurveTimeUnit,
    pub ordinate_unit: CurveOrdinateUnit,
    pub easing: Easing,
}

impl CurveClipboard {
    pub fn new(easing: Easing) -> Result<Self> {
        let value = Self {
            format: FORMAT.into(),
            version: CURVE_CLIPBOARD_VERSION,
            time_unit: CurveTimeUnit::NormalizedInterval,
            ordinate_unit: unit(easing),
            easing,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        ensure(
            self.format == FORMAT,
            "not a Motion Studio easing clipboard",
        )?;
        ensure(
            self.version == CURVE_CLIPBOARD_VERSION,
            "unsupported easing clipboard version",
        )?;
        ensure(
            self.ordinate_unit == unit(self.easing),
            "easing clipboard units do not match curve space",
        )?;
        self.easing.validate()
    }

    pub fn to_text(&self) -> Result<String> {
        self.validate()?;
        let text = serde_json::to_string(self)?;
        ensure(
            text.len() <= MAX_CURVE_CLIPBOARD_BYTES,
            "easing clipboard exceeds metadata limit",
        )?;
        Ok(text)
    }

    pub fn from_text(text: &str) -> Result<Self> {
        ensure(
            text.len() <= MAX_CURVE_CLIPBOARD_BYTES,
            "easing clipboard exceeds metadata limit",
        )?;
        // Deserializer messages can quote arbitrary clipboard text or field
        // names. Keep category and exact location without reflecting payloads
        // into the application's default error logs.
        let value: Self = serde_json::from_str(text).map_err(|cause| {
            Error::Invalid(format!(
                "invalid easing clipboard JSON: category={:?} line={} column={}",
                cause.classify(),
                cause.line(),
                cause.column()
            ))
        })?;
        value.validate()?;
        Ok(value)
    }

    pub fn capture(project: &Project, selected: &KeyframeSelection) -> Result<Self> {
        ensure(
            selected.frames.len() == 1,
            "copy easing requires one source interval",
        )?;
        // Copy is read-only and remains available on a locked source layer.
        selected
            .target
            .signature(project)
            .map_err(|cause| interpolation::failure("copy_easing", selected, cause))?;
        let frame = project
            .edit_frame(selected.target.object(), selected.frames[0])
            .map_err(|cause| interpolation::failure("copy_easing", selected, cause))?;
        macro_rules! capture {
            ($track:expr) => {{
                let keys = &$track.keys;
                let index = keys
                    .binary_search_by_key(&frame, |key| key.frame)
                    .map_err(|_| Error::Invalid("source keyframe does not exist".into()))?;
                ensure(
                    index + 1 < keys.len(),
                    "source keyframe has no outgoing interval",
                )?;
                Self::new(Easing {
                    ease: keys[index].ease,
                    curve: keys[index].curve,
                })
            }};
        }
        (|| -> Result<Self> {
            match read(project, &selected.target)? {
                ReadChannel::Scalar(t) => capture!(t),
                ReadChannel::Vec2(t) => capture!(t),
                ReadChannel::Vec3(t) => capture!(t),
                ReadChannel::Vec4(t) => capture!(t),
                ReadChannel::Geometry(t) => capture!(t),
                ReadChannel::ColorCurve(t) => capture!(t),
            }
        })()
        .map_err(|cause| interpolation::failure("copy_easing", selected, cause))
    }

    /// Validate every address and outgoing interval before touching any channel.
    /// Selecting the final key along with earlier keys is harmless; selecting
    /// only terminal keys is rejected rather than reporting a successful paste.
    pub fn paste(&self, project: &mut Project, selections: &[KeyframeSelection]) -> Result<()> {
        self.validate()?;
        let frames = interpolation::preflight(project, selections, "paste_easing")?;
        let mut plans = Vec::with_capacity(selections.len());
        for (selected, frames) in selections.iter().zip(frames) {
            macro_rules! plan {
                ($track:expr) => {
                    outgoing(&$track.keys, &frames)
                };
            }
            let plan = (|| -> Result<Vec<i32>> {
                match read(project, &selected.target)? {
                    ReadChannel::Scalar(t) => plan!(t),
                    ReadChannel::Vec2(t) => plan!(t),
                    ReadChannel::Vec3(t) => plan!(t),
                    ReadChannel::Vec4(t) => plan!(t),
                    ReadChannel::Geometry(t) => plan!(t),
                    ReadChannel::ColorCurve(t) => plan!(t),
                }
            })()
            .map_err(|cause| interpolation::failure("paste_easing", selected, cause))?;
            plans.push(plan);
        }
        for (selected, frames) in selections.iter().zip(plans) {
            macro_rules! edit {
                ($track:expr) => {
                    for frame in frames {
                        $track.set_curve(frame, self.easing)?;
                    }
                };
            }
            (|| -> Result<()> {
                match write(project, &selected.target)? {
                    WriteChannel::Scalar(t) => edit!(t),
                    WriteChannel::Vec2(t) => edit!(t),
                    WriteChannel::Vec3(t) => edit!(t),
                    WriteChannel::Vec4(t) => edit!(t),
                    WriteChannel::Geometry(t) => edit!(t),
                    WriteChannel::ColorCurve(t) => {
                        for frame in frames {
                            let index = t
                                .keys
                                .binary_search_by_key(&frame, |key| key.frame)
                                .map_err(|_| {
                                    Error::Invalid("keyframe not found after preflight".into())
                                })?;
                            t.keys[index].ease = self.easing.ease;
                            t.keys[index].curve = self.easing.curve;
                        }
                    }
                }
                Ok(())
            })()
            .map_err(|cause| interpolation::failure("paste_easing", selected, cause))?;
        }
        Ok(())
    }
}

fn unit(easing: Easing) -> CurveOrdinateUnit {
    if easing
        .curve
        .is_some_and(|curve| curve.space == CurveSpace::Velocity)
    {
        CurveOrdinateUnit::ProgressPerNormalizedTime
    } else {
        CurveOrdinateUnit::NormalizedProgress
    }
}

fn outgoing<T>(keys: &[Keyframe<T>], frames: &[i32]) -> Result<Vec<i32>> {
    let mut result = Vec::with_capacity(frames.len());
    for frame in frames {
        let index = keys
            .binary_search_by_key(frame, |key| key.frame)
            .map_err(|_| Error::Invalid("selected keyframe does not exist".into()))?;
        if index + 1 < keys.len() {
            result.push(*frame);
        }
    }
    ensure(
        !result.is_empty(),
        "selected keys have no outgoing intervals",
    )?;
    Ok(result)
}
