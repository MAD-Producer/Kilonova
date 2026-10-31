//! A temporal handle belongs to one endpoint, rather than the entire interval.
use super::*;
use crate::{Curve, CurveShape, CurveSpace, Easing};

fn interval<T>(keys: &[Keyframe<T>], frame: i32) -> Result<usize> {
    let index = keys
        .binary_search_by_key(&frame, |key| key.frame)
        .map_err(|_| Error::Invalid("curve interval start key is missing".into()))?;
    ensure(
        index + 1 < keys.len(),
        "curve handle requires a following keyframe",
    )?;
    Ok(index)
}

fn edited(easing: Easing, incoming: bool, control: [f64; 2]) -> Result<Easing> {
    let (mut control1, mut control2) = timing::cubic(easing, false)?;
    if incoming {
        control2 = control;
    } else {
        control1 = control;
    }
    let result = Easing {
        ease: easing.ease,
        curve: Some(Curve {
            space: CurveSpace::Progress,
            shape: CurveShape::Cubic {
                control1,
                control2,
                start: 0.,
                end: 1.,
            },
        }),
    };
    result.validate()?;
    Ok(result)
}

/// Resolve current easing at application time. The caller cannot replace the
/// opposite endpoint by returning an old interval snapshot from a drag.
pub fn set_handle(
    project: &mut Project,
    target: &KeyframeTarget,
    frame: u32,
    incoming: bool,
    control: [f64; 2],
) -> Result<()> {
    let selected = KeyframeSelection {
        target: target.clone(),
        frames: vec![frame],
    };
    let frames =
        interpolation::preflight(project, std::slice::from_ref(&selected), "curve_handle")?;
    let frame = frames[0][0];
    (|| -> Result<()> {
        if let KeyframeTarget::Effect {
            object,
            effect,
            param,
        } = target
        {
            let parameter = project
                .layers
                .iter()
                .find(|l| l.id == *object)
                .and_then(|l| l.effects.iter().find(|e| e.id == *effect))
                .and_then(|e| e.params.get(param))
                .ok_or_else(|| Error::Invalid("curve handle effect parameter missing".into()))?;
            ensure(
                !parameter.kind.discrete(),
                "boolean and enum parameters have no temporal handles",
            )?;
        }
        macro_rules! apply {
            ($track:expr, $variant:ident) => {{
                let index = interval(&$track.keys, frame)?;
                let easing = edited($track.effective_easing(index)?, incoming, control)?;
                let mut next = $track.clone();
                next.set_curve_sides(frame, easing, !incoming, incoming)?;
                let WriteChannel::$variant(destination) = write(project, target)? else {
                    return Err(Error::Invalid(
                        "curve handle channel changed after preflight".into(),
                    ));
                };
                *destination = next;
            }};
        }
        match read(project, target)? {
            ReadChannel::Scalar(track) => apply!(track, Scalar),
            ReadChannel::Vec2(track) => apply!(track, Vec2),
            ReadChannel::Vec3(track) => apply!(track, Vec3),
            ReadChannel::Vec4(track) => apply!(track, Vec4),
            ReadChannel::Geometry(track) => apply!(track, Geometry),
            ReadChannel::ColorCurve(track) => {
                let index = interval(&track.keys, frame)?;
                let easing = edited(track.effective_easing(index)?, incoming, control)?;
                let WriteChannel::ColorCurve(destination) = write(project, target)? else {
                    return Err(Error::Invalid(
                        "curve handle channel changed after preflight".into(),
                    ));
                };
                destination.keys[index].ease = easing.ease;
                destination.keys[index].curve = easing.curve;
            }
        }
        Ok(())
    })()
    .map_err(|cause| interpolation::failure("curve_handle", &selected, cause))
}
