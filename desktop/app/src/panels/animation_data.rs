//! Rebuild read-only tracks from timeline snapshots using composition/local time.
use motion_core::{Axis, Command, Keyframe, Property, Track, Tween};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(super) fn name(property: Property) -> &'static str {
    match property {
        Property::Anchor => "anchor",
        Property::Position => "position",
        Property::Rotation => "rotation",
        Property::Scale => "scale",
        Property::Opacity => "opacity",
        Property::Target => "target",
        Property::Roll => "roll",
        Property::Fov => "fov",
        Property::Radius => "radius",
        Property::Azimuth => "azimuth",
        Property::Elevation => "elevation",
    }
}
pub(super) fn display_scale(state: &Value, object: u64, property: Property, component: usize) -> f32 {
    if property == Property::Opacity { return 100.; }
    if property != Property::Anchor { return 1.; }
    let size=state["project"]["layers"].as_array().into_iter().flatten()
        .find(|l| l["id"].as_u64()==Some(object))
        .and_then(|l| l["size"][component].as_f64());
    observe!("animation.anchor_display_scale",size.filter(|v|v.is_finite()&&*v>=1.).ok_or("anchor source size unavailable"),"object={object} component={component}").unwrap_or(1.) as f32
}
pub(super) fn timeline(state: &Value, object: u64) -> Option<&Value> {
    state["timeline_layers"]
        .as_array()?
        .iter()
        .find(|l| l["object"].as_u64() == Some(object))
}
pub(super) fn channel(
    state: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
) -> Option<&Value> {
    let track = &timeline(state, object)?["properties"][name(property)];
    if track.is_null() {
        return None;
    }
    if track["separated"].as_bool() == Some(true) {
        Some(&track["axes"][["x", "y", "z"][axis?.index()]])
    } else if axis.is_none() {
        Some(track)
    } else {
        None
    }
}
pub(super) fn track<T: Tween + DeserializeOwned>(source: &Value, offset: i64) -> Option<Track<T>> {
    observe!("animation.track_store", if source["spatial_keys"].is_array() {
        Err("spatial timeline view requires an authored-track sampler")
    } else {Ok(())}, "offset={offset} value_type={}",std::any::type_name::<T>()).ok()?;
    let value = observe!(
        "animation.track_value",
        serde_json::from_value(source["value"].clone()),
        "offset={offset} value_type={}",
        std::any::type_name::<T>()
    )
    .ok()?;
    let mut keys = vec![];
    for key in source["keys"].as_array().into_iter().flatten() {
        let frame = key["local_frame"]
            .as_i64()
            .or_else(|| key["frame"].as_i64().map(|f| f - offset));
        let frame = observe!(
            "animation.key_frame",
            frame.ok_or("missing or noninteger frame"),
            "offset={offset} key_index={}",
            keys.len()
        )
        .ok()?;
        let mut entry = serde_json::Map::new();
        entry.insert(
            "frame".into(),
            serde_json::json!(observe!(
                "animation.key_range",
                i32::try_from(frame),
                "offset={offset} frame={frame}"
            )
            .ok()?),
        );
        entry.insert("value".into(), key["value"].clone());
        for field in ["ease", "curve", "spatial", "auto_bezier"] {
            if let Some(value) = key.get(field) {
                entry.insert(field.into(), value.clone());
            }
        }
        keys.push(
            observe!(
                "animation.key_decode",
                serde_json::from_value::<Keyframe<T>>(Value::Object(entry)),
                "offset={offset} frame={frame} key_index={} value_type={}",
                keys.len(),
                std::any::type_name::<T>()
            )
            .ok()?,
        );
    }
    keys.sort_by_key(|k| k.frame);
    keys.dedup_by_key(|k| k.frame);
    Some(Track {
        spatial_animation: None,
        value,
        keys,
        axes: None,
    })
}
pub(super) fn sample(
    state: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    frame: f64,
) -> Option<Value> {
    let layer = timeline(state, object)?;
    let offset = layer["offset_frame"].as_i64().unwrap_or(0);
    let source = channel(state, object, property, axis)?;
    // Called by explicit key-authoring actions, never the graph paint loop.
    // Spatial timeline summaries omit reference coordinates, so decode the
    // authoritative store rather than inventing a rounded substitute.
    fn read<T:Tween+DeserializeOwned>(state:&Value,object:u64,property:Property,axis:Option<Axis>,source:&Value,offset:i64)->Option<Track<T>> {
        if !source["spatial_keys"].is_array() {return track(source,offset);}
        let target=motion_core::KeyframeTarget::Property {object,property,axis};
        let (raw,_)=super::key_selection::authored_track(state,&target)?;
        let decoded:Track<T>=observe!("animation.authored_track",serde_json::from_value(raw.clone()),
            "object={object} property={property:?} axis={axis:?}").ok()?;
        observe!("animation.authored_track_validation",decoded.validate_local(),
            "object={object} property={property:?} axis={axis:?}").ok()?;
        Some(decoded)
    }
    let local = frame - offset as f64;
    if property.is_scalar() || axis.is_some() {
        Some(serde_json::json!(
            observe!("animation_data.sample", read::<f32>(state,object,property,axis,source,offset)?.sample(local), "object={object} property={property:?} axis={axis:?} frame={frame}").ok()?
        ))
    } else if property == Property::Anchor {
        Some(serde_json::json!(observe!("animation_data.sample", read::<[f32; 2]>(state,object,property,axis,source,offset)?.sample(local), "object={object} property={property:?} axis={axis:?} frame={frame}").ok()?))
    } else {
        Some(serde_json::json!(
            observe!("animation_data.sample", read::<[f32; 3]>(state,object,property,axis,source,offset)?.sample(local), "object={object} property={property:?} axis={axis:?} frame={frame}").ok()?
        ))
    }
}

/// Build an atomic edit for exactly one authored channel. Snapshot key times
/// are composition times; `sample` performs the layer-local conversion.
pub(super) fn key_commands(
    state: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    frame: u32,
    remove: bool,
) -> Option<Vec<Command>> {
    let layer = state["project"]["layers"].as_array()?.iter()
        .find(|layer| layer["id"].as_u64() == Some(object))?;
    if state["observing"] == true || layer["locked"] == true
        || u64::from(frame) >= state["project"]["frames"].as_u64()?
    {
        return None;
    }
    let source = channel(state, object, property, axis)?;
    let present = source["keys"].as_array().into_iter().flatten()
        .any(|key| key["frame"].as_i64() == Some(i64::from(frame)));
    if remove {
        return present.then(|| vec![Command::DeleteKey { object, property, axis, frame }]);
    }
    if present {
        return Some(vec![]);
    }
    let sampled = sample(state, object, property, axis, f64::from(frame))?;
    let value = if property.is_scalar() || axis.is_some() {
        let value = observe!("animation.key_scalar", serde_json::from_value::<f32>(sampled),
            "object={object} property={property:?} axis={axis:?} frame={frame}").ok()?;
        if let Some(axis) = axis {
            Command::SetComponent { object, property, axis, frame, value }
        } else {
            Command::SetScalar { object, property, frame, value }
        }
    } else {
        let value = if property == Property::Anchor {
            let anchor = observe!("animation.key_anchor", serde_json::from_value::<[f32; 2]>(sampled),
                "object={object} frame={frame}").ok()?;
            [anchor[0], anchor[1], 0.]
        } else {
            observe!("animation.key_vector", serde_json::from_value::<[f32; 3]>(sampled),
                "object={object} property={property:?} frame={frame}").ok()?
        };
        Command::SetVector { object, property, frame, value }
    };
    Some(vec![Command::Animate { object, property, axis, frame, enabled: true }, value])
}
