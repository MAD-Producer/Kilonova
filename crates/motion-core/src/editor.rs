use crate::{
    ensure, CameraMode, Command, Content, Ease, Easing, EditResult, Error, Layer, Project, Property, Result, Track,
};
use glam::{Quat,Vec3};
use std::collections::VecDeque;

/// Commands route explicitly within the shared composition graph.
/// Omitting it always means comp-main, never an editor's current selection.
pub fn parse_commands(text: &str) -> Result<Vec<Command>> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let requests = match value {
        serde_json::Value::Array(values) => values,
        value => vec![value],
    };
    requests
        .into_iter()
        .map(|mut value| {
            let object = value
                .as_object_mut()
                .ok_or_else(|| Error::Invalid("command must be an object".into()))?;
            // Desktop actions serialize the typed wrapper; public API clients
            // also use a flat command with a composition field. Preserve the
            // wrapper's required field instead of consuming it as flat scope.
            let wrapped = object.get("op").and_then(serde_json::Value::as_str) == Some("in_composition");
            let composition = if wrapped { object.get("composition").cloned().unwrap_or(serde_json::Value::Null) }
                else { object.remove("composition").unwrap_or_else(||serde_json::json!(crate::MAIN_COMPOSITION)) };
            let composition = composition.as_str().ok_or_else(||Error::Invalid("composition must be a stable ID string".into()))?.to_string();
            ensure(composition.starts_with("comp-")&&composition.len()<=64&&composition.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'),"invalid composition ID")?;
            let command = serde_json::from_value(value)?;
            let command = if wrapped { command } else { Command::InComposition { composition, command: Box::new(command) } };
            let mut scope = &command;
            while let Command::InComposition { composition, command } = scope {
                ensure(composition.starts_with("comp-")&&composition.len()<=64&&composition.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'),"invalid composition ID")?;
                scope = command;
            }
            Ok(command)
        })
        .collect()
}

enum Channel<'a> {
    Anchor(&'a mut Track<[f32; 2]>),
    Scalar(&'a mut Track<f32>),
    Vector(&'a mut Track<[f32; 3]>),
}
impl Channel<'_> {
    fn curve(self, frame: i32, easing: Easing) -> Result<()> {
        match self {
            Self::Anchor(t) => t.set_curve(frame, easing),
            Self::Scalar(t) => t.set_curve(frame, easing),
            Self::Vector(t) => t.set_curve(frame, easing),
        }
    }
    fn copy_key(self, from: i32, to: i32) -> Result<()> {
        match self {
            Self::Anchor(t) => t.copy_key(from, to),
            Self::Scalar(t) => t.copy_key(from, to),
            Self::Vector(t) => t.copy_key(from, to),
        }
    }
    fn animate(self, frame: i32, enabled: bool) -> Result<()> {
        match self {
            Self::Anchor(t) => t.set_animated(frame, enabled),
            Self::Scalar(t) => t.set_animated(frame, enabled),
            Self::Vector(t) => t.set_animated(frame, enabled),
        }
    }
    fn move_key(self, from: i32, to: i32) -> Result<()> {
        match self {
            Self::Anchor(t) => t.move_key(from, to),
            Self::Scalar(t) => t.move_key(from, to),
            Self::Vector(t) => t.move_key(from, to),
        }
    }
    fn delete_key(self, frame: i32) -> Result<()> {
        match self {
            Self::Anchor(t) => t.delete_key(frame),
            Self::Scalar(t) => t.delete_key(frame),
            Self::Vector(t) => t.delete_key(frame),
        }
    }
    fn ease(self, frame: i32, ease: Ease) -> Result<()> {
        match self {
            Self::Anchor(t) => t.set_ease(frame, ease),
            Self::Scalar(t) => t.set_ease(frame, ease),
            Self::Vector(t) => t.set_ease(frame, ease),
        }
    }
}

fn channel(project: &mut Project, object: u64, property: Property) -> Result<Channel<'_>> {
    if object == 0 || project.layers.iter().any(|l| l.id == object && l.camera_data().is_some()) {
        let c = project.camera_for_mut(object)?;
        return Ok(match property {
            Property::Position => {
                ensure(
                    c.mode == CameraMode::Position,
                    "position is driven by the orbit path",
                )?;
                Channel::Vector(&mut c.position)
            }
            Property::Target => Channel::Vector(&mut c.target),
            Property::Roll => Channel::Scalar(&mut c.roll),
            Property::Fov => Channel::Scalar(&mut c.fov),
            Property::Radius => Channel::Scalar(&mut c.radius),
            Property::Azimuth => Channel::Scalar(&mut c.azimuth),
            Property::Elevation => Channel::Scalar(&mut c.elevation),
            _ => {
                return Err(Error::Invalid(
                    "property is not available on a camera".into(),
                ))
            }
        });
    }
    let layer = project.layer_mut(object)?;
    ensure(
        !matches!(layer.content, Content::Audio { .. }),
        "audio has no spatial or opacity properties",
    )?;
    if layer.locked {
        return Err(Error::Locked(object));
    }
    Ok(match property {
        Property::Anchor => Channel::Anchor(&mut layer.transform.anchor),
        Property::Position => Channel::Vector(&mut layer.transform.position),
        Property::Rotation => Channel::Vector(&mut layer.transform.rotation),
        Property::Scale => Channel::Vector(&mut layer.transform.scale),
        Property::Opacity => Channel::Scalar(&mut layer.transform.opacity),
        _ => {
            return Err(Error::Invalid(
                "property is not available on a layer".into(),
            ))
        }
    })
}
fn axis_channel(
    project: &mut Project,
    object: u64,
    property: Property,
    axis: Option<crate::Axis>,
) -> Result<Channel<'_>> {
    let c = channel(project, object, property)?;
    match (c, axis) {
        (Channel::Anchor(_), Some(_)) => Err(Error::Invalid("anchor does not support separated axes".into())),
        (Channel::Vector(t), Some(axis)) => Ok(Channel::Scalar(t.axis_mut(axis)?)),
        (Channel::Scalar(_), Some(_)) => {
            Err(Error::Invalid("scalar property has no XYZ axis".into()))
        }
        (c, None) => Ok(c),
    }
}

fn editable_clip(project: &mut Project, object: u64) -> Result<&mut Layer> {
    ensure(object != 0, "camera has no editable clip")?;
    let layer = project.layer_mut(object)?;
    if layer.locked {
        return Err(Error::Locked(object));
    }
    Ok(layer)
}

fn video_audio_layer(project: &Project, source: &Layer, id: u64) -> Result<Layer> {
    ensure(matches!(source.content, Content::Video { .. }), "audio extraction requires a video layer")?;
    let audio = project.layer_audio(source)
        .ok_or_else(|| Error::Invalid("video has no audio track".into()))?;
    let asset = project.audio_assets.iter().find(|a| a.id == audio.asset)
        .ok_or_else(|| Error::Invalid("audio asset missing".into()))?;
    let mut layer = Layer::solid(id, &format!("{} Audio", source.name), [0.; 2], [0.; 3], [0.; 4]);
    layer.content = Content::Audio { audio: audio.clone() };
    layer.timeline = source.timeline;
    layer.subframe_range = source.subframe_range;
    layer.audio_levels = source.audio_levels.clone();
    layer.audio_markers = source.audio_markers.clone();
    layer.audio_source_channel = source.audio_source_channel;
    layer.visible = source.visible;
    layer.solo = source.solo;
    // Video and audio tracks can have different durations. Preserve the source
    // clock and intersect the video clip with recoverable audio samples.
    let span = source.interval(project.frames);
    let source_begin = f64::from(source.clip(project.frames).offset_frame)
        - audio.source_offset_us as f64 * f64::from(project.fps) / 1_000_000.;
    let source_end = source_begin + asset.sample_frames as f64
        * f64::from(project.fps) / f64::from(asset.sample_rate);
    let range = crate::ClipRange { in_frame: span.in_frame.max(source_begin),
        out_frame: span.out_frame.min(source_end) };
    ensure(range.in_frame < range.out_frame, "video clip has no recoverable audio samples")?;
    layer.set_interval(project.frames, range)?;
    Ok(layer)
}

fn apply_to(project: &mut Project, command: Command) -> Result<Option<EditResult>> {
    if let Command::InComposition { composition, command } = command {
        let previous = project.composition_id.clone();
        project.activate_composition(&composition)?;
        let result = apply_to(project, *command);
        project.activate_composition(&previous)?;
        return result;
    }
    let valid_frame = |frame| ensure(frame < project.frames, "edit frame outside the composition");
    let mut result = None;
    match command {
        Command::InComposition { .. } => unreachable!(),
        Command::ApplyAnimationPreset { preset, objects, at_frame } => preset.apply(project, &objects, at_frame)?,
        Command::PasteLayers { clipboard, at_frame, keep_external_references } => {
            result = Some(crate::layer_clipboard::paste(project, *clipboard, at_frame, keep_external_references)?);
        }
        Command::RemoveLayers { objects, frame } => crate::layer_clipboard::remove(project, objects, frame)?,
        Command::PasteKeyframes { clipboard, targets, at_frame } => clipboard.paste(project, &targets, at_frame)?,
        Command::RemoveKeyframes { clipboard } => motion_model::keyframe_clipboard::remove(project, &clipboard)?,
        Command::KeyframeInterpolation { selections, ease } => motion_model::keyframe_clipboard::set_interpolation(project, &selections, ease)?,
        Command::PasteEasing { clipboard, selections } => clipboard.paste(project, &selections)?,
        Command::KeyframeTiming { selections, settings } => motion_model::keyframe_clipboard::timing::configure(project, &selections, &settings)?,
        Command::EaseKeyframes { selections, incoming, outgoing } => motion_model::keyframe_clipboard::ease_keys(project, &selections, incoming, outgoing)?,
        Command::DeleteKeyframes { selections } => motion_model::keyframe_clipboard::delete_keys(project, &selections)?,
        Command::TransformKeyframes { selections, delta_frame, value_offset, copy } => motion_model::keyframe_clipboard::transform_keys(project, &selections, delta_frame, value_offset.as_ref(), copy)?,
        Command::PastePaths { clipboard, target, at_frame } => clipboard.paste(project, target, at_frame)?,
        Command::RemovePaths { clipboard } => clipboard.remove(project)?,
        Command::CopyPathAs { target, path } => { motion_model::path_conversion::copy_as(project, target, path)?; },
        Command::SetWorkingPrecision { precision } => project.working_precision = precision,
        Command::SetGuides { guides } => motion_model::guides::set(project, guides)?,
        Command::SetWorkArea { in_frame, out_frame } => {
            let area = crate::WorkArea { in_frame, out_frame };
            area.validate(project.frames)?;
            project.work_area = area;
        }
        Command::Composition { action } => result=Some(EditResult::Composition { result: project.edit_composition(action)? }),
        Command::SetColor {object,value} => {
            ensure(value.iter().all(|v|v.is_finite()&&(0. ..=1.).contains(v)),"color requires four normalized 0..1 components")?;
            if object==0 {project.background=value;}else {
                let layer=project.layer_mut(object)?;
                ensure(!layer.locked,"color layer is locked")?;
                match &mut layer.content {
                    Content::Solid {color} | Content::Text {color,..} => *color=value,
                    _=>return Err(Error::Invalid("layer has no editable source color".into())),
                }
            }
        }
        Command::RegisterAudioAsset { asset } => project.audio_assets.push(asset),
        Command::RegisterVideoAsset { asset } => project.video_assets.push(asset),
        Command::MediaAsset { action } => motion_model::media_assets::apply(project, action)?,
        Command::SetAudio {
            object,
            volume,
            muted,
        } => {
            let layer = editable_clip(project, object)?;
            match &mut layer.content {
                Content::Audio { audio } => {
                    if let Some(v) = volume {
                        audio.volume = v;
                    }
                    if let Some(v) = muted {
                        audio.muted = v;
                    }
                }
                Content::Video { video } => {
                    if let Some(v) = volume {
                        video.volume = v;
                    }
                    if let Some(v) = muted {
                        video.muted = v;
                    }
                }
                _ => return Err(Error::Invalid("object has no audio controls".into())),
            }
        }
        Command::AudioLevels {object,action}=>{
            let frames=project.frames;
            crate::audio_levels::edit(project.layer_mut(object)?,frames,action)?;
        }
        Command::AudioMarkers { object, action } => {
            let frames = project.frames;
            motion_model::audio_edit::edit(project.layer_mut(object)?, frames, action)?;
        }
        Command::ExtractAudio { object } => {
            let source = editable_clip(project, object)?.clone();
            ensure(project.layers.len() < crate::MAX_LAYERS, "layer limit exceeded")?;
            let next = project.layers.iter().map(|l| l.id).max().unwrap_or(0)
                .checked_add(1).ok_or_else(|| Error::Invalid("layer ID space exhausted".into()))?;
            let audio = video_audio_layer(project, &source, next)?;
            let index = project.layers.iter().position(|l| l.id == object).ok_or(Error::Missing(object))?;
            if let Content::Video { video } = &mut project.layers[index].content { video.muted = true; }
            project.layers[index].audio_markers.clear();
            project.layers.insert(index + 1, audio);
        }
        Command::SplitAudioChannels { object } => {
            let source = editable_clip(project, object)?.clone();
            ensure(source.audio_source_channel == crate::AudioSourceChannel::Stereo,
                "audio channels are already separated")?;
            let audio = project.layer_audio(&source)
                .ok_or_else(|| Error::Invalid("layer has no single audio source".into()))?;
            let asset = project.audio_assets.iter().find(|a| a.id == audio.asset)
                .ok_or_else(|| Error::Invalid("audio asset missing".into()))?.clone();
            ensure(asset.channels == 2,
                "channel separation requires stereo audio")?;
            let is_audio = matches!(source.content, Content::Audio { .. });
            ensure(project.layers.len() + if is_audio { 1 } else { 2 } <= crate::MAX_LAYERS, "layer limit exceeded")?;
            let next = project.layers.iter().map(|l| l.id).max().unwrap_or(0)
                .checked_add(1).ok_or_else(|| Error::Invalid("layer ID space exhausted".into()))?;
            let mut left = if is_audio { source.clone() } else {
                video_audio_layer(project, &source, next)?
            };
            let mut right = left.clone();
            // One analysis has one owner on the shared time ruler. Moving a
            // split-off channel must not reveal an overlapping duplicate set.
            right.audio_markers.clear();
            right.id = if is_audio { next } else { next.checked_add(1)
                .ok_or_else(|| Error::Invalid("layer ID space exhausted".into()))? };
            left.name = format!("{} L", source.name);
            right.name = format!("{} R", source.name);
            left.audio_source_channel = crate::AudioSourceChannel::Left;
            right.audio_source_channel = crate::AudioSourceChannel::Right;
            let index = project.layers.iter().position(|l| l.id == object).ok_or(Error::Missing(object))?;
            if is_audio {
                project.layers[index] = left;
                project.layers.insert(index + 1, right);
            } else {
                if let Content::Video { video } = &mut project.layers[index].content { video.muted = true; }
                project.layers[index].audio_markers.clear();
                project.layers.insert(index + 1, left);
                project.layers.insert(index + 2, right);
            }
        }
        Command::SetRotationOrder {object,order}=>{
            let layer=project.layer_mut(object)?;
            if layer.locked{return Err(Error::Locked(object));}
            ensure(!matches!(layer.content,Content::Audio{..}|Content::Adjustment),"object has no spatial rotation order")?;
            layer.rotation_order=order;
        }
        Command::SetLayer3d { object, enabled } => {
            let layer = project.layer_mut(object)?;
            if layer.locked {
                return Err(Error::Locked(object));
            }
            layer.three_d = enabled;
        }
        Command::SetLayerSampling { object, sampling } => {
            let layer = project.layer_mut(object)?;
            if layer.locked { return Err(Error::Locked(object)); }
            sampling.validate_content(&layer.content)?;
            layer.sampling = sampling;
        }
        Command::SetExpression { expression, frame } => {
            crate::expressions::set(project, expression, frame)?
        }
        Command::RemoveExpression { target } => {
            if target.object() != 0 {
                ensure(
                    !project.layer_mut(target.object())?.locked,
                    "object is locked",
                )?;
            }
            ensure(
                project.expressions.iter().any(|e| e.target == target),
                "expression does not exist",
            )?;
            project.expressions.retain(|e| e.target != target);
        }
        Command::Effect { object, action } => {
            let copy = if let crate::EffectAction::Duplicate { effect } = &action {
                Some(*effect)
            } else {
                None
            };
            let frames = project.frames;
            crate::effects::apply(project.layer_mut(object)?, action, frames)?;
            if let Some(from) = copy {
                let to = project.layer_mut(object)?.effects.last().ok_or_else(|| {
                    Error::Invalid(format!(
                        "effect duplication produced no instance: object={object} source={from}"
                    ))
                })?.id;
                let copies: Vec<_> = project
                    .expressions
                    .iter()
                    .filter_map(|e| {
                        if let crate::ExpressionTarget::Effect {
                            object: o, effect, ..
                        } = &e.target
                        {
                            if *o == object && *effect == from {
                                let mut e = e.clone();
                                if let crate::ExpressionTarget::Effect { effect, .. } =
                                    &mut e.target
                                {
                                    *effect = to;
                                }
                                return Some(e);
                            }
                        }
                        None
                    })
                    .collect();
                project.expressions.extend(copies);
            }
            let ids: Vec<_> = project
                .layer_mut(object)?
                .effects
                .iter()
                .map(|e| e.id)
                .collect();
            project.expressions.retain(|e|!matches!(&e.target,crate::ExpressionTarget::Effect {object:o,effect,..} if *o==object && !ids.contains(effect)));
        }
        Command::SeparateDimensions { object, property } => {
            match channel(project, object, property)? {
                Channel::Vector(t) => t.separate()?,
                _ => {
                    return Err(Error::Invalid(
                        "only vector properties can separate dimensions".into(),
                    ))
                }
            }
        }
        Command::MergeDimensions { object, property } => {
            match channel(project, object, property)? {
                Channel::Vector(t) => t.merge_dimensions()?,
                _ => return Err(Error::Invalid("only vector properties can merge dimensions".into())),
            }
        }
        Command::SetComponent {
            object,
            property,
            axis,
            frame,
            value,
        } => {
            let frame = project.edit_frame(object, frame)?;
            match axis_channel(project, object, property, Some(axis))? {
                Channel::Scalar(t) => t.set_at(frame, value)?,
                _ => unreachable!(),
            }
        }
        Command::MoveLayerClip { object, in_frame } => {
            let frames = project.frames;
            let layer = editable_clip(project, object)?;
            let old = layer.clip(frames);
            let delta = i64::from(in_frame) - i64::from(old.in_frame);
            let out_frame = i32::try_from(i64::from(old.out_frame) + delta)
                .map_err(|_| Error::Invalid("clip interval overflow".into()))?;
            let offset_frame = i32::try_from(i64::from(old.offset_frame) + delta)
                .map_err(|_| Error::Invalid("clip offset overflow".into()))?;
            let clip = crate::LayerTimeline {
                in_frame,
                out_frame,
                offset_frame,
            };
            clip.validate(frames)?;
            if clip != old {
                layer.timeline = Some(clip);
                if let Some(range)=layer.subframe_range {layer.subframe_range=Some(range.translated(delta));}
            }
        }
        Command::TrimLayerClip {
            object,
            in_frame,
            out_frame,
        } => {
            let frames = project.frames;
            let layer = editable_clip(project, object)?;
            let old=layer.clip(frames);
            let exact=layer.interval(frames);
            let clip = crate::LayerTimeline {
                in_frame,
                out_frame,
                ..layer.clip(frames)
            };
            clip.validate(frames)?;
            if clip != layer.clip(frames) {
                if layer.subframe_range.is_some(){layer.set_interval(frames,crate::ClipRange{
                    in_frame:if in_frame==old.in_frame{exact.in_frame}else{f64::from(in_frame)},
                    out_frame:if out_frame==old.out_frame{exact.out_frame}else{f64::from(out_frame)}})?;}
                else{layer.timeline=Some(clip);}
            }
        }
        Command::SetLayerRange {object,in_frame,out_frame}=>{
            let frames=project.frames;
            editable_clip(project,object)?.set_interval(frames,crate::ClipRange{in_frame,out_frame})?;
        }
        Command::SplitLayerClip { object, frame } => {
            ensure(
                project.layers.len() < crate::MAX_LAYERS,
                "layer limit exceeded",
            )?;
            let frames = project.frames;
            ensure(frame < frames, "split frame outside composition")?;
            let frame = i32::try_from(frame).map_err(|_| Error::Invalid("split frame overflow".into()))?;
            let mut right = editable_clip(project, object)?.clone();
            let clip = right.clip(frames);
            let precise=right.subframe_range.is_some();
            let exact=right.interval(frames);
            ensure(
                exact.in_frame < f64::from(frame) && f64::from(frame) < exact.out_frame,
                "split must be inside the layer clip",
            )?;
            right.id = project
                .layers
                .iter()
                .map(|l| l.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("layer ID space exhausted".into()))?;
            right.timeline = Some(crate::LayerTimeline {
                in_frame: frame,
                ..clip
            });
            if precise{right.set_interval(frames,crate::ClipRange{in_frame:f64::from(frame),..exact})?;}
            let right_object = right.id;
            let index = project
                .layers
                .iter()
                .position(|l| l.id == object)
                .ok_or(Error::Missing(object))?;
            project.layers[index].timeline = Some(crate::LayerTimeline {
                out_frame: frame,
                ..clip
            });
            if precise{project.layers[index].set_interval(frames,crate::ClipRange{out_frame:f64::from(frame),..exact})?;}
            project.layers.insert(index + 1, right);
            crate::expressions::copy_layer(project, object, right_object);
            result = Some(EditResult::SplitLayerClip {
                left_object: object,
                right_object,
            });
        }
        Command::Spatial { object, property, frame, tangents } => {
            valid_frame(frame)?;
            ensure(matches!(property, Property::Position | Property::Target), "spatial path requires a position property")?;
            let frame = project.edit_frame(object, frame)?;
            match channel(project, object, property)? {
                Channel::Vector(t) => t.set_spatial(frame, tangents)?,
                _ => return Err(Error::Invalid("spatial path requires a vector property".into())),
            }
        }
        Command::Curve {
            object,
            property,
            axis,
            frame,
            easing,
        } => {
            valid_frame(frame)?;
            let frame = project.edit_frame(object, frame)?;
            axis_channel(project, object, property, axis)?.curve(frame, easing)?;
        }
        Command::CopyKey {
            object,
            property,
            axis,
            from,
            to,
        } => {
            valid_frame(from)?;
            valid_frame(to)?;
            let from = project.edit_frame(object, from)?;
            let to = project.edit_frame(object, to)?;
            axis_channel(project, object, property, axis)?.copy_key(from, to)?;
        }
        Command::SetVector {
            object,
            property,
            frame,
            value,
        } => {
            valid_frame(frame)?;
            let frame = project.edit_frame(object, frame)?;
            match channel(project, object, property)? {
                Channel::Anchor(t) => {
                    ensure(value[2] == 0., "anchor requires two components; Z must be zero")?;
                    t.set_at(frame, [value[0], value[1]])?;
                }
                Channel::Vector(t) => t.set_at(frame, value)?,
                _ => return Err(Error::Invalid("expected a scalar property".into())),
            }
        }
        Command::SetScalar {
            object,
            property,
            frame,
            value,
        } => {
            valid_frame(frame)?;
            let frame = project.edit_frame(object, frame)?;
            match channel(project, object, property)? {
                Channel::Scalar(t) => t.set_at(frame, value)?,
                _ => return Err(Error::Invalid("expected a vector property".into())),
            }
        }
        Command::Animate {
            object,
            property,
            axis,
            frame,
            enabled,
        } => {
            valid_frame(frame)?;
            let frame = project.edit_frame(object, frame)?;
            axis_channel(project, object, property, axis)?.animate(frame, enabled)?;
        }
        Command::MoveKey {
            object,
            property,
            axis,
            from,
            to,
        } => {
            valid_frame(from)?;
            valid_frame(to)?;
            let from = project.edit_frame(object, from)?;
            let to = project.edit_frame(object, to)?;
            axis_channel(project, object, property, axis)?.move_key(from, to)?;
        }
        Command::DeleteKey {
            object,
            property,
            axis,
            frame,
        } => {
            valid_frame(frame)?;
            let frame = project.edit_frame(object, frame)?;
            axis_channel(project, object, property, axis)?.delete_key(frame)?;
        }
        Command::Ease {
            object,
            property,
            axis,
            frame,
            ease,
        } => {
            valid_frame(frame)?;
            let frame = project.edit_frame(object, frame)?;
            axis_channel(project, object, property, axis)?.ease(frame, ease)?;
        }
        Command::AddAdjustment { id, name } => {
            let mut layer = Layer::solid(
                id,
                &name,
                [project.width as f32, project.height as f32],
                [project.width as f32 * 0.5, project.height as f32 * 0.5, 0.],
                [1.; 4],
            );
            layer.content = Content::Adjustment;
            project.layers.push(layer);
        }
        Command::AddShape {
            id,
            name,
            shape,
            size,
            position,
        } => {
            let mut layer = Layer::solid(id, &name, size, position, [1.; 4]);
            layer.rotation_order=crate::RotationOrder::FixedXyz;
            layer.content = Content::Vector {
                vector: crate::vector::VectorContent::shape(shape),
            };
            project.layers.push(layer);
        }
        Command::Vector { object, action } => {
            let first_vertex = match &action {
                crate::vector::VectorAction::SetFirstVertex { path, node, .. } => Some((*path, *node)),
                _ => None,
            };
            if let crate::vector::VectorAction::EditGroupKey { edit, .. } = &action {
                for frame in edit.frames() { valid_frame(frame)?; }
            }
            let edit_frame = match &action {
                crate::vector::VectorAction::SetNode { frame, .. }
                | crate::vector::VectorAction::SetParameter { frame, .. }
                | crate::vector::VectorAction::SetModifierParameter { frame, .. }
                | crate::vector::VectorAction::SetModifierCurve { frame, .. }
                | crate::vector::VectorAction::SetGroupParameter { frame, .. }
                | crate::vector::VectorAction::ConvertToPath { frame } => Some(*frame),
                _ => None,
            };
            if let Some(frame) = edit_frame {
                valid_frame(frame)?;
            }
            let edited = (|| -> Result<()> {
                let layer = project.layer_mut(object)?;
                if layer.locked {
                    return Err(Error::Locked(object));
                }
                let Content::Vector { vector } = &mut layer.content else {
                    return Err(Error::Invalid("not a vector layer".into()));
                };
                vector.edit(
                    action,
                    layer.size,
                    layer.timeline.map_or(0, |t| t.offset_frame),
                )?;
                Ok(())
            })();
            edited.map_err(|source| match first_vertex {
                Some((path, node)) => Error::PathOperation {
                    operation: "first_vertex", object, path, node: Some(node), source: Box::new(source),
                },
                None => source,
            })?;
        }
        Command::Mask { object, action } => {
            let first_vertex = match &action {
                crate::masks::MaskAction::SetFirstVertex { path, node, .. } => Some((*path, *node)),
                _ => None,
            };
            for frame in action.frames() { valid_frame(frame)?; }
            let edited = (|| -> Result<()> {
                let layer = project.layer_mut(object)?;
                if layer.locked { return Err(Error::Locked(object)); }
                crate::masks::edit(&mut layer.masks, action, layer.timeline.map_or(0, |t| t.offset_frame))?;
                Ok(())
            })();
            edited.map_err(|source| match first_vertex {
                Some((path, node)) => Error::PathOperation {
                    operation: "first_vertex", object, path, node: Some(node), source: Box::new(source),
                },
                None => source,
            })?;
        }
        Command::SetLayerBlend {object,mode,space}=> {
            let l=project.layer_mut(object)?;if l.locked{return Err(Error::Locked(object));}
            if let Some(mode)=mode {l.blend.mode=mode;}if let Some(space)=space {l.blend.space=space;}
        }
        Command::SetTrackMatte {object,matte}=> {
            let l=project.layer_mut(object)?;if l.locked{return Err(Error::Locked(object));}l.track_matte=matte;
        }
        Command::RegisterAsset { asset } => project.assets.push(asset),
        Command::RegisterModelAsset { asset } => project.model_assets.push(asset),
        Command::RegisterFontAsset { asset } => project.fonts.push(asset),
        Command::Content {
            object,
            content,
            size,
        } => {
            let layer = project.layer_mut(object)?;
            if layer.locked {
                return Err(Error::Locked(object));
            }
            layer.content = content;
            layer.size = size;
        }
        Command::Add { mut layer } => {
            layer
                .timeline
                .get_or_insert_with(|| crate::LayerTimeline::full(project.frames));
            project.layers.push(layer);
        }
        Command::CreateCamera => {
            let object = project.layers.iter().map(|l| l.id).max().unwrap_or(0)
                .checked_add(1).ok_or_else(|| Error::Invalid("layer ID space exhausted".into()))?;
            let mut layer = Layer::camera(object, &format!("Camera {object}"), project.width, project.height);
            layer.timeline = Some(crate::LayerTimeline::full(project.frames));
            project.layers.push(layer);
            result = Some(EditResult::CreateCamera { object });
        }
        Command::Remove { object, frame } => {
            ensure(!project.layers.iter().any(|l|l.track_matte.is_some_and(|m|m.source==object)),"layer is referenced as a track matte; clear dependent references before deletion")?;
            valid_frame(frame)?;
            if object == 0 {
                ensure(project.camera.created, "camera does not exist")?;
            }
            if object != 0 {
                ensure(!project.layer_mut(object)?.locked, "object is locked")?;
            }
            let children: Vec<_> = project
                .layers
                .iter()
                .filter(|l| l.parent.as_ref().and_then(|p| p.object) == Some(object))
                .map(|l| l.id)
                .collect();
            for child in children {
                crate::hierarchy::reparent(project, child, None, frame)?;
            }
            if project.camera.parent.as_ref().and_then(|p| p.object) == Some(object) {
                crate::hierarchy::reparent(project, 0, None, frame)?;
            }
            if object == 0 {
                project.camera = crate::Camera::new(project.width, project.height);
                project.camera.created = false;
            } else {
                project.layers.retain(|l| l.id != object);
                crate::effects::clear_layer_image_inputs(&mut project.layers, object);
            }
            project.expressions.retain(|e| e.target.object() != object);
        }
        Command::Parent {
            object,
            parent,
            frame,
        } => {
            valid_frame(frame)?;
            crate::hierarchy::reparent(project, object, parent, frame)?;
        }
        Command::ParentWithMode { object, parent, frame, mode } => {
            valid_frame(frame).map_err(|source| Error::ParentOperation {
                object, parent, frame, mode, source: Box::new(source),
            })?;
            crate::hierarchy::reparent_with_mode(project, object, parent, frame, mode)?;
        }
        Command::Delete { object } => {
            if project.layer_mut(object)?.locked {
                return Err(Error::Locked(object));
            }
            project.layers.retain(|l| l.id != object);
            crate::effects::clear_layer_image_inputs(&mut project.layers, object);
            project.expressions.retain(|e| e.target.object() != object);
        }
        Command::Duplicate { object } => {
            let mut layer = project.layer_mut(object)?.clone();
            layer.id = project
                .layers
                .iter()
                .map(|l| l.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("layer ID space exhausted".into()))?;
            layer.name = format!("{} 副本", layer.name);
            layer.locked = false;
            let new_object = layer.id;
            project.layers.push(layer);
            crate::expressions::copy_layer(project, object, new_object);
        }
        Command::Rename { object, name } => project.layer_mut(object)?.name = name,
        Command::Flags {
            object,
            visible,
            locked,
        } => {
            let layer = project.layer_mut(object)?;
            layer.visible = visible;
            layer.locked = locked;
        }
        Command::SetLayerShy { object, shy } => {
            let layer = project.layer_mut(object)?;
            if layer.locked { return Err(Error::Locked(object)); }
            layer.shy = shy;
        }
        Command::SetLayerSwitch { object, switch, enabled } => project.set_layer_switch(object, switch, enabled)?,
        Command::Reorder { object, index } => {
            ensure(index < project.layers.len(), "invalid layer destination")?;
            let from = project
                .layers
                .iter()
                .position(|l| l.id == object)
                .ok_or(Error::Missing(object))?;
            let layer = project.layers.remove(from);
            project.layers.insert(index, layer);
        }
        Command::CameraLens { object, frame, film_width_mm, focal_length_mm } => {
            valid_frame(frame)?;
            let lens = motion_model::lens::Lens::from_focal_length(
                f64::from(film_width_mm), f64::from(focal_length_mm), project.width, project.height)?;
            let local = if object == 0 { frame as i32 } else {
                let frames = project.frames;
                project.layer_mut(object)?.clip(frames).edit_frame(frame)?
            };
            let camera = project.camera_for_mut(object)?;
            camera.fov.set_at(local, lens.vertical_fov_degrees as f32)?;
            camera.film_width_mm = film_width_mm;
        }
        Command::CameraMode { object, mode } => {
            let (start, end) = if object == 0 { (0, project.frames as i32) } else {
                let frames = project.frames;
                let clip = project.layer_mut(object)?.clip(frames);
                let local = |frame: i32| frame.checked_sub(clip.offset_frame)
                    .ok_or_else(|| Error::Invalid("local camera time overflow".into()));
                (local(clip.in_frame)?, local(clip.out_frame)?)
            };
            project.camera_for_mut(object)?.convert_mode_range(mode, start, end)?;
        }
        Command::Dolly { object, frame, amount } => {
            valid_frame(frame)?;
            let local = if object == 0 { frame as i32 } else {
                let frames = project.frames;
                let layer = project.layer_mut(object)?;
                layer.clip(frames).edit_frame(frame)?
            };
            let camera = project.camera_for_mut(object)?;
            ensure(amount.is_finite(), "invalid dolly movement")?;
            if camera.mode == CameraMode::Orbit {
                let radius = camera.radius.sample(f64::from(local))?;
                camera
                    .radius
                    .set_at(local, (radius - amount).max(1.0))?;
            } else {
                let eye = Vec3::from_array(camera.position.sample(f64::from(local))?);
                let target = Vec3::from_array(camera.target.sample(f64::from(local))?);
                let distance = eye.distance(target);
                ensure(distance >= 1.0, "camera is too close to its target")?;
                let step = amount.min(distance - 1.0);
                camera
                    .position
                    .set_at(local, (eye + (target - eye).normalize() * step).to_array())?;
            }
        }
        Command::Pan { object, frame, x, y } => {
            valid_frame(frame)?;
            let local = if object == 0 { frame as i32 } else {
                let frames = project.frames;
                let layer = project.layer_mut(object)?;
                layer.clip(frames).edit_frame(frame)?
            };
            let (width, height) = (project.width, project.height);
            let camera = project.camera_for_mut(object)?;
            ensure(x.is_finite() && y.is_finite(), "invalid camera pan")?;
            let pose = camera
                .pose(f64::from(local), width, height)?;
            let forward = (pose.target - pose.eye).normalize();
            let basis_up = if forward.dot(Vec3::Y).abs() > 0.999 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            let rolled_up = Quat::from_axis_angle(
                forward,
                camera.roll.sample(f64::from(local))?.to_radians(),
            ) * basis_up;
            let right = forward.cross(rolled_up).normalize();
            let up = right.cross(forward).normalize();
            let delta_world = right * x + up * y;
            let delta = [delta_world.x, -delta_world.y, -delta_world.z];
            let pos = camera.position.sample(f64::from(local))?;
            let target = camera.target.sample(f64::from(local))?;
            let sum = |v: [f32; 3]| std::array::from_fn(|i| v[i] + delta[i]);
            if camera.mode == CameraMode::Position {
                camera.position.set_at(local, sum(pos))?;
            }
            camera.target.set_at(local, sum(target))?;
        }
        Command::Anchor { object, anchor } => {
            ensure(
                anchor
                    .into_iter()
                    .all(|v| v.is_finite() && v.abs() <= 1000.0),
                "invalid anchor",
            )?;
            let old = project.layer_mut(object)?.clone();
            if old.locked {
                return Err(Error::Locked(object));
            }
            let compensate = |frame: f64| -> Result<[f32; 3]> {
                let previous = old.transform.anchor.sample(frame)?;
                let local_delta = Vec3::new((anchor[0]-previous[0])*old.size[0],
                    (previous[1]-anchor[1])*old.size[1],0.);
                let mut r = old.transform.rotation.sample(frame)?;
                if !old.three_d {
                    r[0] = 0.0;
                    r[1] = 0.0;
                }
                let rotation=motion_model::geometry::rotation_quaternion(r,old.rotation_order);
                let delta = rotation
                    * (Vec3::from_array(old.transform.scale.sample(frame)?) / 100.0 * local_delta);
                let pos = old.transform.position.sample(frame)?;
                Ok([pos[0] + delta.x, pos[1] - delta.y, pos[2] - delta.z])
            };
            let animated = old.transform.position.is_animated()
                || old.transform.anchor.is_animated()
                || old.transform.rotation.is_animated()
                || old.transform.scale.is_animated();
            let mut position = Track::constant(compensate(0.0)?);
            if animated {
                let first = old.clip(project.frames).edit_frame(0)?;
                let last = old.clip(project.frames).edit_frame(project.frames - 1)?;
                let key_frames = old
                    .transform
                    .position
                    .key_frames()
                    .chain(old.transform.anchor.key_frames())
                    .chain(old.transform.rotation.key_frames())
                    .chain(old.transform.scale.key_frames());
                let (first, last) =
                    key_frames.fold((first, last), |(a, b), f| (a.min(f), b.max(f)));
                ensure(
                    i64::from(last) - i64::from(first) < i64::from(crate::MAX_FRAMES),
                    "anchor compensation exceeds key budget",
                )?;
                for frame in first..=last {
                    position.upsert(frame, compensate(f64::from(frame))?, Ease::Linear)?;
                }
            }
            if old.transform.position.axes.is_some() {
                position.separate()?;
            }
            let layer = project.layer_mut(object)?;
            layer.transform.position = position;
            layer.transform.anchor = Track::constant(anchor);
        }
        Command::AnchorAt { object, frame, anchor } => {
            let local = project.edit_frame(object, frame)?;
            ensure(anchor.iter().all(|v|v.is_finite() && v.abs()<=1000.), "invalid anchor")?;
            let layer = project.layer_mut(object)?;
            ensure(layer.camera_data().is_none() && !matches!(layer.content, Content::Audio { .. }), "object has no layer anchor")?;
            if layer.locked { return Err(Error::Locked(object)); }
            let previous = layer.transform.anchor.sample(f64::from(local))?;
            let mut rotation = layer.transform.rotation.sample(f64::from(local))?;
            if !layer.three_d { rotation[0]=0.; rotation[1]=0.; }
            let delta = motion_model::geometry::rotation_quaternion(rotation,layer.rotation_order)
                * (Vec3::from_array(layer.transform.scale.sample(f64::from(local))?)/100.
                    * Vec3::new((anchor[0]-previous[0])*layer.size[0],(previous[1]-anchor[1])*layer.size[1],0.));
            let position = layer.transform.position.sample(f64::from(local))?;
            layer.transform.position.set_at(local,[position[0]+delta.x,position[1]-delta.y,position[2]-delta.z])?;
            layer.transform.anchor.set_at(local,anchor)?;
        }
    }
    project.rebuild_plugin_dependencies();
    project.validate()?;
    Ok(result)
}

struct History {
    project: Project,
    size: crate::history_accounting::ProjectSize,
}
struct Gesture {
    before: History,
    changes: crate::history_accounting::Changes,
    started: std::time::Instant,
}
pub struct Engine {
    project: Project,
    size: crate::history_accounting::ProjectSize,
    revision: u64,
    undo: VecDeque<History>,
    redo: VecDeque<History>,
    gesture: Option<Gesture>,
    history_budget: usize,
    history_evictions: u64,
}
impl Engine {
    pub fn activate_composition(&mut self, id: &str) -> Result<()> {
        if self.gesture.is_some() { return Err(Error::Gesture); }
        self.project.activate_composition(id)?;
        self.size = self
            .size
            .updated(&self.project, &crate::history_accounting::Changes::default());
        Ok(())
    }
    pub fn gesture_active(&self) -> bool { self.gesture.is_some() }
    pub fn new(project: Project) -> Result<Self> {
        project.validate()?;
        let size = crate::history_accounting::ProjectSize::new(&project);
        Ok(Self {
            project,
            size,
            revision: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            gesture: None,
            history_budget: 32 * 1024 * 1024,
            history_evictions: 0,
        })
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn snapshot(&self) -> Project {
        self.project.clone()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Runtime-only history pressure; eviction is the existing bounded policy.
    pub fn history_diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"undoEntries":self.undo.len(),"redoEntries":self.redo.len(),
            "budgetBytes":self.history_budget,"evictions":self.history_evictions,
            "gestureActive":self.gesture.is_some(),
            "gestureElapsedMs":self.gesture.as_ref().map(|g|g.started.elapsed().as_millis().min(u64::MAX as u128) as u64)})
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() && self.gesture.is_none()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && self.gesture.is_none()
    }
    pub fn begin_gesture(&mut self) -> Result<()> {
        if self.gesture.is_some() {
            log::warn!(
                "gesture begin rejected revision={} history={}",
                self.revision,
                self.history_diagnostics()
            );
            return Err(Error::Gesture);
        }
        self.gesture = Some(Gesture {
            before: History {
                project: self.project.clone(),
                size: self.size.clone(),
            },
            changes: crate::history_accounting::Changes::default(),
            started: std::time::Instant::now(),
        });
        log::debug!("gesture began revision={}", self.revision);
        Ok(())
    }
    pub fn end_gesture(&mut self, commit: bool) -> Result<()> {
        let gesture = self.gesture.take().ok_or_else(|| {
            log::warn!(
                "gesture close rejected revision={} commit={commit} reason=no_active_gesture",
                self.revision
            );
            Error::Gesture
        })?;
        let before = gesture.before;
        log::debug!(
            "gesture closing revision={} commit={commit} changed={} elapsed_ms={}",
            self.revision,
            before.project != self.project,
            gesture.started.elapsed().as_millis()
        );
        if before.project != self.project {
            if commit {
                self.size = self.size.updated(&self.project, &gesture.changes);
                self.push_undo(before);
                self.redo.clear();
            } else {
                self.project = before.project;
                self.size = before.size;
                self.revision = self.revision.wrapping_add(1);
            }
        }
        Ok(())
    }
    pub fn apply(&mut self, command: Command) -> Result<()> {
        self.apply_batch(vec![command]).map(|_| ())
    }
    /// All edits either commit together or restore the pre-edit state.
    pub fn apply_batch(&mut self, commands: Vec<Command>) -> Result<Vec<EditResult>> {
        crate::CommandRegistry::run(self, commands)
    }
    /// A failed preparation creates no history entry and preserves an active
    /// gesture. Its callback must not partially mutate external state on error.
    pub fn apply_batch_with_prepare(
        &mut self,
        commands: Vec<Command>,
        prepare: impl FnOnce(&Project) -> Result<()>,
    ) -> Result<Vec<EditResult>> {
        crate::CommandRegistry::run_prepared(self, commands, prepare)
    }
    /// Publish history and revision only after the project file is saved successfully.
    /// Used by media imports after their owned source has been staged and validated.
    pub fn apply_batch_saved(
        &mut self,
        commands: Vec<Command>,
        root: &std::path::Path,
    ) -> Result<Vec<EditResult>> {
        ensure(
            self.gesture.is_none(),
            "finish the active gesture before importing media",
        )?;
        crate::CommandRegistry::run_saved(self, commands, root)
    }
    pub(crate) fn apply_batch_inner(
        &mut self,
        commands: Vec<Command>,
        root: Option<&std::path::Path>,
    ) -> Result<Vec<EditResult>> {
        self.apply_batch_inner_prepared(commands, root, |_| Ok(()))
    }
    pub(crate) fn apply_batch_inner_prepared(
        &mut self,
        commands: Vec<Command>,
        root: Option<&std::path::Path>,
        prepare: impl FnOnce(&Project) -> Result<()>,
    ) -> Result<Vec<EditResult>> {
        if self.gesture.is_some() && commands.iter().any(Command::composition_transaction) {
            return Err(Error::Gesture);
        }
        let before = self.project.clone();
        let mut changes = crate::history_accounting::Changes::default();
        for command in &commands {
            changes.note(command, &self.project.composition_id);
        }
        let mut results = Vec::new();
        let prepared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            for command in commands {
                match apply_to(&mut self.project, command) {
                    Ok(Some(result)) => results.push(result),
                    Ok(None) => {}
                    Err(error) => return Err(error),
                }
            }
            prepare(&self.project)
        }));
        match prepared {
            Ok(Ok(())) => {},
            Ok(Err(error)) => {
                self.project = before;
                return Err(error);
            }
            Err(payload) => {
                self.project = before;
                std::panic::resume_unwind(payload);
            }
        }
        if before != self.project {
            if let Some(root) = root {
                if let Err(error) = crate::storage::save(root, &self.project) {
                    self.project = before;
                    return Err(error);
                }
            }
            if self.gesture.is_none() {
                let next_size = self.size.updated(&self.project, &changes);
                let before_size = std::mem::replace(&mut self.size, next_size);
                self.push_undo(History {
                    project: before,
                    size: before_size,
                });
                self.redo.clear();
            } else if let Some(gesture) = &mut self.gesture {
                // Continuous gesture updates create no snapshots or size walks.
                gesture.changes.merge(changes);
            }
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(results)
    }
    pub fn undo(&mut self) -> Result<bool> {
        if self.gesture.is_some() {
            log::warn!(
                "undo blocked revision={} history={}",
                self.revision,
                self.history_diagnostics()
            );
            return Err(Error::Gesture);
        }
        let Some(entry) = self.undo.pop_back() else {
            log::debug!("undo no-op revision={} reason=history_empty", self.revision);
            return Ok(false);
        };
        let previous = std::mem::replace(&mut self.project, entry.project);
        let previous_size = std::mem::replace(&mut self.size, entry.size);
        self.redo.push_back(History {
            size: previous_size,
            project: previous,
        });
        self.revision = self.revision.wrapping_add(1);
        self.trim();
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool> {
        if self.gesture.is_some() {
            log::warn!(
                "redo blocked revision={} history={}",
                self.revision,
                self.history_diagnostics()
            );
            return Err(Error::Gesture);
        }
        let Some(entry) = self.redo.pop_back() else {
            log::debug!("redo no-op revision={} reason=history_empty", self.revision);
            return Ok(false);
        };
        let previous = std::mem::replace(&mut self.project, entry.project);
        let previous_size = std::mem::replace(&mut self.size, entry.size);
        self.push_undo(History {
            project: previous,
            size: previous_size,
        });
        self.revision = self.revision.wrapping_add(1);
        self.trim();
        Ok(true)
    }
    fn push_undo(&mut self, entry: History) {
        self.undo.push_back(entry);
        self.trim();
    }
    fn trim(&mut self) {
        while self.undo.len() + self.redo.len() > 128
            || self
                .undo
                .iter()
                .chain(&self.redo)
                .map(|h| h.size.bytes())
                .sum::<usize>()
                > self.history_budget
        {
            if self.undo.pop_front().or_else(|| self.redo.pop_front()).is_some() {
                self.history_evictions = self.history_evictions.saturating_add(1);
            } else {
                break;
            }
        }
    }
}

impl motion_model::EditSink for Engine {
    fn project(&self) -> &Project { Engine::project(self) }
    fn apply_batch_saved(&mut self, commands: Vec<Command>, root: &std::path::Path) -> Result<Vec<EditResult>> {
        Engine::apply_batch_saved(self, commands, root)
    }
}

#[cfg(test)]
#[path = "history_accounting_tests.rs"]
mod history_accounting_tests;
