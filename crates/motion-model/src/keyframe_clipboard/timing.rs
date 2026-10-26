//! Endpoint timing edits address incident intervals, not displayed graph modes.
//! Each channel is fully planned before any authored metadata changes.
use super::*;
use crate::{Curve, CurveShape, CurveSpace, Ease, Easing};
use std::collections::BTreeMap;

mod units;
use units::{Metric, Numeric};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VelocityUnits {
    #[default]
    PropertyPerSecond,
    /// Multiple of displacement / duration; one means interval-average velocity.
    AverageMultiple,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingInterpolation {
    #[default]
    Preserve,
    Linear,
    Hold,
    Bezier,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingSide {
    /// None preserves this endpoint's existing derivative.
    pub speed: Option<f64>,
    /// Percent of the incident interval's duration; None preserves influence.
    pub influence: Option<f64>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingSettings {
    pub interpolation: TimingInterpolation,
    pub units: VelocityUnits,
    /// None leaves the respective incident interval endpoint untouched.
    pub incoming: Option<TimingSide>,
    pub outgoing: Option<TimingSide>,
    /// A one-time rate match, not a persistent constraint on later edits.
    pub match_speed: bool,
}
impl TimingSettings {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.incoming.is_some() || self.outgoing.is_some(),
            "choose a keyframe timing side",
        )?;
        let mut changed = self.interpolation != TimingInterpolation::Preserve;
        for side in self.incoming.iter().chain(self.outgoing.iter()) {
            if let Some(speed) = side.speed {
                ensure(speed.is_finite(), "keyframe speed must be finite")?;
                changed = true;
            }
            if let Some(influence) = side.influence {
                ensure(
                    influence.is_finite() && influence > 0. && influence <= 100.,
                    "keyframe influence must be greater than zero and at most 100 percent",
                )?;
                changed = true;
            }
            ensure(
                !matches!(
                    self.interpolation,
                    TimingInterpolation::Linear | TimingInterpolation::Hold
                ) || side.speed.is_none() && side.influence.is_none(),
                "linear and hold interpolation cannot set Bezier speed or influence",
            )?;
        }
        if self.match_speed {
            ensure(
                self.units == VelocityUnits::PropertyPerSecond,
                "matching endpoint rates requires property units per second",
            )?;
            let (Some(a), Some(b)) = (self.incoming, self.outgoing) else {
                return Err(Error::Invalid(
                    "matching endpoint rates requires both sides".into(),
                ));
            };
            ensure(
                a.speed.is_some() && a.speed == b.speed,
                "matching endpoint rates requires equal explicit speeds",
            )?;
        }
        ensure(changed, "no keyframe timing changes requested")
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SideProfile {
    pub intervals: usize,
    pub speed: Option<f64>,
    pub influence: Option<f64>,
    pub progress_editable: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct TimingProfile {
    pub channels: usize,
    pub keys: usize,
    pub units: BTreeSet<String>,
    pub incoming: SideProfile,
    pub outgoing: SideProfile,
    pub numeric: bool,
    pub vector: bool,
    pub temporal: bool,
    pub progress_editable: bool,
    pub curve_spaces: BTreeSet<String>,
}

impl SideProfile {
    fn add(&mut self, speed: Option<f64>, influence: Option<f64>, editable: bool) {
        if self.intervals == 0 {
            self.speed = speed;
            self.influence = influence;
            self.progress_editable = editable;
        } else {
            self.progress_editable &= editable;
            if self.speed != speed {
                self.speed = None;
            }
            if self.influence != influence {
                self.influence = None;
            }
        }
        self.intervals += 1;
    }
}

fn inspect<T: Numeric>(
    keys: &[Keyframe<T>],
    frames: &[i32],
    metric: &Metric,
    fps: f64,
    units: VelocityUnits,
    profile: &mut TimingProfile,
) -> Result<()> {
    for frame in frames {
        let index = keys
            .binary_search_by_key(frame, |k| k.frame)
            .map_err(|_| Error::Invalid("selected keyframe does not exist".into()))?;
        for (interval, incoming) in [
            (index.checked_sub(1), true),
            ((index + 1 < keys.len()).then_some(index), false),
        ] {
            let Some(index) = interval else { continue };
            let a = &keys[index];
            let b = &keys[index + 1];
            let easing = Easing {
                ease: a.ease,
                curve: a.curve,
            };
            let handles = match easing.curve {
                Some(Curve {
                    space: CurveSpace::Progress,
                    shape: CurveShape::Cubic { .. } | CurveShape::Quadratic { .. },
                }) => Some(cubic(easing, false)?),
                None if easing.ease != Ease::Hold => Some(cubic(easing, false)?),
                _ => None, // Unsupported representation is a normal capability result.
            };
            profile.progress_editable &= handles.is_some();
            profile.curve_spaces.insert(
                match easing.curve {
                    None => "preset",
                    Some(Curve {
                        space: CurveSpace::Progress,
                        ..
                    }) => "progress",
                    Some(Curve {
                        space: CurveSpace::Velocity,
                        ..
                    }) => "velocity",
                }
                .into(),
            );
            let slope = easing.sample(if incoming { 1. } else { 0. }).velocity;
            let speed = if !metric.temporal {
                None
            } else if !metric.numeric {
                (units == VelocityUnits::AverageMultiple).then_some(slope)
            } else {
                let (baseline, average) = endpoint(a, b, incoming, metric, fps)?;
                let speed = slope * baseline;
                if units == VelocityUnits::AverageMultiple {
                    (average != 0.).then(|| speed / average)
                } else {
                    Some(if metric.dimensions > 1 {
                        speed.abs()
                    } else {
                        speed
                    })
                }
            };
            ensure(
                speed.is_none_or(|v| v.is_finite()),
                "keyframe velocity overflow",
            )?;
            let influence = handles.map(|(p, q)| {
                if incoming {
                    (1. - q[0]) * 100.
                } else {
                    p[0] * 100.
                }
            });
            if incoming {
                profile.incoming.add(speed, influence, handles.is_some())
            } else {
                profile.outgoing.add(speed, influence, handles.is_some())
            }
        }
    }
    Ok(())
}

/// Called once when opening a draft or changing its units, never on playback.
pub fn profile(
    project: &Project,
    selections: &[KeyframeSelection],
    units: VelocityUnits,
) -> Result<TimingProfile> {
    let frames = interpolation::preflight(project, selections, "timing_profile")?;
    let mut profile = TimingProfile {
        numeric: true,
        temporal: true,
        progress_editable: true,
        ..Default::default()
    };
    for (selected, frames) in selections.iter().zip(frames) {
        (|| -> Result<()> {
            let metric = units::metric(project, &selected.target)?;
            profile.channels += 1;
            profile.keys += frames.len();
            profile.numeric &= metric.numeric;
            profile.vector |= metric.numeric && metric.dimensions > 1;
            profile.temporal &= metric.temporal;
            profile
                .units
                .insert(if units == VelocityUnits::AverageMultiple {
                    "x average".into()
                } else {
                    metric.label.clone()
                });
            macro_rules! inspect {
                ($t:expr) => {
                    inspect(
                        &$t.keys,
                        &frames,
                        &metric,
                        f64::from(project.fps),
                        units,
                        &mut profile,
                    )
                };
            }
            match read(project, &selected.target)? {
                ReadChannel::Scalar(t) => inspect!(t),
                ReadChannel::Vec2(t) => inspect!(t),
                ReadChannel::Vec3(t) => inspect!(t),
                ReadChannel::Vec4(t) => inspect!(t),
                ReadChannel::Geometry(t) => inspect!(t),
                ReadChannel::ColorCurve(t) => inspect!(t),
            }
        })()
        .map_err(|cause| interpolation::failure("timing_profile", selected, cause))?;
    }
    Ok(profile)
}

fn cubic(easing: Easing, explicit_conversion: bool) -> Result<([f64; 2], [f64; 2])> {
    match easing.curve {
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic {
                control1, control2, ..
            },
        }) => Ok((control1, control2)),
        Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Quadratic { control, .. },
        }) => Ok((
            [control[0] * 2. / 3., control[1] * 2. / 3.],
            [
                1. / 3. + control[0] * 2. / 3.,
                1. / 3. + control[1] * 2. / 3.,
            ],
        )),
        _ => {
            ensure(
                explicit_conversion || easing.curve.is_none() && easing.ease != Ease::Hold,
                "choose Bezier explicitly to replace hold, velocity-space or elastic interpolation",
            )?;
            let a = easing.sample(0.).velocity;
            let b = easing.sample(1.).velocity;
            ensure(
                a.is_finite() && b.is_finite(),
                "existing endpoint derivative is not finite",
            )?;
            Ok(([1. / 3., a / 3.], [2. / 3., 1. - b / 3.]))
        }
    }
}

fn endpoint<T: Numeric>(
    a: &Keyframe<T>,
    b: &Keyframe<T>,
    incoming: bool,
    metric: &Metric,
    fps: f64,
) -> Result<(f64, f64)> {
    let seconds = (f64::from(b.frame) - f64::from(a.frame)) / fps;
    ensure(
        seconds.is_finite() && seconds > 0.,
        "keyframe interval has invalid duration",
    )?;
    let average = metric.difference(&a.value, &b.value)? / seconds;
    let tangent = if incoming {
        b.spatial.as_ref().and_then(|s| s.incoming.as_ref())
    } else {
        a.spatial.as_ref().and_then(|s| s.outgoing.as_ref())
    };
    let baseline = if let Some(tangent) = tangent {
        metric.tangent(tangent, if incoming { -3. } else { 3. })? / seconds
    } else {
        average
    };
    Ok((baseline, average))
}

fn intervals<T: Numeric>(
    keys: &[Keyframe<T>],
    frames: &[i32],
    metric: &Metric,
    fps: f64,
    settings: &TimingSettings,
) -> Result<Vec<(i32, Easing)>> {
    let mut sides: BTreeMap<usize, (Option<TimingSide>, Option<TimingSide>)> = BTreeMap::new();
    let changed = |side: Option<TimingSide>| {
        side.filter(|side| {
            settings.interpolation != TimingInterpolation::Preserve
                || side.speed.is_some()
                || side.influence.is_some()
        })
    };
    for frame in frames {
        let i = keys
            .binary_search_by_key(frame, |k| k.frame)
            .map_err(|_| Error::Invalid("selected keyframe does not exist".into()))?;
        if let (Some(side), Some(previous)) = (changed(settings.incoming), i.checked_sub(1)) {
            sides.entry(previous).or_default().0 = Some(side);
        }
        if i + 1 < keys.len() {
            if let Some(side) = changed(settings.outgoing) {
                sides.entry(i).or_default().1 = Some(side);
            }
        }
    }
    let mut plan = Vec::with_capacity(sides.len());
    for (index, (incoming, outgoing)) in sides {
        let a = &keys[index];
        let b = &keys[index + 1];
        let mut easing = Easing {
            ease: a.ease,
            curve: a.curve,
        };
        match settings.interpolation {
            TimingInterpolation::Linear => {
                easing = Easing {
                    ease: Ease::Linear,
                    curve: None,
                }
            }
            TimingInterpolation::Hold => {
                easing = Easing {
                    ease: Ease::Hold,
                    curve: None,
                }
            }
            TimingInterpolation::Preserve | TimingInterpolation::Bezier => {
                let (mut p, mut q) = cubic(
                    easing,
                    settings.interpolation == TimingInterpolation::Bezier,
                )?;
                for (side, is_incoming) in [(outgoing, false), (incoming, true)] {
                    let Some(side) = side else { continue };
                    let (old_x, old_y) = if is_incoming {
                        (1. - q[0], 1. - q[1])
                    } else {
                        (p[0], p[1])
                    };
                    let mut slope = if old_x > 0. {
                        old_y / old_x
                    } else {
                        easing.sample(if is_incoming { 1. } else { 0. }).velocity
                    };
                    if let Some(speed) = side.speed {
                        ensure(
                            settings.units != VelocityUnits::PropertyPerSecond
                                || metric.dimensions == 1
                                || speed >= 0.,
                            "vector speed must be nonnegative; use average multiples to reverse progress",
                        )?;
                        let reverse = settings.units == VelocityUnits::PropertyPerSecond
                            && metric.dimensions > 1
                            && slope < 0.;
                        if settings.units == VelocityUnits::AverageMultiple && !metric.numeric {
                            slope = speed;
                        } else {
                            let (baseline, average) = endpoint(a, b, is_incoming, metric, fps)?;
                            let desired = if settings.units == VelocityUnits::AverageMultiple {
                                speed * average
                            } else {
                                speed
                            };
                            ensure(
                                baseline != 0. || desired == 0.,
                                "stationary keyframe endpoint cannot have nonzero speed",
                            )?;
                            slope = if baseline == 0. {
                                0.
                            } else {
                                desired / baseline * if reverse { -1. } else { 1. }
                            };
                        }
                    }
                    let influence = side.influence.map_or(old_x, |v| v / 100.);
                    ensure(
                        slope.is_finite() && influence > 0.,
                        "endpoint speed or influence cannot be represented by finite Bezier handles",
                    )?;
                    if is_incoming {
                        q = [1. - influence, 1. - slope * influence];
                    } else {
                        p = [influence, slope * influence];
                    }
                }
                easing = Easing {
                    ease: Ease::Linear,
                    curve: Some(Curve {
                        space: CurveSpace::Progress,
                        shape: CurveShape::Cubic {
                            control1: p,
                            control2: q,
                            start: 0.,
                            end: 1.,
                        },
                    }),
                };
            }
        }
        easing.validate()?;
        plan.push((a.frame, easing));
    }
    Ok(plan)
}

pub fn configure(
    project: &mut Project,
    selections: &[KeyframeSelection],
    settings: &TimingSettings,
) -> Result<()> {
    settings.validate().map_err(|cause| {
        if let Some(selected) = selections.first() {
            interpolation::failure("timing", selected, cause)
        } else {
            cause
        }
    })?;
    let local = interpolation::preflight(project, selections, "timing")?;
    let mut plans = Vec::with_capacity(selections.len());
    let mut count = 0usize;
    for (selected, frames) in selections.iter().zip(local) {
        let result = (|| -> Result<_> {
            let metric = units::metric(project, &selected.target)?;
            ensure(
                metric.temporal || settings.interpolation == TimingInterpolation::Hold,
                "boolean and enum keyframes support hold interpolation only",
            )?;
            ensure(
                metric.numeric
                    || settings.units == VelocityUnits::AverageMultiple
                    || settings
                        .incoming
                        .iter()
                        .chain(settings.outgoing.iter())
                        .all(|s| s.speed.is_none()),
                "property-unit speed is unavailable for this keyframe type",
            )?;
            macro_rules! plan {
                ($t:expr) => {
                    intervals(&$t.keys, &frames, &metric, f64::from(project.fps), settings)
                };
            }
            match read(project, &selected.target)? {
                ReadChannel::Scalar(t) => plan!(t),
                ReadChannel::Vec2(t) => plan!(t),
                ReadChannel::Vec3(t) => plan!(t),
                ReadChannel::Vec4(t) => plan!(t),
                ReadChannel::Geometry(t) => plan!(t),
                ReadChannel::ColorCurve(t) => plan!(t),
            }
        })()
        .map_err(|cause| interpolation::failure("timing", selected, cause))?;
        count += result.len();
        plans.push(result);
    }
    ensure(
        count > 0,
        "selected keyframes have no incident interval on the chosen sides",
    )?;
    for (selected, plan) in selections.iter().zip(plans) {
        let result = (|| -> Result<()> {
            macro_rules! apply {
                ($t:expr) => {
                    for (frame, easing) in plan {
                        $t.set_curve(frame, easing)?;
                    }
                };
            }
            match write(project, &selected.target)? {
                WriteChannel::Scalar(t) => apply!(t),
                WriteChannel::Vec2(t) => apply!(t),
                WriteChannel::Vec3(t) => apply!(t),
                WriteChannel::Vec4(t) => apply!(t),
                WriteChannel::Geometry(t) => apply!(t),
                WriteChannel::ColorCurve(t) => {
                    for (frame, easing) in plan {
                        let index =
                            t.keys
                                .binary_search_by_key(&frame, |k| k.frame)
                                .map_err(|_| {
                                    Error::Invalid("keyframe changed after timing preflight".into())
                                })?;
                        t.keys[index].ease = easing.ease;
                        t.keys[index].curve = easing.curve;
                    }
                }
            }
            Ok(())
        })();
        result.map_err(|cause| interpolation::failure("timing", selected, cause))?;
    }
    Ok(())
}
