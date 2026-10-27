//! Independently authored animation recipes. Coordinates are pixels; key times
//! are source frames relative to the first captured key, never wall-clock time.
use crate::{
    ensure, Asset, Content, EffectImageInput, EffectInstance, ExpressionTarget, Keyframe, Layer,
    PluginDependency, Project, Property, Result, Track,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ANIMATION_PRESET_VERSION: u32 = 1;
pub const MAX_ANIMATION_PRESET_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "property",
    content = "track",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PresetProperty {
    Anchor(Track<[f32; 2]>),
    Position(Track<[f32; 3]>),
    Rotation(Track<[f32; 3]>),
    Scale(Track<[f32; 3]>),
    Opacity(Track<f32>),
}
impl PresetProperty {
    pub fn property(&self) -> Property {
        match self {
            Self::Anchor(_) => Property::Anchor,
            Self::Position(_) => Property::Position,
            Self::Rotation(_) => Property::Rotation,
            Self::Scale(_) => Property::Scale,
            Self::Opacity(_) => Property::Opacity,
        }
    }
    fn times(&mut self, map: &mut impl FnMut(i32) -> Result<i32>) -> Result<()> {
        match self {
            Self::Anchor(t) => track_times(t, map),
            Self::Position(t) | Self::Rotation(t) | Self::Scale(t) => track_times(t, map),
            Self::Opacity(t) => track_times(t, map),
        }
    }
    fn install(&self, layer: &mut Layer) -> Result<()> {
        match self {
            Self::Anchor(t) => {
                let mut t = t.clone();
                anchor_scale(&mut t, [1. / layer.size[0], 1. / layer.size[1]]);
                layer.transform.anchor = t;
            }
            Self::Position(t) => layer.transform.position = t.clone(),
            Self::Rotation(t) => layer.transform.rotation = t.clone(),
            Self::Scale(t) => layer.transform.scale = t.clone(),
            Self::Opacity(t) => layer.transform.opacity = t.clone(),
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationPreset {
    pub version: u32,
    pub name: String,
    pub category: String,
    pub source_fps: u32,
    pub source_size: [f32; 2],
    pub properties: Vec<PresetProperty>,
    pub effects: Vec<EffectInstance>,
    pub dependencies: Vec<PluginDependency>,
    /// Only effect image resources, never unrelated source content or user text.
    pub assets: Vec<Asset>,
}

fn key_times<T>(keys: &mut [Keyframe<T>], map: &mut impl FnMut(i32) -> Result<i32>) -> Result<()> {
    for key in keys.iter_mut() {
        key.frame = map(key.frame)?;
    }
    ensure(
        keys.windows(2).all(|pair| pair[0].frame < pair[1].frame),
        "preset frame-rate conversion merges distinct keys",
    )
}
fn track_times<T>(track: &mut Track<T>, map: &mut impl FnMut(i32) -> Result<i32>) -> Result<()> {
    key_times(&mut track.keys, map)?;
    if let Some(axes) = &mut track.axes {
        for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
            track_times(axis, map)?;
        }
    }
    Ok(())
}
fn anchor_scale(track: &mut Track<[f32; 2]>, scale: [f32; 2]) {
    let scale_value = |v: &mut [f32; 2]| {
        for i in 0..2 {
            v[i] *= scale[i];
        }
    };
    scale_value(&mut track.value);
    for key in &mut track.keys {
        scale_value(&mut key.value);
        if let Some(spatial) = &mut key.spatial {
            if let Some(v) = &mut spatial.incoming {
                scale_value(v);
            }
            if let Some(v) = &mut spatial.outgoing {
                scale_value(v);
            }
        }
    }
}
impl AnimationPreset {
    /// Empty property/effect selections are deliberate, not shorthand for all.
    pub fn capture(
        project: &Project,
        object: u64,
        name: String,
        category: String,
        properties: &[Property],
        effects: &[u64],
    ) -> Result<Self> {
        project.validate()?;
        let layer = project
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or_else(|| crate::Error::Invalid("preset source layer missing".into()))?;
        ensure(
            !matches!(
                layer.content,
                Content::Camera { .. } | Content::Audio { .. }
            ),
            "preset source has incompatible properties",
        )?;
        let mut result = Self {
            version: ANIMATION_PRESET_VERSION,
            name,
            category,
            source_fps: project.fps,
            source_size: layer.size,
            properties: Vec::new(),
            effects: Vec::new(),
            dependencies: Vec::new(),
            assets: Vec::new(),
        };
        for property in properties {
            ensure(
                !result.properties.iter().any(|p| p.property() == *property),
                "duplicate preset property",
            )?;
            result.properties.push(match property {
                Property::Anchor => {
                    let mut t = layer.transform.anchor.clone();
                    anchor_scale(&mut t, layer.size);
                    PresetProperty::Anchor(t)
                }
                Property::Position => PresetProperty::Position(layer.transform.position.clone()),
                Property::Rotation => PresetProperty::Rotation(layer.transform.rotation.clone()),
                Property::Scale => PresetProperty::Scale(layer.transform.scale.clone()),
                Property::Opacity => PresetProperty::Opacity(layer.transform.opacity.clone()),
                _ => return Err(crate::Error::Invalid("unsupported preset property".into())),
            });
        }
        let ids: BTreeSet<_> = effects.iter().copied().collect();
        ensure(ids.len() == effects.len(), "duplicate preset effect")?;
        for id in &ids {
            ensure(
                layer.effects.iter().any(|e| e.id == *id),
                "preset effect missing",
            )?;
        }
        // Preserve stack order, independently of selection order.
        result.effects = layer
            .effects
            .iter()
            .filter(|e| ids.contains(&e.id))
            .cloned()
            .collect();
        for expression in &project.expressions {
            let selected = match &expression.target {
                ExpressionTarget::Property {
                    object: o,
                    property,
                    ..
                } => *o == object && properties.contains(property),
                ExpressionTarget::Effect {
                    object: o, effect, ..
                } => *o == object && ids.contains(effect),
            };
            ensure(
                !selected,
                "preset selection contains an expression; remove it before saving",
            )?;
        }
        let assets: BTreeSet<_> = result
            .effects
            .iter()
            .flat_map(|e| {
                let input = match e.image_input {
                    Some(EffectImageInput::Asset { asset }) => Some(asset),
                    _ => None,
                };
                input
                    .into_iter()
                    .chain(e.scene.as_ref().and_then(|s| s.sprite_asset))
            })
            .collect();
        result.assets = project
            .assets
            .iter()
            .filter(|a| assets.contains(&a.id))
            .cloned()
            .collect();
        result.dependencies = result
            .effects
            .iter()
            .map(EffectInstance::dependency)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut first = None;
        result.times(&mut |frame| {
            first = Some(first.map_or(frame, |v: i32| v.min(frame)));
            Ok(frame)
        })?;
        let first = first.unwrap_or(0);
        result.times(&mut |frame| {
            frame
                .checked_sub(first)
                .ok_or_else(|| crate::Error::Invalid("preset time overflow".into()))
        })?;
        result.validate()?;
        Ok(result)
    }
    fn times(&mut self, map: &mut impl FnMut(i32) -> Result<i32>) -> Result<()> {
        for property in &mut self.properties {
            property.times(map)?;
        }
        for effect in &mut self.effects {
            for param in effect.params.values_mut() {
                track_times(&mut param.track, map)?;
                if let Some(curve) = &mut param.curve {
                    key_times(&mut curve.keys, map)?;
                }
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.version == ANIMATION_PRESET_VERSION,
            "unsupported animation preset version",
        )?;
        for (value, limit) in [(&self.name, 128), (&self.category, 64)] {
            ensure(
                !value.trim().is_empty()
                    && value.len() <= limit
                    && !value.chars().any(char::is_control),
                "invalid preset name/category",
            )?;
        }
        ensure(
            (1..=crate::MAX_COMPOSITION_FPS).contains(&self.source_fps),
            "invalid preset fps",
        )?;
        ensure(
            self.source_size
                .into_iter()
                .all(|v| v.is_finite() && v > 0. && v <= 10_000_000.),
            "invalid preset source size",
        )?;
        ensure(
            self.properties.len() <= 5
                && self.effects.len() <= motion_effects::MAX_EFFECTS_PER_LAYER,
            "preset scope exceeds limit",
        )?;
        ensure(
            !self.properties.is_empty() || !self.effects.is_empty(),
            "preset selection is empty",
        )?;
        let mut seen = BTreeSet::new();
        let mut layer = Layer::solid(1, "Preset validation", self.source_size, [0.; 3], [1.; 4]);
        for property in &self.properties {
            ensure(
                seen.insert(format!("{:?}", property.property())),
                "duplicate preset property",
            )?;
            property.install(&mut layer)?;
        }
        let mut asset_ids = BTreeSet::new();
        for effect in &self.effects {
            ensure(
                !matches!(effect.image_input, Some(EffectImageInput::Layer { .. }))
                    && effect
                        .scene
                        .as_ref()
                        .is_none_or(|s| s.source_layer.is_none()),
                "preset layer input must be replaced by a portable image resource",
            )?;
            if let Some(EffectImageInput::Asset { asset }) = effect.image_input {
                asset_ids.insert(asset);
            }
            if let Some(asset) = effect.scene.as_ref().and_then(|s| s.sprite_asset) {
                asset_ids.insert(asset);
            }
        }
        let expected: Vec<_> = self
            .effects
            .iter()
            .map(EffectInstance::dependency)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        ensure(
            self.dependencies == expected,
            "preset dependency declaration does not match its effects",
        )?;
        ensure(
            self.assets.iter().map(|a| a.id).collect::<BTreeSet<_>>() == asset_ids
                && self.assets.len() == asset_ids.len(),
            "preset resource declaration does not match its effects",
        )?;
        layer.effects = self.effects.clone();
        let mut project = Project::new(16, 16, self.source_fps, crate::MAX_FRAMES)?;
        project.layers.push(layer);
        project.assets = self.assets.clone();
        project.rebuild_plugin_dependencies();
        project.validate()?;
        let mut copy = self.clone();
        copy.times(&mut |frame| {
            ensure(
                frame >= 0,
                "preset keys must use relative non-negative frames",
            )?;
            Ok(frame)
        })?;
        ensure(
            crate::serialized_bytes(self) <= MAX_ANIMATION_PRESET_BYTES,
            "animation preset exceeds 16 MiB",
        )
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        ensure(
            bytes.len() <= MAX_ANIMATION_PRESET_BYTES,
            "animation preset exceeds 16 MiB",
        )?;
        let preset: Self = serde_json::from_slice(bytes)?;
        preset.validate()?;
        Ok(preset)
    }
    /// Atomic even for direct model clients. Existing effects are appended to;
    /// selected transform tracks are replaced, with one transaction in the core.
    pub fn apply(&self, project: &mut Project, objects: &[u64], at_frame: u32) -> Result<()> {
        self.validate()?;
        ensure(
            at_frame < project.frames,
            "preset playhead is outside the composition",
        )?;
        let unique: BTreeSet<_> = objects.iter().copied().collect();
        ensure(
            !objects.is_empty()
                && objects.len() <= crate::MAX_LAYERS
                && unique.len() == objects.len(),
            "invalid preset targets",
        )?;
        let mut candidate = project.clone();
        let mut assets = BTreeMap::new();
        let mut next_asset = candidate
            .assets
            .iter()
            .map(|a| a.id)
            .chain(candidate.audio_assets.iter().map(|a| a.id))
            .chain(candidate.video_assets.iter().map(|a| a.id))
            .chain(candidate.model_assets.iter().map(|a| a.id))
            .max()
            .unwrap_or(0);
        for source in &self.assets {
            let id = if let Some(existing) = candidate.assets.iter().find(|a| a.path == source.path)
            {
                ensure(
                    existing.width == source.width && existing.height == source.height,
                    "preset resource path has different metadata",
                )?;
                existing.id
            } else {
                next_asset = next_asset
                    .checked_add(1)
                    .ok_or_else(|| crate::Error::Invalid("preset asset ID overflow".into()))?;
                let mut asset = source.clone();
                asset.id = next_asset;
                candidate.assets.push(asset);
                next_asset
            };
            assets.insert(source.id, id);
        }
        let mut next_effect = candidate
            .layers
            .iter()
            .flat_map(|l| &l.effects)
            .map(|e| e.id)
            .max()
            .unwrap_or(0);
        for object in objects {
            for expression in &candidate.expressions {
                if let ExpressionTarget::Property {
                    object: owner,
                    property,
                    ..
                } = expression.target
                {
                    ensure(
                        owner != *object
                            || !self.properties.iter().any(|p| p.property() == property),
                        "preset target property has an expression",
                    )?;
                }
            }
            let fps = candidate.fps;
            let layer = candidate.layer_mut(*object)?;
            ensure(!layer.locked, "preset target layer is locked")?;
            ensure(
                !matches!(
                    layer.content,
                    Content::Camera { .. } | Content::Audio { .. }
                ),
                "preset target has incompatible properties",
            )?;
            ensure(
                self.effects.is_empty() || !matches!(layer.content, Content::Null),
                "preset effects require a visual layer",
            )?;
            ensure(
                layer.effects.len() + self.effects.len() <= motion_effects::MAX_EFFECTS_PER_LAYER,
                "preset exceeds target effect limit",
            )?;
            let offset = layer.timeline.map_or(0, |t| t.offset_frame);
            let mut preset = self.clone();
            preset.times(&mut |frame| {
                let relative =
                    (f64::from(frame) * f64::from(fps) / f64::from(self.source_fps)).round();
                let local = relative + f64::from(at_frame) - f64::from(offset);
                ensure(
                    local >= f64::from(i32::MIN) && local <= f64::from(i32::MAX),
                    "preset target time overflow",
                )?;
                Ok(local as i32)
            })?;
            for property in &preset.properties {
                property.install(layer)?;
            }
            for mut effect in preset.effects {
                next_effect = next_effect
                    .checked_add(1)
                    .ok_or_else(|| crate::Error::Invalid("preset effect ID overflow".into()))?;
                effect.id = next_effect;
                if let Some(EffectImageInput::Asset { asset }) = &mut effect.image_input {
                    *asset = *assets.get(asset).ok_or_else(|| {
                        crate::Error::Invalid("preset resource mapping missing".into())
                    })?;
                }
                if let Some(asset) = effect.scene.as_mut().and_then(|s| s.sprite_asset.as_mut()) {
                    *asset = *assets.get(asset).ok_or_else(|| {
                        crate::Error::Invalid("preset sprite resource mapping missing".into())
                    })?;
                }
                layer.effects.push(effect);
            }
        }
        candidate.rebuild_plugin_dependencies();
        candidate.validate()?;
        *project = candidate;
        Ok(())
    }
}
