//! Preflight every channel and solve candidate tracks before installing any.
use super::*;

enum Plan {
    Scalar(Track<f32>),
    Vec2(Track<[f32; 2]>),
    Vec3(Track<[f32; 3]>),
    Vec4(Track<[f32; 4]>),
}

pub fn set_automatic(project: &mut Project, selections: &[KeyframeSelection], enabled: bool) -> Result<()> {
    let local = interpolation::preflight(project, selections, "automatic_bezier")?;
    let mut plans = Vec::with_capacity(selections.len());
    for (selected, frames) in selections.iter().zip(local) {
        let candidate = (|| -> Result<Plan> {
            if let KeyframeTarget::Effect { object, effect, param } = &selected.target {
                let parameter = project.layers.iter().find(|l| l.id == *object)
                    .and_then(|l| l.effects.iter().find(|e| e.id == *effect))
                    .and_then(|e| e.params.get(param))
                    .ok_or_else(|| Error::Invalid("automatic Bezier effect parameter missing".into()))?;
                ensure(!parameter.kind.discrete(), "boolean and enum parameters support hold interpolation only")?;
            }
            macro_rules! plan {
                ($track:expr, $variant:ident) => {{
                    let mut track = $track.clone();
                    for frame in frames { track.set_auto_bezier(frame, enabled)?; }
                    Plan::$variant(track)
                }};
            }
            Ok(match read(project, &selected.target)? {
                ReadChannel::Scalar(t) => plan!(t, Scalar),
                ReadChannel::Vec2(t) => plan!(t, Vec2),
                ReadChannel::Vec3(t) => plan!(t, Vec3),
                ReadChannel::Vec4(t) => plan!(t, Vec4),
                ReadChannel::Geometry(_) => return Err(Error::Invalid("path geometry does not support automatic temporal Bezier".into())),
                ReadChannel::ColorCurve(_) => return Err(Error::Invalid("color transfer curves do not support automatic temporal Bezier".into())),
            })
        })().map_err(|cause| interpolation::failure("automatic_bezier", selected, cause))?;
        plans.push(candidate);
    }
    // Addresses, locks and values have not changed since preflight. Writes
    // cannot re-run a solver or partially apply a capability rejection.
    for (selected, plan) in selections.iter().zip(plans) {
        match (write(project, &selected.target)?, plan) {
            (WriteChannel::Scalar(t), Plan::Scalar(next)) => *t = next,
            (WriteChannel::Vec2(t), Plan::Vec2(next)) => *t = next,
            (WriteChannel::Vec3(t), Plan::Vec3(next)) => *t = next,
            (WriteChannel::Vec4(t), Plan::Vec4(next)) => *t = next,
            _ => return Err(interpolation::failure("automatic_bezier", selected,
                Error::Invalid("automatic Bezier channel changed after preflight".into()))),
        }
    }
    Ok(())
}
