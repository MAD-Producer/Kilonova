//! Preflight both channels before publishing compensated anchor edits.
use super::*;

/// Persistent identities for the two channels of an atomic pivot edit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnchorKeyBinding {
    pub anchor: Option<u64>,
    pub position: Option<u64>,
}

pub fn compensate_anchor(
    project: &mut Project,
    object: u64,
    frame: f64,
    anchor: [f32; 2],
) -> Result<()> {
    compensate_anchor_with_keys(project, object, frame, anchor, None)
}

pub fn compensate_anchor_with_keys(
    project: &mut Project,
    object: u64,
    frame: f64,
    anchor: [f32; 2],
    keys: Option<&AnchorKeyBinding>,
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
        if let Some(keys) = keys {
            ensure(
                keys.anchor.is_some() || keys.position.is_some(),
                "empty anchor key binding",
            )?;
            validate_binding(&layer.transform.anchor, keys.anchor, &time)
                .map_err(|source| target.failure("anchor_key_binding", source))?;
            validate_binding(&layer.transform.position, keys.position, &time).map_err(
                |source| {
                    KeyframeTarget::Property {
                        object,
                        property: crate::Property::Position,
                        axis: None,
                    }
                    .failure("anchor_key_binding", source)
                },
            )?;
        }
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
            &bound_edit(keys.and_then(|keys| keys.anchor), TrackValue::Vec2(anchor)),
            Some(anchor),
            false,
        )
        .map_err(|source| target.failure("anchor_at_anchor", source))?;
        let next_position = edit_track(
            &layer.transform.position,
            &time,
            &bound_edit(
                keys.and_then(|keys| keys.position),
                TrackValue::Vec3(position),
            ),
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

fn bound_edit(key: Option<u64>, value: TrackValue) -> TrackEdit {
    match key {
        Some(key) => TrackEdit::SetSpatialKey { key, value },
        None => TrackEdit::Set { value },
    }
}
fn validate_binding<T: Tween>(source: &Track<T>, key: Option<u64>, time: &EditTime) -> Result<()> {
    let Some(key) = key else {
        return Ok(());
    };
    let spatial = source.spatial_animation.as_ref().ok_or_else(|| {
        Error::Invalid(format!(
            "anchor compensation key={key} spatial storage no longer exists"
        ))
    })?;
    let local = spatial
        .prepared()
        .keys()
        .find(|(id, _)| *id == key)
        .map(|(_, local)| local)
        .ok_or_else(|| Error::Invalid(format!("anchor compensation key={key} no longer exists")))?;
    ensure(
        time.clock.projected_composition_frame(local, time.offset)? == time.composition,
        &format!("anchor compensation key={key} no longer belongs to the captured time"),
    )
}
