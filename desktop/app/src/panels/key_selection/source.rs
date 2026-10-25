//! Read authored JSON directly; selection checks never deserialize or sample an
//! entire project, nor depend on visible/expanded timeline rows.
use super::*;
use motion_core::{masks::MaskProperty, AudioChannel};

fn id(values: &Value, object: u64) -> Option<&Value> {
    values
        .as_array()?
        .iter()
        .find(|value| value["id"].as_u64() == Some(object))
}
fn vector<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    Some(match name {
        "fill" => v.get("fill")?,
        "stroke_color" => &v["stroke"]["color"],
        "stroke_width" => &v["stroke"]["width"],
        "trim_start" => &v["trim"]["start"],
        "trim_end" => &v["trim"]["end"],
        "trim_offset" => &v["trim"]["offset"],
        _ if name.starts_with("shape:") => &v["source"]["parameters"][&name[6..]],
        _ if name.starts_with("node:") => {
            let (path, node) = name[5..].split_once(':')?;
            &id(
                &id(&v["source"]["paths"], path.parse().ok()?)?["nodes"],
                node.parse().ok()?,
            )?["geometry"]
        }
        _ => dash(&v["stroke"], name)?,
    })
}
fn dash<'a>(stroke: &'a Value, name: &str) -> Option<&'a Value> {
    if name == "dash_offset" {
        Some(&stroke["dashes"]["offset"])
    } else {
        stroke["dashes"]["pattern"]
            .as_array()?
            .get(name.strip_prefix("dash_")?.parse::<usize>().ok()?)
    }
}
fn group<'a>(g: &'a Value, item: u64, name: &str) -> Option<&'a Value> {
    if g["id"].as_u64() == Some(item) {
        return g["transform"].get(name);
    }
    for value in g["items"].as_array()? {
        if value["kind"] == "group" {
            if let Some(track) = group(&value["group"], item, name) {
                return Some(track);
            }
        } else if value["id"].as_u64() == Some(item) {
            return Some(match value["kind"].as_str()? {
                "geometry" => {
                    if matches!(name, "size" | "position") {
                        value.get(name)?
                    } else {
                        vector(&value["vector"], name)?
                    }
                }
                "fill" if name == "color" => &value["color"],
                "stroke" => {
                    if matches!(name, "color" | "width") {
                        value["stroke"].get(name)?
                    } else {
                        dash(&value["stroke"], name)?
                    }
                }
                "trim" => value["trim"].get(name)?,
                "repeater" => value["repeater"].get(name)?,
                _ => return None,
            });
        }
    }
    None
}

pub(super) fn track<'a>(state: &'a Value, target: &KeyframeTarget) -> Option<(&'a Value, i64)> {
    let object = target.object();
    let layer = super::super::keyframe_clipboard::layer(state, object);
    let offset = layer.map_or(0, super::super::keyframe_clipboard::offset);
    let source = match target {
        KeyframeTarget::Property { property, axis, .. } => {
            let camera = if object == 0 {
                Some(&state["project"]["camera"])
            } else {
                layer.and_then(|l| {
                    (l["content"]["kind"] == "camera").then_some(&l["content"]["camera"])
                })
            };
            let root = if let Some(camera) = camera {
                if *property == motion_core::Property::Position && camera["mode"] != "position" {
                    return None;
                }
                camera
            } else {
                let layer = layer?;
                if layer["content"]["kind"] == "audio" {
                    return None;
                }
                &layer["transform"]
            };
            let track = root.get(super::super::animation_data::name(*property))?;
            if let Some(axis) = axis {
                track["axes"].get(["x", "y", "z"][axis.index()])?
            } else if track["axes"].is_object() {
                return None;
            } else {
                track
            }
        }
        KeyframeTarget::Effect { effect, param, .. } => {
            let p = id(&layer?["effects"], *effect)?["params"].get(param)?;
            if p["animatable"] != true || p["implemented"] != true {
                return None;
            }
            if p["curve"].is_object() {
                &p["curve"]
            } else {
                &p["track"]
            }
        }
        KeyframeTarget::Mask { mask, property, .. } => {
            let m = id(&layer?["masks"], *mask)?;
            match property {
                MaskProperty::Opacity => &m["opacity"],
                MaskProperty::Feather => &m["feather"],
                MaskProperty::Expansion => &m["expansion"],
                MaskProperty::Node(node) => &id(&m["path"]["nodes"], *node)?["geometry"],
            }
        }
        KeyframeTarget::Audio { channel, .. } => {
            &layer?["audio_levels"][match channel {
                AudioChannel::Left => "left",
                AudioChannel::Right => "right",
            }]
        }
        KeyframeTarget::Vector {
            item, parameter, ..
        } => {
            let v = &layer?["content"]["vector"];
            if let Some(item) = item {
                group(&v["source"]["group"], *item, parameter)?
            } else {
                vector(v, parameter)?
            }
        }
    };
    source.is_object().then_some((source, offset))
}

pub(super) fn contains(state: &Value, target: &KeyframeTarget, frame: u32) -> bool {
    if state["project"]["frames"]
        .as_u64()
        .is_none_or(|end| u64::from(frame) >= end)
    {
        return false;
    }
    let Some((source, offset)) = track(state, target) else {
        return false;
    };
    let Some(local) = i64::from(frame).checked_sub(offset) else {
        return false;
    };
    let Some(keys) = observe!(
        "selection.authored_keys",
        source["keys"]
            .as_array()
            .ok_or("authored key array missing"),
        "object={} phase=selection",
        target.object()
    )
    .ok() else {
        return false;
    };
    let mut malformed = false;
    let found = keys.binary_search_by(|key| match key["frame"].as_i64() {
        Some(frame) => frame.cmp(&local),
        None => {
            malformed = true;
            std::cmp::Ordering::Less
        }
    });
    // This reuses the bounded panel observer: counts are per-site process
    // lifetime, so a healthy adjacent channel does not erase malformed data.
    observe!(
        "selection.authored_key_frame",
        if malformed {
            Err("authored key frame is not an integer")
        } else {
            Ok(found.is_ok())
        },
        "object={} frame={frame} phase=selection fallback=disable_key_edit",
        target.object()
    )
    .unwrap_or(false)
}
