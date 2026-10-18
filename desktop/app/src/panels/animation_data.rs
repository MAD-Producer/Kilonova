//! Rebuild read-only tracks from timeline snapshots using composition/local time.
use motion_core::{Axis, Keyframe, Property, Track, Tween};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(super) fn name(property: Property) -> &'static str {
    match property {
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
        for field in ["ease", "curve", "spatial"] {
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
    let local = frame - offset as f64;
    if property.is_scalar() || axis.is_some() {
        Some(serde_json::json!(
            track::<f32>(source, offset)?.sample(local)
        ))
    } else {
        Some(serde_json::json!(
            track::<[f32; 3]>(source, offset)?.sample(local)
        ))
    }
}
