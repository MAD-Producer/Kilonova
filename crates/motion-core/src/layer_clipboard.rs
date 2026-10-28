//! Clipboard edits run inside the ordinary engine transaction and history path.
use crate::{
    ensure, Content, EditResult, EffectImageInput, Error, Layer, LayerClipboard, ParentLink,
    Project, Result,
};
use std::collections::{BTreeMap, BTreeSet};

fn next_id(last: &mut u64, kind: &str) -> Result<u64> {
    *last = last
        .checked_add(1)
        .ok_or_else(|| Error::Invalid(format!("{kind} ID space exhausted")))?;
    Ok(*last)
}

fn mapped(map: &BTreeMap<u64, u64>, old: u64, kind: &str) -> Result<u64> {
    map.get(&old)
        .copied()
        .ok_or_else(|| Error::Invalid(format!("clipboard {kind} mapping missing: source={old}")))
}

fn references(layer: &Layer) -> Vec<u64> {
    layer
        .parent
        .iter()
        .chain(layer.camera_data().into_iter().flat_map(|c| &c.parent))
        .filter_map(|p| p.object)
        .chain(layer.track_matte.iter().map(|matte| matte.source))
        .chain(
            layer
                .effects
                .iter()
                .filter_map(|effect| effect.image_input.and_then(EffectImageInput::layer)),
        )
        .collect()
}

fn remap_parent(parent: &mut Option<ParentLink>, objects: &BTreeMap<u64, u64>) {
    if let Some(parent) = parent {
        if let Some(id) = parent.object.and_then(|id| objects.get(&id)) {
            parent.object = Some(*id);
        }
    }
}

fn remap_content(
    layer: &mut Layer,
    assets: &BTreeMap<u64, u64>,
    fonts: &BTreeMap<String, String>,
    compositions: &BTreeMap<String, String>,
) -> Result<()> {
    match &mut layer.content {
        Content::Image { asset } => *asset = mapped(assets, *asset, "image")?,
        Content::Model { model } => model.asset = mapped(assets, model.asset, "model")?,
        Content::Camera { camera } => {
            if let Some(binding) = &mut camera.imported {
                binding.asset = mapped(assets, binding.asset, "imported camera")?;
            }
        }
        Content::Text {
            font, raster_asset, ..
        } => {
            *raster_asset = mapped(assets, *raster_asset, "text raster")?;
            if let Some(id) = fonts.get(font) {
                *font = id.clone();
            }
        }
        Content::Video { video } => video.asset = mapped(assets, video.asset, "video")?,
        Content::Audio { audio } => audio.asset = mapped(assets, audio.asset, "audio")?,
        Content::Composition { clip } => {
            clip.composition = compositions
                .get(&clip.composition)
                .cloned()
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "clipboard composition mapping missing: source={}",
                        clip.composition
                    ))
                })?;
        }
        _ => {}
    }
    if let Some(calibration) = &mut layer.depth_calibration {
        calibration.remap_asset(mapped(assets, calibration.asset(), "calibrated source")?);
    }
    if let Some(track) = &mut layer.video_depth_calibration {
        let Content::Video { video } = &layer.content else {
            return Err(Error::Invalid("video depth clipboard source is not video".into()));
        };
        track.remap_asset(video.asset);
    }
    for effect in &mut layer.effects {
        if let Some(EffectImageInput::Asset { asset }) = &mut effect.image_input {
            *asset = mapped(assets, *asset, "effect image")?;
        }
    }
    Ok(())
}

/// External layer references can only be retained when the host establishes
/// that both sides still belong to the same document and composition.
pub(crate) fn paste(
    project: &mut Project,
    clipboard: LayerClipboard,
    at_frame: Option<u32>,
    keep_external_references: bool,
) -> Result<EditResult> {
    let view = clipboard.resolve()?;
    ensure(
        project.layers.len() + view.layers.len() <= crate::MAX_LAYERS,
        "layer limit exceeded",
    )?;
    if let Some(frame) = at_frame {
        ensure(frame < project.frames, "paste frame outside composition")?;
    }
    let mut last = project.layers.iter().map(|l| l.id).max().unwrap_or(0);
    let mut objects = BTreeMap::new();
    for layer in &view.layers {
        objects.insert(layer.id, next_id(&mut last, "layer")?);
    }
    for layer in &view.layers {
        for reference in references(layer) {
            if objects.contains_key(&reference) {
                continue;
            }
            ensure(keep_external_references,
                &format!("paste requires copying the referenced layer together: object={} dependency={reference}", layer.id))?;
            ensure(
                if reference == 0 {
                    project.camera.created
                } else {
                    project.layers.iter().any(|l| l.id == reference)
                },
                &format!(
                    "paste external reference missing: object={} dependency={reference}",
                    layer.id
                ),
            )?;
        }
    }

    let mut composition_ids: BTreeSet<_> = project.composition_ids().into_iter().collect();
    let mut compositions = BTreeMap::new();
    for source in &view.compositions {
        let id = (1..=crate::composition::MAX_COMPOSITIONS + 1)
            .map(|i| format!("comp-paste-{i}"))
            .find(|id| !composition_ids.contains(id))
            .ok_or_else(|| Error::Invalid("clipboard composition IDs exhausted".into()))?;
        composition_ids.insert(id.clone());
        compositions.insert(source.id.clone(), id);
    }
    ensure(
        composition_ids.len() <= crate::composition::MAX_COMPOSITIONS,
        "composition limit exceeded",
    )?;

    let mut assets = BTreeMap::new();
    let mut last = project
        .assets
        .iter()
        .map(|a| a.id)
        .chain(project.audio_assets.iter().map(|a| a.id))
        .chain(project.video_assets.iter().map(|a| a.id))
        .chain(project.model_assets.iter().map(|a| a.id))
        .max()
        .unwrap_or(0);
    // A same-document paste reuses an identical registered source. A host
    // transferring between roots assigns owned destination paths beforehand.
    macro_rules! import_assets {
        ($field:ident) => {
            for source in &view.$field {
                let mut asset = (*source).clone();
                if let Some(existing) = project
                    .$field
                    .iter()
                    .find(|a| a.path.eq_ignore_ascii_case(&asset.path))
                {
                    asset.id = existing.id;
                    ensure(
                        &asset == existing,
                        &format!("paste source metadata conflict: path={}", asset.path),
                    )?;
                } else {
                    asset.id = next_id(&mut last, "asset")?;
                    project.$field.push(asset.clone());
                }
                assets.insert(source.id, asset.id);
            }
        };
    }
    import_assets!(assets);
    import_assets!(model_assets);
    import_assets!(audio_assets);
    for source in &view.video_assets {
        let mut asset = (*source).clone();
        asset.audio_asset = asset
            .audio_asset
            .map(|id| mapped(&assets, id, "video audio"))
            .transpose()?;
        if let Some(existing) = project
            .video_assets
            .iter()
            .find(|a| a.path.eq_ignore_ascii_case(&asset.path))
        {
            asset.id = existing.id;
            ensure(
                &asset == existing,
                &format!("paste video metadata conflict: path={}", asset.path),
            )?;
        } else {
            asset.id = next_id(&mut last, "asset")?;
            project.video_assets.push(asset.clone());
        }
        assets.insert(source.id, asset.id);
    }
    let mut fonts = BTreeMap::new();
    for source in &view.fonts {
        let mut font = (*source).clone();
        if let Some(existing) = project.fonts.iter().find(|a| a.path == font.path) {
            font.id = existing.id.clone();
            ensure(
                &font == existing,
                &format!("paste font metadata conflict: path={}", font.path),
            )?;
        } else {
            if project.fonts.iter().any(|f| f.id == font.id) {
                font.id = (1..=129)
                    .map(|i| format!("clipboard-font-{i}"))
                    .find(|id| !project.fonts.iter().any(|f| &f.id == id))
                    .ok_or_else(|| Error::Invalid("clipboard font IDs exhausted".into()))?;
            }
            project.fonts.push(font.clone());
        }
        fonts.insert(source.id.clone(), font.id);
    }
    for source in &view.compositions {
        let mut composition = (*source).clone();
        composition.id = compositions[&source.id].clone();
        if let Some(binding) = &mut composition.camera.imported {
            binding.asset = mapped(&assets, binding.asset, "composition imported camera")?;
        }
        for layer in &mut composition.layers {
            remap_content(layer, &assets, &fonts, &compositions)?;
        }
        project.compositions.push(composition);
    }

    let anchor = view
        .layers
        .iter()
        .map(|l| l.interval(clipboard.source.frames).in_frame.floor() as i64)
        .min()
        .ok_or_else(|| Error::Invalid("layer clipboard is empty".into()))?;
    let delta = at_frame.map_or(0, |frame| i64::from(frame) - anchor);
    for source in &view.layers {
        let mut layer = (*source).clone();
        layer.id = objects[&source.id];
        layer.locked = false;
        let mut timeline = layer.clip(clipboard.source.frames);
        let shift = |frame: i32| {
            i32::try_from(i64::from(frame) + delta)
                .map_err(|_| Error::Invalid("paste layer time overflow".into()))
        };
        timeline.in_frame = shift(timeline.in_frame)?;
        timeline.out_frame = shift(timeline.out_frame)?;
        timeline.offset_frame = shift(timeline.offset_frame)?;
        layer.timeline = Some(timeline);
        layer.subframe_range = layer.subframe_range.map(|range| range.translated(delta));
        remap_parent(&mut layer.parent, &objects);
        if let Content::Camera { camera } = &mut layer.content {
            remap_parent(&mut camera.parent, &objects);
        }
        if let Some(matte) = &mut layer.track_matte {
            if let Some(id) = objects.get(&matte.source) {
                matte.source = *id;
            }
        }
        for effect in &mut layer.effects {
            if let Some(EffectImageInput::Layer { layer, .. }) = &mut effect.image_input {
                if let Some(id) = objects.get(layer) {
                    *layer = *id;
                }
            }
        }
        remap_content(&mut layer, &assets, &fonts, &compositions)?;
        project.layers.push(layer);
    }
    project.rebuild_plugin_dependencies();
    for source in view.expressions {
        let mut expression = source.clone();
        expression
            .target
            .set_object(objects[&source.target.object()]);
        crate::expressions::set(project, expression, at_frame.unwrap_or(0))?;
    }
    Ok(EditResult::PasteLayers {
        objects: clipboard.objects.iter().map(|id| objects[id]).collect(),
    })
}

pub(crate) fn remove(project: &mut Project, objects: Vec<u64>, frame: u32,
    cameras: &dyn crate::CameraSampler) -> Result<()> {
    ensure(frame < project.frames, "cut frame outside composition")?;
    let ids: BTreeSet<_> = objects.iter().copied().collect();
    ensure(
        !ids.is_empty() && ids.len() == objects.len() && !ids.contains(&0),
        "select unique layers to cut",
    )?;
    for id in &ids {
        if project.layer_mut(*id)?.locked {
            return Err(Error::Locked(*id));
        }
    }
    for layer in project.layers.iter().filter(|l| !ids.contains(&l.id)) {
        ensure(
            !layer.track_matte.is_some_and(|m| ids.contains(&m.source)),
            &format!(
                "cut layer is referenced by an unselected track matte: object={}",
                layer.id
            ),
        )?;
    }
    let children: Vec<_> = project
        .layers
        .iter()
        .filter(|l| !ids.contains(&l.id))
        .filter(|l| {
            l.parent
                .as_ref()
                .and_then(|p| p.object)
                .is_some_and(|id| ids.contains(&id))
                || l.camera_data()
                    .and_then(|c| c.parent.as_ref())
                    .and_then(|p| p.object)
                    .is_some_and(|id| ids.contains(&id))
        })
        .map(|l| l.id)
        .collect();
    for child in children {
        crate::hierarchy::reparent(project, child, None, frame, cameras)?;
    }
    if project
        .camera
        .parent
        .as_ref()
        .and_then(|p| p.object)
        .is_some_and(|id| ids.contains(&id))
    {
        crate::hierarchy::reparent(project, 0, None, frame, cameras)?;
    }
    project.layers.retain(|l| !ids.contains(&l.id));
    project
        .expressions
        .retain(|e| !ids.contains(&e.target.object()));
    for id in ids {
        crate::effects::clear_layer_image_inputs(&mut project.layers, id);
    }
    Ok(())
}
