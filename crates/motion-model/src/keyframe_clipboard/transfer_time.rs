//! Clipboard identity stays authored; only transfer coordinates are continuous.
use super::*;
use crate::layer_time::{LayerClock, TimeDirection};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalKeyframeSelection {
    pub target: KeyframeTarget,
    pub frames: Vec<i32>,
}

/// Composition packets retain the existing integer transfer representation.
/// Local packets freeze the source clock and signed storage addresses together.
/// Neither kind needs to round a displayed key into a different identity.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KeyframeTimeSpace {
    Composition,
    Local {
        clock: LayerClock,
        offset_frame: i32,
    },
}
impl KeyframeTimeSpace {
    pub(super) fn composition(self, frame: i32) -> Result<f64> {
        match self {
            Self::Composition => Ok(f64::from(frame)),
            Self::Local {
                clock,
                offset_frame,
            } => clock.projected_composition_frame(f64::from(frame), offset_frame),
        }
    }
    fn reversed(self) -> bool {
        matches!(self, Self::Local { clock, .. } if clock.direction() == TimeDirection::Reverse)
    }
    fn local(self, composition: f64) -> Result<i32> {
        let (local, exact) = match self {
            Self::Composition => (composition, composition.round() == composition),
            Self::Local {
                clock,
                offset_frame,
            } => {
                let local = clock.local_frame(composition, offset_frame)?;
                // Recognize an exact forward projection even when its inverse
                // has arithmetic noise. No epsilon, snapping, or grid merging.
                let exact = local.fract() == 0.
                    || clock.projected_composition_frame(local.round(), offset_frame)?
                        == composition;
                (local, exact)
            }
        };
        ensure(
            exact,
            "clipboard destination time requires continuous key addressing",
        )?;
        ensure(
            local.round() >= f64::from(i32::MIN) && local.round() <= f64::from(i32::MAX),
            "clipboard destination local time overflow",
        )?;
        Ok(local.round() as i32)
    }
}
pub(super) fn destination(project: &Project, target: &KeyframeTarget) -> Result<KeyframeTimeSpace> {
    if target.object() == 0 {
        return Ok(KeyframeTimeSpace::Composition);
    }
    let layer = project
        .layers
        .iter()
        .find(|l| l.id == target.object())
        .ok_or(Error::Missing(target.object()))?;
    Ok(KeyframeTimeSpace::Local {
        clock: layer
            .time_mapping
            .as_ref()
            .map_or_else(LayerClock::default, |m| m.clock),
        offset_frame: layer.clip(project.frames).offset_frame,
    })
}

fn capture_local<T: Clone>(
    keys: &[Keyframe<T>],
    selected: &LocalKeyframeSelection,
    time_space: KeyframeTimeSpace,
    frames: u32,
) -> Result<Vec<Keyframe<T>>> {
    let mut local = selected.frames.clone();
    local.sort_unstable();
    ensure(
        !local.is_empty()
            && local.len() <= crate::MAX_FRAMES as usize
            && local.windows(2).all(|w| w[0] < w[1]),
        "clipboard keys must be nonempty and unique",
    )?;
    local
        .into_iter()
        .map(|frame| {
            let composition = time_space.composition(frame)?;
            ensure(
                (0. ..f64::from(frames)).contains(&composition),
                "clipboard key outside composition",
            )?;
            keys.binary_search_by_key(&frame, |key| key.frame)
                .ok()
                .and_then(|index| keys.get(index))
                .cloned()
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "clipboard source local key missing: object={} frame={frame}",
                        selected.target.object()
                    ))
                })
        })
        .collect()
}
impl KeyframeClipboard {
    /// Both coordinate kinds share one packet, metadata budget and transaction.
    pub fn capture_mixed(
        project: &Project,
        composition: &[KeyframeSelection],
        local: &[LocalKeyframeSelection],
    ) -> Result<Self> {
        ensure(
            !composition.is_empty() || !local.is_empty(),
            "invalid clipboard track count",
        )?;
        ensure(
            crate::serialized_bytes(composition).saturating_add(crate::serialized_bytes(local))
                <= MAX_KEYFRAME_CLIPBOARD_BYTES,
            "keyframe clipboard selection exceeds metadata limit",
        )?;
        let mut result = if composition.is_empty() {
            Self { tracks: vec![] }
        } else {
            Self::capture(project, composition)?
        };
        let mut bytes = crate::serialized_bytes(&result);
        for selected in local {
            let track = (|| -> Result<KeyframeClipTrack> {
                let time_space = destination(project, &selected.target)?;
                macro_rules! data {
                    ($variant:ident,$keys:expr) => {
                        KeyframeData::$variant(capture_local(
                            $keys,
                            selected,
                            time_space,
                            project.frames,
                        )?)
                    };
                }
                let data = match read(project, &selected.target)? {
                    ReadChannel::Scalar(t) => data!(Scalar, &t.keys),
                    ReadChannel::Vec2(t) => data!(Vec2, &t.keys),
                    ReadChannel::Vec3(t) => data!(Vec3, &t.keys),
                    ReadChannel::Vec4(t) => data!(Vec4, &t.keys),
                    ReadChannel::Geometry(t) => data!(Geometry, &t.keys),
                    ReadChannel::ColorCurve(t) => data!(ColorCurve, &t.keys),
                };
                Ok(KeyframeClipTrack {
                    source: selected.target.clone(),
                    signature: selected.target.signature(project)?,
                    time_space,
                    data,
                })
            })()
            .map_err(|cause| selected.target.failure("capture_local", cause))?;
            bytes = bytes.saturating_add(crate::serialized_bytes(&track));
            ensure(
                bytes <= MAX_KEYFRAME_CLIPBOARD_BYTES,
                "keyframe clipboard exceeds metadata limit",
            )?;
            result.tracks.push(track);
        }
        result.validate()?;
        Ok(result)
    }
}

fn reverse_easing(easing: crate::Easing) -> Result<crate::Easing> {
    use crate::{Curve, CurveShape, CurveSpace, Ease};
    let curve = if let Some(curve) = easing.curve {
        let point = |p: [f64; 2]| {
            [
                1. - p[0],
                if curve.space == CurveSpace::Progress {
                    1. - p[1]
                } else {
                    p[1]
                },
            ]
        };
        let (start, end) = if curve.space == CurveSpace::Progress {
            (0., 1.)
        } else {
            match curve.shape {
                CurveShape::Quadratic { start, end, .. } | CurveShape::Cubic { start, end, .. } => {
                    (end, start)
                }
                _ => {
                    return Err(Error::Invalid(
                        "reversed clipboard elastic interpolation is not representable".into(),
                    ))
                }
            }
        };
        let shape = match curve.shape {
            CurveShape::Quadratic { control, .. } => CurveShape::Quadratic {
                control: point(control),
                start,
                end,
            },
            CurveShape::Cubic {
                control1, control2, ..
            } => CurveShape::Cubic {
                control1: point(control2),
                control2: point(control1),
                start,
                end,
            },
            CurveShape::Elastic { .. } => {
                return Err(Error::Invalid(
                    "reversed clipboard elastic interpolation is not representable".into(),
                ))
            }
        };
        Some(Curve {
            space: curve.space,
            shape,
        })
    } else {
        None
    };
    let ease = if curve.is_some() {
        easing.ease
    } else {
        match easing.ease {
            Ease::In => Ease::Out,
            Ease::Out => Ease::In,
            Ease::Hold => {
                return Err(Error::Invalid(
                    "reversed clipboard hold interpolation requires an opposite-sided step".into(),
                ))
            }
            other => other,
        }
    };
    let result = crate::Easing { ease, curve };
    result.validate()?;
    Ok(result)
}
pub(super) fn move_keys<T: Clone>(
    keys: &[Keyframe<T>],
    source: KeyframeTimeSpace,
    destination: KeyframeTimeSpace,
    earliest: f64,
    at: Option<f64>,
    frames: u32,
) -> Result<Vec<Keyframe<T>>> {
    let mut moved = keys.to_vec();
    for key in &mut moved {
        let original = source.composition(key.frame)?;
        let composition = at.map_or(original, |at| (original - earliest) + at);
        ensure(
            (0. ..f64::from(frames)).contains(&composition),
            "clipboard destination key outside composition",
        )?;
        key.frame = destination.local(composition)?;
    }
    if source.reversed() != destination.reversed() {
        moved.reverse();
        for (i, key) in moved.iter_mut().enumerate() {
            if let Some(spatial) = key.spatial.as_mut() {
                std::mem::swap(&mut spatial.incoming, &mut spatial.outgoing);
            }
            let original = if i + 1 < keys.len() {
                &keys[keys.len() - 2 - i]
            } else {
                keys.last().expect("nonempty validated clipboard")
            };
            let easing = crate::Easing {
                ease: original.ease,
                curve: original.curve,
            };
            let easing = if i + 1 < keys.len() {
                reverse_easing(easing)?
            } else {
                easing
            };
            key.ease = easing.ease;
            key.curve = easing.curve;
            key.auto_bezier = original.auto_bezier;
        }
    }
    ensure(
        moved.windows(2).all(|w| w[0].frame < w[1].frame),
        "clipboard destination keys collide after time mapping",
    )?;
    Ok(moved)
}

pub(super) fn cut_selections(
    project: &Project,
    clipboard: &KeyframeClipboard,
) -> Result<Vec<LocalKeyframeSelection>> {
    let mut selections = vec![];
    for track in &clipboard.tracks {
        let selected = (|| -> Result<LocalKeyframeSelection> {
            let frames = key_data!(&track.data,k=>k.iter().map(|k|k.frame).collect::<Vec<_>>());
            let (current, local) = match track.time_space {
                KeyframeTimeSpace::Composition => {
                    let selected = KeyframeSelection {
                        target: track.source.clone(),
                        frames: frames.iter().map(|f| *f as u32).collect(),
                    };
                    let local = selected
                        .frames
                        .iter()
                        .map(|f| project.edit_frame(selected.target.object(), *f))
                        .collect::<Result<Vec<_>>>()?;
                    (KeyframeClipboard::capture(project, &[selected])?, local)
                }
                KeyframeTimeSpace::Local { .. } => {
                    let selected = LocalKeyframeSelection {
                        target: track.source.clone(),
                        frames: frames.clone(),
                    };
                    (
                        KeyframeClipboard::capture_mixed(project, &[], &[selected])?,
                        frames,
                    )
                }
            };
            ensure(
                current.tracks.first() == Some(track),
                "clipboard cut source changed",
            )?;
            Ok(LocalKeyframeSelection {
                target: track.source.clone(),
                frames: local,
            })
        })()
        .map_err(|cause| track.source.failure("cut", cause))?;
        selections.push(selected);
    }
    Ok(selections)
}
