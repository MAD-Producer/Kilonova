//! Preflight both channels before publishing compensated anchor edits.
use super::*;

pub fn compensate_anchor(
    project: &mut Project,
    object: u64,
    frame: f64,
    anchor: [f32; 2],
) -> Result<()> {
    let target = KeyframeTarget::Property {
        object,
        property: crate::Property::Anchor,
        axis: None,
    };
    let result = (|| {
        target.editable(project)?;
        ensure(
            anchor.iter().all(|v| v.is_finite() && v.abs() <= 1000.),
            "invalid anchor",
        )?;
        let time = EditTime::read(project, object, frame)?;
        let layer = project
            .layers
            .iter()
            .find(|layer| layer.id == object)
            .ok_or(Error::Missing(object))?;
        ensure(
            layer.camera_data().is_none() && !matches!(layer.content, Content::Audio { .. }),
            "object has no layer anchor",
        )?;
        let previous = layer.transform.anchor.sample(time.local)?;
        let mut rotation = layer.transform.rotation.sample(time.local)?;
        if !layer.three_d {
            rotation[0] = 0.;
            rotation[1] = 0.;
        }
        let delta = crate::geometry::rotation_quaternion(rotation, layer.rotation_order)
            * (glam::Vec3::from_array(layer.transform.scale.sample(time.local)?) / 100.
                * glam::Vec3::new(
                    (anchor[0] - previous[0]) * layer.size[0],
                    (previous[1] - anchor[1]) * layer.size[1],
                    0.,
                ));
        let position = layer.transform.position.sample(time.local)?;
        let position = [
            position[0] + delta.x,
            position[1] - delta.y,
            position[2] - delta.z,
        ];
        let next_anchor = edit_track(
            &layer.transform.anchor,
            &time,
            &TrackEdit::Set {
                value: TrackValue::Vec2(anchor),
            },
            Some(anchor),
            false,
        )
        .map_err(|source| target.failure("anchor_at_anchor", source))?;
        let next_position = edit_track(
            &layer.transform.position,
            &time,
            &TrackEdit::Set {
                value: TrackValue::Vec3(position),
            },
            Some(position),
            false,
        )
        .map_err(|source| {
            KeyframeTarget::Property {
                object,
                property: crate::Property::Position,
                axis: None,
            }
            .failure("anchor_at_position", source)
        })?;
        // Roving times depend on geometry. Never publish a compensation whose
        // new timing no longer samples the requested pivot/position here.
        ensure(
            next_anchor.sample(time.local)? == anchor
                && next_position.sample(time.local)? == position,
            "anchor compensation would retime spatial keys; fix the affected key times first",
        )?;
        // No live mutation until both storage identities and values are valid.
        let layer = project.layer_mut(object)?;
        layer.transform.anchor = next_anchor;
        layer.transform.position = next_position;
        Ok(())
    })();
    result.map_err(|source| target.failure("anchor_at", source))
}
