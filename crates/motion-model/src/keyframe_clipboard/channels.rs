//! Shared typed channel addressing for read snapshots and transaction writes.
use super::*;
use crate::vector::{groups::GroupItem, VectorContent, VectorSource};

pub(super) enum ReadChannel<'a> {
    Scalar(&'a Track<f32>),
    Vec2(&'a Track<[f32; 2]>),
    Vec3(&'a Track<[f32; 3]>),
    Vec4(&'a Track<[f32; 4]>),
    Geometry(&'a Track<[f32; 6]>),
    ColorCurve(&'a crate::CurveTrack),
}
pub(super) enum WriteChannel<'a> {
    Scalar(&'a mut Track<f32>),
    Vec2(&'a mut Track<[f32; 2]>),
    Vec3(&'a mut Track<[f32; 3]>),
    Vec4(&'a mut Track<[f32; 4]>),
    Geometry(&'a mut Track<[f32; 6]>),
    ColorCurve(&'a mut crate::CurveTrack),
}

// Both accessors have identical addressing and failure semantics. Only the
// borrow and iterator methods differ; reads cannot accidentally modify a source.
macro_rules! accessors {
    ($entry:ident, $vector:ident, $group:ident, $channel:ident, $iter:ident, $as_ref:ident, $get:ident, $camera:ident, $audio:ident, $($mutable:tt)?) => {
        pub(super) fn $entry<'a>(project: &'a $($mutable)? Project, target: &KeyframeTarget) -> Result<$channel<'a>> {
            let object = target.object();
            if let KeyframeTarget::Property { property, axis, .. } = target {
                let camera = object == 0 || project.layers.iter().any(|l| l.id == object && l.camera_data().is_some());
                let c = if camera {
                    let c = project.$camera(object)?;
                    match property {
                        Property::Position => {
                            ensure(c.mode == CameraMode::Position, "camera position requires position mode")?;
                            $channel::Vec3(&$($mutable)? c.position)
                        }
                        Property::Target => $channel::Vec3(&$($mutable)? c.target),
                        Property::Roll => $channel::Scalar(&$($mutable)? c.roll),
                        Property::Fov => $channel::Scalar(&$($mutable)? c.fov),
                        Property::Radius => $channel::Scalar(&$($mutable)? c.radius),
                        Property::Azimuth => $channel::Scalar(&$($mutable)? c.azimuth),
                        Property::Elevation => $channel::Scalar(&$($mutable)? c.elevation),
                        _ => return Err(Error::Invalid("clipboard property unavailable on camera".into())),
                    }
                } else {
                    let l = project.layers.$iter().find(|l| l.id == object).ok_or(Error::Missing(object))?;
                    ensure(!matches!(l.content, Content::Audio { .. }), "audio has no spatial properties")?;
                    match property {
                        Property::Anchor => $channel::Vec2(&$($mutable)? l.transform.anchor),
                        Property::Position => $channel::Vec3(&$($mutable)? l.transform.position),
                        Property::Rotation => $channel::Vec3(&$($mutable)? l.transform.rotation),
                        Property::Scale => $channel::Vec3(&$($mutable)? l.transform.scale),
                        Property::Opacity => $channel::Scalar(&$($mutable)? l.transform.opacity),
                        _ => return Err(Error::Invalid("clipboard property unavailable on layer".into())),
                    }
                };
                return match (c, axis) {
                    ($channel::Vec3(t), Some(axis)) => {
                        let axes = t.axes.$as_ref().ok_or_else(|| Error::Invalid("clipboard axis requires separated dimensions".into()))?;
                        Ok($channel::Scalar(axes.$get(*axis)))
                    }
                    ($channel::Vec3(t), None) => {
                        ensure(t.axes.is_none(), "choose a separated clipboard axis explicitly")?;
                        Ok($channel::Vec3(t))
                    }
                    (c, None) => Ok(c),
                    _ => Err(Error::Invalid("clipboard property has no separated axes".into())),
                };
            }
            let l = project.layers.$iter().find(|l| l.id == object).ok_or(Error::Missing(object))?;
            match target {
                KeyframeTarget::Audio { channel, .. } => {
                    ensure(matches!(l.content, Content::Audio { .. } | Content::Video { .. } | Content::Composition { .. }), "clipboard object has no audio levels")?;
                    let levels = $audio(l)?;
                    Ok($channel::Scalar(match channel {
                        crate::AudioChannel::Left => &$($mutable)? levels.left,
                        crate::AudioChannel::Right => &$($mutable)? levels.right,
                    }))
                }
                KeyframeTarget::Effect { effect, param, .. } => {
                    let p = l.effects.$iter().find(|e| e.id == *effect)
                        .and_then(|e| e.params.$get(param))
                        .ok_or_else(|| Error::Invalid("clipboard effect parameter missing".into()))?;
                    ensure(p.animatable && p.implemented, "clipboard effect parameter is not editable")?;
                    Ok(if let Some(curve) = p.curve.$as_ref() { $channel::ColorCurve(curve) }
                        else { $channel::Vec4(&$($mutable)? p.track) })
                }
                KeyframeTarget::Mask { mask, property, .. } => {
                    let mask = l.masks.$iter().find(|m| m.id == *mask)
                        .ok_or_else(|| Error::Invalid("clipboard mask missing".into()))?;
                    Ok(match property {
                        crate::masks::MaskProperty::Opacity => $channel::Scalar(&$($mutable)? mask.opacity),
                        crate::masks::MaskProperty::Feather => $channel::Vec2(&$($mutable)? mask.feather),
                        crate::masks::MaskProperty::Expansion => $channel::Scalar(&$($mutable)? mask.expansion),
                        crate::masks::MaskProperty::Node(id) => $channel::Geometry(&$($mutable)? mask.path.nodes.$iter()
                            .find(|n| n.id == *id).ok_or_else(|| Error::Invalid("clipboard mask node missing".into()))?.geometry),
                    })
                }
                KeyframeTarget::Vector { item, parameter, .. } => {
                    let Content::Vector { vector } = &$($mutable)? l.content else {
                        return Err(Error::Invalid("clipboard vector content missing".into()));
                    };
                    if let Some(item) = item {
                        let VectorSource::Group { group } = &$($mutable)? vector.source else {
                            return Err(Error::Invalid("clipboard shape group missing".into()));
                        };
                        $group(group, *item, parameter)
                    } else { $vector(vector, parameter) }
                }
                KeyframeTarget::Property { .. } => unreachable!(),
            }
        }

        fn $vector<'a>(v: &'a $($mutable)? VectorContent, name: &str) -> Result<$channel<'a>> {
            let missing = || Error::Invalid(format!("clipboard vector channel missing: {}", name.chars().take(256).collect::<String>()));
            Ok(match name {
                "fill" => $channel::Vec4(v.fill.$as_ref().ok_or_else(missing)?),
                "stroke_color" => $channel::Vec4(&$($mutable)? v.stroke.$as_ref().ok_or_else(missing)?.color),
                "stroke_width" => $channel::Scalar(&$($mutable)? v.stroke.$as_ref().ok_or_else(missing)?.width),
                "trim_start" => $channel::Scalar(&$($mutable)? v.trim.$as_ref().ok_or_else(missing)?.start),
                "trim_end" => $channel::Scalar(&$($mutable)? v.trim.$as_ref().ok_or_else(missing)?.end),
                "trim_offset" => $channel::Scalar(&$($mutable)? v.trim.$as_ref().ok_or_else(missing)?.offset),
                _ if name.starts_with("shape:") => {
                    let VectorSource::Shape { parameters, .. } = &$($mutable)? v.source else { return Err(missing()); };
                    $channel::Scalar(parameters.$get(&name[6..]).ok_or_else(missing)?)
                }
                _ if name.starts_with("node:") => {
                    let (path, node) = name[5..].split_once(':').ok_or_else(missing)?;
                    let path: u64 = path.parse().map_err(|_| missing())?;
                    let node: u64 = node.parse().map_err(|_| missing())?;
                    let VectorSource::Paths { paths } = &$($mutable)? v.source else { return Err(missing()); };
                    $channel::Geometry(&$($mutable)? paths.$iter().find(|p| p.id == path)
                        .and_then(|p| p.nodes.$iter().find(|n| n.id == node)).ok_or_else(missing)?.geometry)
                }
                _ if name == "dash_offset" || name.starts_with("dash_") => {
                    let dash = v.stroke.$as_ref().and_then(|s| s.dashes.$as_ref()).ok_or_else(missing)?;
                    if name == "dash_offset" { $channel::Scalar(&$($mutable)? dash.offset) }
                    else {
                        let index: usize = name[5..].parse().map_err(|_| missing())?;
                        $channel::Scalar(dash.pattern.$get(index).ok_or_else(missing)?)
                    }
                }
                _ => return Err(missing()),
            })
        }

        fn $group<'a>(g: &'a $($mutable)? crate::vector::groups::VectorGroup, id: u64, name: &str) -> Result<$channel<'a>> {
            let missing = || Error::Invalid(format!("clipboard group channel missing: item={id} parameter={}", name.chars().take(256).collect::<String>()));
            if g.id == id {
                let t = &$($mutable)? g.transform;
                return Ok(match name {
                    "position" => $channel::Vec2(&$($mutable)? t.position),
                    "anchor" => $channel::Vec2(&$($mutable)? t.anchor),
                    "scale" => $channel::Vec2(&$($mutable)? t.scale),
                    "rotation" => $channel::Scalar(&$($mutable)? t.rotation),
                    "skew" => $channel::Scalar(&$($mutable)? t.skew),
                    "skew_axis" => $channel::Scalar(&$($mutable)? t.skew_axis),
                    "opacity" => $channel::Scalar(&$($mutable)? t.opacity),
                    _ => return Err(missing()),
                });
            }
            // A missing nested item is a normal search miss, never discard an
            // error from the selected item's actual channel accessor.
            fn contains(g: &crate::vector::groups::VectorGroup, id: u64) -> bool {
                g.id == id || g.items.iter().any(|i| i.id() == id || match i {
                    GroupItem::Group { group } => contains(group, id), _ => false,
                })
            }
            for item in &$($mutable)? g.items {
                if let GroupItem::Group { group } = item {
                    if contains(group, id) { return $group(group, id, name); }
                    continue;
                }
                if item.id() != id { continue; }
                return Ok(match item {
                    GroupItem::Geometry { vector, size, position, .. } => match name {
                        "size" => $channel::Vec2(size), "position" => $channel::Vec2(position),
                        _ => return $vector(vector, name),
                    },
                    GroupItem::Fill { color, .. } if name == "color" => $channel::Vec4(color),
                    GroupItem::Stroke { stroke, .. } => match name {
                        "color" => $channel::Vec4(&$($mutable)? stroke.color),
                        "width" => $channel::Scalar(&$($mutable)? stroke.width),
                        _ => {
                            let dash = stroke.dashes.$as_ref().ok_or_else(missing)?;
                            if name == "dash_offset" { $channel::Scalar(&$($mutable)? dash.offset) }
                            else {
                                let index: usize = name.strip_prefix("dash_").ok_or_else(missing)?.parse().map_err(|_| missing())?;
                                $channel::Scalar(dash.pattern.$get(index).ok_or_else(missing)?)
                            }
                        }
                    },
                    GroupItem::Trim { trim, .. } => match name {
                        "start" => $channel::Scalar(&$($mutable)? trim.start),
                        "end" => $channel::Scalar(&$($mutable)? trim.end),
                        "offset" => $channel::Scalar(&$($mutable)? trim.offset),
                        _ => return Err(missing()),
                    },
                    GroupItem::Repeater { repeater: r, .. } => match name {
                        "copies" => $channel::Scalar(&$($mutable)? r.copies),
                        "offset" => $channel::Scalar(&$($mutable)? r.offset),
                        "position" => $channel::Vec2(&$($mutable)? r.position),
                        "anchor" => $channel::Vec2(&$($mutable)? r.anchor),
                        "scale" => $channel::Vec2(&$($mutable)? r.scale),
                        "rotation" => $channel::Scalar(&$($mutable)? r.rotation),
                        "start_opacity" => $channel::Scalar(&$($mutable)? r.start_opacity),
                        "end_opacity" => $channel::Scalar(&$($mutable)? r.end_opacity),
                        _ => return Err(missing()),
                    },
                    _ => return Err(missing()),
                });
            }
            Err(missing())
        }
    };
}

accessors!(
    read,
    read_vector,
    read_group,
    ReadChannel,
    iter,
    as_ref,
    get,
    camera_for,
    read_audio,
);
accessors!(
    write,
    write_vector,
    write_group,
    WriteChannel,
    iter_mut,
    as_mut,
    get_mut,
    camera_for_mut,
    write_audio,
    mut
);

fn read_audio(layer: &crate::Layer) -> Result<&crate::AudioLevels> {
    layer
        .audio_levels
        .as_ref()
        .ok_or_else(|| Error::Invalid("clipboard audio envelope missing".into()))
}
fn write_audio(layer: &mut crate::Layer) -> Result<&mut crate::AudioLevels> {
    Ok(layer
        .audio_levels
        .get_or_insert_with(crate::AudioLevels::default))
}
