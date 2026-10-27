//! Frozen layer metadata. Managed source bytes remain the host's responsibility.
use crate::{
    ensure, Asset, AudioAsset, Composition, Content, EffectImageInput, Error, FontAsset, Layer,
    Project, PropertyExpression, Result, VideoAsset, MAX_LAYERS,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One application clipboard snapshot, independent of project history. Keeping
/// the validated source document also freezes nested composition definitions;
/// only the selected layers' resource closure is imported when pasting.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerClipboard {
    pub source: Box<Project>,
    pub objects: Vec<u64>,
}

pub struct LayerClipboardView<'a> {
    /// Source stack order, independent of the order in which layers were selected.
    pub layers: Vec<&'a Layer>,
    pub compositions: Vec<&'a Composition>,
    pub assets: Vec<&'a Asset>,
    pub model_assets: Vec<&'a crate::ModelAsset>,
    pub fonts: Vec<&'a FontAsset>,
    pub audio_assets: Vec<&'a AudioAsset>,
    pub video_assets: Vec<&'a VideoAsset>,
    pub expressions: Vec<&'a PropertyExpression>,
}

impl LayerClipboard {
    pub fn capture(source: &Project, objects: Vec<u64>) -> Result<Self> {
        ensure(
            crate::serialized_bytes(source) <= 16 * 1024 * 1024,
            "layer clipboard metadata exceeds 16 MiB",
        )?;
        let clipboard = Self {
            source: Box::new(source.clone()),
            objects,
        };
        clipboard.resolve()?;
        Ok(clipboard)
    }

    pub fn resolve(&self) -> Result<LayerClipboardView<'_>> {
        ensure(
            crate::serialized_bytes(self) <= 16 * 1024 * 1024,
            "layer clipboard metadata exceeds 16 MiB",
        )?;
        self.source.validate()?;
        ensure(
            !self.objects.is_empty() && self.objects.len() <= MAX_LAYERS,
            "select 1..128 layers to copy",
        )?;
        let selected: BTreeSet<_> = self.objects.iter().copied().collect();
        ensure(
            !selected.contains(&0) && selected.len() == self.objects.len(),
            "layer clipboard selection must contain unique nonzero IDs",
        )?;
        let layers: Vec<_> = self
            .source
            .layers
            .iter()
            .filter(|l| selected.contains(&l.id))
            .collect();
        for id in &selected {
            ensure(
                layers.iter().any(|l| l.id == *id),
                &format!("copy layer missing: object={id}"),
            )?;
        }

        let mut compositions = Vec::new();
        let mut pending: Vec<_> = layers
            .iter()
            .filter_map(|layer| match &layer.content {
                Content::Composition { clip } => Some(clip.composition.as_str()),
                _ => None,
            })
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let composition = self
                .source
                .compositions
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| {
                    Error::Invalid(format!("clipboard composition missing: composition={id}"))
                })?;
            pending.extend(
                composition
                    .layers
                    .iter()
                    .filter_map(|layer| match &layer.content {
                        Content::Composition { clip } => Some(clip.composition.as_str()),
                        _ => None,
                    }),
            );
            compositions.push(composition);
        }

        let mut images = BTreeSet::new();
        let mut models = BTreeSet::new();
        let mut fonts = BTreeSet::new();
        let mut audio = BTreeSet::new();
        let mut video = BTreeSet::new();
        for layer in layers
            .iter()
            .copied()
            .chain(compositions.iter().flat_map(|c| &c.layers))
        {
            match &layer.content {
                Content::Model { model } => { models.insert(model.asset); }
                Content::Image { asset } => {
                    images.insert(*asset);
                }
                Content::Text {
                    font, raster_asset, ..
                } => {
                    fonts.insert(font.as_str());
                    images.insert(*raster_asset);
                }
                Content::Audio { audio: clip } => {
                    audio.insert(clip.asset);
                }
                Content::Video { video: clip } => {
                    video.insert(clip.asset);
                }
                _ => {}
            }
            for effect in &layer.effects {
                if let Some(EffectImageInput::Asset { asset }) = effect.image_input {
                    images.insert(asset);
                }
            }
        }
        // Video and its recoverable audio share one source, even when muted.
        let video_assets: Vec<_> = self
            .source
            .video_assets
            .iter()
            .filter(|a| video.contains(&a.id))
            .collect();
        for asset in &video_assets {
            if let Some(id) = asset.audio_asset {
                audio.insert(id);
            }
        }
        Ok(LayerClipboardView {
            layers,
            compositions,
            model_assets: self.source.model_assets.iter().filter(|a| models.contains(&a.id)).collect(),
            assets: self
                .source
                .assets
                .iter()
                .filter(|a| images.contains(&a.id))
                .collect(),
            fonts: self
                .source
                .fonts
                .iter()
                .filter(|a| fonts.contains(a.id.as_str()))
                .collect(),
            audio_assets: self
                .source
                .audio_assets
                .iter()
                .filter(|a| audio.contains(&a.id))
                .collect(),
            video_assets,
            expressions: self
                .source
                .expressions
                .iter()
                .filter(|e| selected.contains(&e.target.object()))
                .collect(),
        })
    }
}
