use crate::{ensure, Camera, Result, Track};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The only project JSON schema accepted by this build.
pub const CURRENT_PROJECT_VERSION: u32 = 29;

pub const MAX_LAYERS: usize = 128;
pub const MAX_FRAMES: u32 = 36_000;
pub const MAX_COMPOSITION_FPS: u32 = 240;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: u64,
    pub path: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontAsset {
    pub id: String,
    pub path: String,
    pub name: String,
    pub face_index: u32,
    pub license: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Model { model: crate::ModelClip },
    /// Independent, non-pixel camera in the composition layer stack.
    Camera { camera: Box<Camera> },
    Adjustment,
    Vector {
        vector: crate::vector::VectorContent,
    },
    Composition { clip: crate::CompositionClip },
    Video {
        video: crate::VideoClip,
    },
    Audio {
        audio: crate::AudioClip,
    },
    Null,
    Solid {
        color: [f32; 4],
    },
    Image {
        asset: u64,
    },
    Text {
        text: String,
        font: String,
        style: crate::TextStyle,
        color: [f32; 4],
        raster_asset: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub position: Track<[f32; 3]>,
    pub rotation: Track<[f32; 3]>,
    pub scale: Track<[f32; 3]>,
    pub opacity: Track<f32>,
    /// Normalized anchor in the layer's source rectangle, with Y pointing down.
    #[serde(with = "anchor_serde")]
    pub anchor: Track<[f32; 2]>,
}
impl Transform {
    pub fn new(position: [f32; 3]) -> Self {
        Self {
            position: Track::constant(position),
            rotation: Track::constant([0.0; 3]),
            scale: Track::constant([100.0; 3]),
            opacity: Track::constant(1.0),
            anchor: Track::constant([0.5; 2]),
        }
    }
    pub fn validate(&self, frames: u32) -> Result<()> {
        self.validate_tracks(Some(frames))
    }
    pub fn validate_local(&self) -> Result<()> {
        self.validate_tracks(None)
    }
    fn validate_tracks(&self, frames: Option<u32>) -> Result<()> {
        if let Some(frames) = frames {
            self.position.validate(frames)?;
            self.rotation.validate(frames)?;
            self.scale.validate(frames)?;
            self.opacity.validate(frames)?;
            self.anchor.validate(frames)?;
        } else {
            self.position.validate_local()?;
            self.rotation.validate_local()?;
            self.scale.validate_local()?;
            self.opacity.validate_local()?;
            self.anchor.validate_local()?;
        }
        self.anchor.validate_bound(1000.)?;
        ensure(self.anchor.axes.is_none(), "anchor does not support separated axes")?;
        for (track, bound) in [
            (&self.position, 10_000_000.0),
            (&self.rotation, 1_000_000.0),
            (&self.scale, 100_000.0),
        ] {
            track.validate_bound(bound)?;
        }
        for value in
            std::iter::once(self.opacity.value).chain(self.opacity.keys.iter().map(|k| k.value))
        {
            ensure(
                (0.0..=1.0).contains(&value),
                "opacity must be between zero and one",
            )?;
        }
        Ok(())
    }
}

// Static anchors use a compact array; animated anchors carry a complete Track.
mod anchor_serde {
    use super::*;
    pub fn serialize<S: serde::Serializer>(value: &Track<[f32; 2]>, s: S) -> std::result::Result<S::Ok, S::Error> {
        if !value.is_animated() && value.axes.is_none() { value.value.serialize(s) }
        else { value.serialize(s) }
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Track<[f32; 2]>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored { Static([f32; 2]), Animated(Track<[f32; 2]>) }
        Ok(match Stored::deserialize(d)? {
            Stored::Static(value) => Track::constant(value),
            Stored::Animated(track) => track,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub content: Content,
    pub size: [f32; 2],
    pub transform: Transform,
    pub visible: bool,
    pub locked: bool,
    /// Output isolation within this composition, shared by preview and export.
    pub solo: bool,
    /// Bypass the chain without changing any individual effect's enabled state.
    pub effects_enabled: bool,
    /// Editor list organization only; never affects render visibility.
    pub shy: bool,
    /// New layers are flat until the user explicitly enables spatial transforms.
    pub three_d: bool,
    pub sampling: crate::render_sampling::LayerSampling,
    pub rotation_order:crate::geometry::RotationOrder,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentLink>,
    #[serde(default)]
    pub effects: Vec<crate::EffectInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masks: Vec<crate::masks::LayerMask>,
    pub blend:crate::compositing::LayerBlend,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub track_matte:Option<crate::compositing::TrackMatte>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<LayerTimeline>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub subframe_range:Option<crate::ClipRange>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub audio_levels: Option<crate::AudioLevels>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_markers: Vec<crate::AudioMarker>,
    pub audio_source_channel: crate::AudioSourceChannel,
}
impl Layer {
    pub fn solid(id: u64, name: &str, size: [f32; 2], position: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            id,
            name: name.into(),
            content: Content::Solid { color },
            size,
            transform: Transform::new(position),
            visible: true,
            locked: false,
            solo: false,
            effects_enabled: true,
            shy: false,
            three_d: false,
            sampling: Default::default(),
            rotation_order:Default::default(),
            parent: None,
            effects: Vec::new(),
            masks: Vec::new(),
            blend:Default::default(),track_matte:None,
            timeline: None,
            subframe_range:None,
            audio_levels: None,
            audio_markers: Vec::new(),
            audio_source_channel: Default::default(),
        }
    }
    pub fn camera(id: u64, name: &str, width: u32, height: u32) -> Self {
        let mut layer = Self::solid(id, name, [1.; 2], [0.; 3], [0.; 4]);
        layer.content = Content::Camera { camera: Box::new(Camera::new(width, height)) };
        layer.three_d = true;
        layer
    }
    pub fn camera_data(&self) -> Option<&Camera> {
        match &self.content { Content::Camera { camera } => Some(camera), _ => None }
    }
    pub fn clip(&self, frames: u32) -> LayerTimeline {
        self.timeline.unwrap_or_else(|| LayerTimeline::full(frames))
    }
    pub fn local_frame(&self, composition_frame: f64) -> f64 {
        composition_frame - f64::from(self.timeline.map_or(0, |t| t.offset_frame))
    }
    pub fn active(&self, frame: f64, frames: u32) -> bool {
        let t = self.interval(frames);
        self.visible && frame >= 0. && frame < f64::from(frames)
            && frame >= t.in_frame && frame < t.out_frame
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerTimeline {
    pub in_frame: i32,
    pub out_frame: i32,
    pub offset_frame: i32,
}
impl LayerTimeline {
    pub fn full(frames: u32) -> Self {
        Self {
            in_frame: 0,
            out_frame: frames as i32,
            offset_frame: 0,
        }
    }
    pub fn validate(&self, frames: u32) -> Result<()> {
        ensure(
            self.in_frame < self.out_frame
                && self.in_frame >= -(MAX_FRAMES as i32)
                && self.out_frame <= MAX_FRAMES as i32,
            "invalid layer clip interval",
        )?;
        self.edit_frame(0)?;
        self.edit_frame(frames - 1)?;
        Ok(())
    }
    pub fn edit_frame(&self, frame: u32) -> Result<i32> {
        i32::try_from(i64::from(frame) - i64::from(self.offset_frame))
            .map_err(|_| crate::Error::Invalid("local keyframe time overflow".into()))
    }
    pub fn extends_composition(&self, frames: u32) -> bool {
        self.in_frame < 0 || i64::from(self.out_frame) > i64::from(frames)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParentLink {
    pub object: Option<u64>,
    pub bind: [[f32; 4]; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub version: u32,
    /// One project-wide working precision, shared by every composition.
    /// Required on disk; unsupported GPU modes are never silently reduced.
    pub working_precision: motion_effects::image_contract::ChannelPrecision,
    pub composition_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compositions: Vec<crate::Composition>,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub frames: u32,
    pub work_area: crate::WorkArea,
    pub background: [f32; 4],
    pub assets: Vec<Asset>,
    pub model_assets: Vec<crate::ModelAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<FontAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_assets: Vec<crate::AudioAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub video_assets: Vec<crate::VideoAsset>,
    pub camera: Camera,
    /// Index zero is the back of the same-depth stack.
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub plugin_dependencies: Vec<crate::PluginDependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<crate::PropertyExpression>,
}
impl Project {
    pub fn new(width: u32, height: u32, fps: u32, frames: u32) -> Result<Self> {
        ensure(width > 0 && height > 0, "composition size must be positive")?;
        let mut project = Self {
            version: CURRENT_PROJECT_VERSION,
            working_precision: motion_effects::image_contract::ChannelPrecision::Unorm8,
            composition_id: crate::MAIN_COMPOSITION.into(),
            compositions: Vec::new(),
            name: "空间练习 01".into(),
            width,
            height,
            fps,
            frames,
            work_area: crate::WorkArea::full(frames),
            background: [0.05, 0.06, 0.09, 1.0],
            assets: Vec::new(),
            model_assets: Vec::new(),
            fonts: Vec::new(),
            audio_assets: Vec::new(),
            video_assets: Vec::new(),
            camera: Camera::new(width, height),
            layers: Vec::new(),
            plugin_dependencies: Vec::new(),
            expressions: Vec::new(),
        };
        project.camera.created = false;
        project.validate()?;
        Ok(project)
    }
    pub fn demo() -> Result<Self> {
        let mut p = Self::new(1080, 1920, 30, 180)?;
        p.camera.created = true;
        p.layers = vec![
            Layer::solid(
                1,
                "背景",
                [1800.0, 2400.0],
                [540.0, 960.0, 500.0],
                [0.07, 0.18, 0.25, 1.0],
            ),
            Layer::solid(
                2,
                "主体卡片",
                [560.0, 760.0],
                [540.0, 960.0, 0.0],
                [0.22, 0.83, 0.75, 1.0],
            ),
            Layer::solid(
                3,
                "前景",
                [240.0, 400.0],
                [260.0, 1280.0, -500.0],
                [0.88, 0.73, 0.43, 0.85],
            ),
        ];
        // This is an explicitly spatial example, not the empty-project default.
        for layer in &mut p.layers {
            layer.three_d = true;
        }
        Ok(p)
    }
    pub fn validate(&self) -> Result<()> {
        ensure(self.version == CURRENT_PROJECT_VERSION, &format!("unsupported project format: version={} expected={CURRENT_PROJECT_VERSION}", self.version))?;
        ensure(self.fonts.len()<=128,"too many font assets")?;
        let mut font_ids=HashSet::new();
        for font in &self.fonts {
            ensure(motion_effects::valid_id(&font.id)&&font_ids.insert(&font.id)&&font.face_index<=255&&font.name.len()<=256&&font.license.len()<=65536,"invalid font asset metadata")?;
            crate::storage::validate_relative_path(&font.path)?;
            ensure(font.path.starts_with("assets/fonts/")&&font.path.ends_with(".ttf"),"invalid managed font path")?;
        }
        ensure(self.plugin_dependencies == self.composition_dependencies(),
               "plugin dependency list does not match effect instances")?;
        self.validate_one(self)?;
        self.validate_compositions()
    }
    pub(crate) fn validate_one(&self, document: &Project) -> Result<()> {
        crate::effects::validate_image_graph(self)?;
        ensure(
            (1..=8192).contains(&self.width) && (1..=8192).contains(&self.height),
            "invalid composition size",
        )?;
        ensure(
            (1..=MAX_COMPOSITION_FPS).contains(&self.fps),
            "composition frame rate must be 1..240",
        )?;
        ensure(
            (1..=MAX_FRAMES).contains(&self.frames),
            "invalid composition duration",
        )?;
        self.work_area.validate(self.frames)?;
        ensure(self.name.len() <= 1024, "project name too long")?;
        validate_color(self.background)?;
        ensure(self.layers.len() <= MAX_LAYERS, "layer limit exceeded")?;
        ensure(self.assets.len() <= MAX_LAYERS * 2, "asset limit exceeded")?;
        self.camera.validate(self.frames)?;
        ensure(
            self.video_assets.len() <= MAX_LAYERS * 2,
            "video asset limit exceeded",
        )?;
        let mut asset_ids = HashSet::new();
        for asset in &self.assets {
            ensure(
                asset.id != 0 && asset_ids.insert(asset.id),
                "asset IDs must be unique and nonzero",
            )?;
            ensure(
                (1..=16384).contains(&asset.width) && (1..=16384).contains(&asset.height),
                "invalid source image size",
            )?;
            crate::storage::validate_relative_path(&asset.path)?;
        }
        ensure(
            self.audio_assets.len() + self.video_assets.len() + self.assets.len() + self.model_assets.len() <= MAX_LAYERS * 2,
            "asset limit exceeded",
        )?;
        for asset in &self.model_assets {
            ensure(asset_ids.insert(asset.id), "asset IDs must be unique and nonzero")?;
            asset.validate()?;
        }
        for asset in &self.audio_assets {
            ensure(
                asset.id != 0 && asset_ids.insert(asset.id),
                "asset IDs must be unique and nonzero",
            )?;
            asset.validate()?;
        }
        for asset in &self.video_assets {
            ensure(
                asset.id != 0 && asset_ids.insert(asset.id),
                "asset IDs must be unique and nonzero",
            )?;
            asset.validate()?;
            if let Some(id) = asset.audio_asset {
                let audio = self
                    .audio_assets
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or_else(|| crate::Error::Invalid("video audio asset missing".into()))?;
                ensure(
                    audio.path == asset.path
                        && audio.bytes == asset.bytes
                        && audio.duration_us <= asset.duration_us,
                    "video/audio source timeline mismatch",
                )?;
            }
        }
        let mut audio_paths = HashSet::new();
        for a in &self.audio_assets {
            ensure(
                audio_paths.insert(a.path.to_lowercase()),
                "audio assets cannot share different track caches at one path",
            )?;
            if let Some(v) = self
                .video_assets
                .iter()
                .find(|v| v.path.eq_ignore_ascii_case(&a.path))
            {
                ensure(
                    v.path == a.path && v.audio_asset == Some(a.id),
                    "shared MP4 must reference its matching audio asset",
                )?;
            }
        }
        let mut video_paths = HashSet::new();
        for a in &self.video_assets {
            ensure(
                video_paths.insert(a.path.to_lowercase()),
                "duplicate video source path",
            )?;
        }
        let mut ids = HashSet::new();
        crate::compositing::validate(&self.layers)?;
        for layer in &self.layers {
            layer.sampling.validate_content(&layer.content)?;
            crate::audio_edit::validate(layer)?;
            if let Some(levels)=&layer.audio_levels {
                ensure(matches!(layer.content,Content::Audio{..}|Content::Video{..}|Content::Composition{..}),
                       "audio levels require an audio, video or composition layer")?;
                levels.validate()?;
            }
            crate::masks::validate(&layer.masks).map_err(|e| crate::Error::Invalid(format!("layer {}: {e}",layer.id)))?;
            ensure(layer.masks.is_empty() || !matches!(layer.content, Content::Null | Content::Camera { .. } | Content::Audio {..} | Content::Adjustment),
                "source masks require an ordinary visual layer; adjustment masks require composition compositing")?;
            ensure(
                layer.id != 0 && ids.insert(layer.id),
                "layer IDs must be unique and nonzero",
            )?;
            ensure(layer.name.len() <= 1024, "layer name too long")?;
            ensure(
                matches!(layer.content, Content::Audio { .. }) && layer.size == [0.0; 2]
                    || layer
                        .size
                        .into_iter()
                        .all(|v| v.is_finite() && v > 0.0 && v <= 32768.0),
                "invalid layer size",
            )?;
            layer.clip(self.frames).validate(self.frames)?;
            if let Some(range)=layer.subframe_range {
                range.validate()?;
                ensure(range.covers(layer.clip(self.frames)),"integer clip bounds do not cover precise interval")?;
            }
            layer.transform.validate_local()?;
            ensure(
                layer.effects.len() <= motion_effects::MAX_EFFECTS_PER_LAYER,
                "too many layer effects",
            )?;
            ensure(
                !matches!(layer.content, Content::Null | Content::Camera { .. }) || layer.effects.is_empty(),
                "null and camera layers cannot contain effects",
            )?;
            let mut effect_ids = HashSet::new();
            for e in &layer.effects {
                ensure(effect_ids.insert(e.id), "duplicate effect instance ID")?;
                e.validate(self.frames)?;
                if let Some(input) = e.image_input {
                    match input {
                        crate::EffectImageInput::Layer { layer: source, .. } => {
                            let source = self.layers.iter().find(|l| l.id == source)
                                .ok_or_else(|| crate::Error::Invalid("effect image source must belong to the same composition".into()))?;
                            ensure(!matches!(source.content, Content::Null | Content::Camera { .. } | Content::Audio { .. } | Content::Adjustment),
                                "effect image source has no independent pixels")?;
                        }
                        crate::EffectImageInput::Asset { asset } => ensure(document.assets.iter().any(|a| a.id == asset), "effect image asset missing")?,
                        crate::EffectImageInput::Empty => {}
                    }
                }
            }
            crate::layer_switches::validate(layer)?;
            match &layer.content {
                Content::Adjustment => {
                    ensure(!layer.three_d, "adjustment layers must be 2D")?;
                    ensure(
                        layer.effects.iter().all(|e| e.scene.is_none()),
                        "adjustment layers cannot contain scene generators",
                    )?;
                }
                Content::Vector { vector } => {
                    vector.validate().map_err(|e| {
                        crate::Error::Invalid(format!("layer {} vector: {e}", layer.id))
                    })?;
                }
                Content::Composition { clip } => {
                    document.composition_view(&clip.composition)?;
                    ensure(clip.volume.is_finite() && (0.0..=4.0).contains(&clip.volume)
                        && clip.source_start_frame.unsigned_abs() <= MAX_FRAMES, "invalid composition source interval or volume")?;
                }
                Content::Video { video } => {
                    video.validate()?;
                    let asset = self
                        .video_assets
                        .iter()
                        .find(|a| a.id == video.asset)
                        .ok_or_else(|| crate::Error::Invalid("video asset missing".into()))?;
                    let clip = layer.clip(self.frames);
                    let start = i128::from(video.source_offset_us) * i128::from(self.fps)
                        + (i128::from(clip.in_frame) - i128::from(clip.offset_frame)) * 1_000_000;
                    let last = i128::from(video.source_offset_us) * i128::from(self.fps)
                        + (i128::from(clip.out_frame) - 1 - i128::from(clip.offset_frame))
                            * 1_000_000;
                    if let Some(range)=layer.subframe_range {
                        let origin=f64::from(clip.offset_frame);
                        let first=video.source_offset_us as f64/1_000_000.+(range.in_frame-origin)/f64::from(self.fps);
                        let end=video.source_offset_us as f64/1_000_000.+(range.out_frame-origin)/f64::from(self.fps);
                        ensure(first>=0.&&end<=asset.duration_us as f64/1_000_000.+1e-9,"video clip exceeds recoverable source interval")?;
                    }else{
                        ensure(start >= 0 && last < i128::from(asset.duration_us) * i128::from(self.fps),"video clip exceeds recoverable source interval")?;
                    }
                }
                Content::Audio { audio } => {
                    audio.validate()?;
                    let asset = self
                        .audio_assets
                        .iter()
                        .find(|a| a.id == audio.asset)
                        .ok_or_else(|| {
                            crate::Error::Invalid("audio references a missing audio asset".into())
                        })?;
                    ensure(
                        !layer.three_d
                            && layer.parent.is_none()
                            && layer.size == [0.0; 2]
                            && layer.transform == Transform::new([0.0; 3]),
                        "audio has no spatial properties or parent",
                    )?;
                    let clip = layer.clip(self.frames);
                    let start = i128::from(audio.source_offset_us) * i128::from(self.fps)
                        + (i128::from(clip.in_frame) - i128::from(clip.offset_frame)) * 1_000_000;
                    let last = i128::from(audio.source_offset_us) * i128::from(self.fps)
                        + (i128::from(clip.out_frame) - 1 - i128::from(clip.offset_frame))
                            * 1_000_000;
                    if let Some(range)=layer.subframe_range {
                        let origin=f64::from(clip.offset_frame);
                        let first=audio.source_offset_us as f64/1_000_000.+(range.in_frame-origin)/f64::from(self.fps);
                        let end=audio.source_offset_us as f64/1_000_000.+(range.out_frame-origin)/f64::from(self.fps);
                        let duration=asset.sample_frames as f64/f64::from(asset.sample_rate);
                        ensure(first>=0.&&end<=duration+1e-9,"audio clip exceeds recoverable source interval")?;
                    }else{
                        ensure(start >= 0 && last < i128::from(asset.duration_us) * i128::from(self.fps),"audio clip exceeds recoverable source interval")?;
                    }
                }
                Content::Camera { camera } => {
                    ensure(camera.created && camera.parent.is_none(), "camera layers use their layer parent")?;
                    ensure(layer.three_d && layer.transform == Transform::new([0.; 3]),
                        "camera spatial properties belong to the camera tracks")?;
                    camera.validate_local()?;
                }
                Content::Model { model } => {
                    model.validate()?;
                    ensure(layer.three_d, "model layers require 3D transforms")?;
                    ensure(self.model_assets.iter().any(|a| a.id == model.asset), "model asset missing")?;
                }
                Content::Null => {}
                Content::Solid { color } => validate_color(*color)?,
                Content::Image { asset } => ensure(
                    self.assets.iter().any(|a| a.id == *asset),
                    "image references a missing asset",
                )?,
                Content::Text {
                    text,
                    font,
                    style,
                    color,
                    raster_asset,
                } => {
                    ensure(
                        text.len() <= 65536 && font.len() <= 256,
                        "text or font identifier too long",
                    )?;
                    validate_color(*color)?;
                    style.validate()?;
                    ensure(
                        self.assets.iter().any(|a| a.id == *raster_asset),
                        "text raster asset is missing",
                    )?;
                }
            }
        }
        crate::hierarchy::validate(self)?;
        crate::expressions::validate(self)?;
        Ok(())
    }
    pub fn frame_pts_us(&self, frame: u32) -> Result<i64> {
        ensure(frame < self.frames, "output frame outside composition")?;
        Ok((u64::from(frame) * 1_000_000 + u64::from(self.fps) / 2) as i64 / i64::from(self.fps))
    }
    pub fn layer_audio(&self, layer: &Layer) -> Option<crate::AudioClip> {
        match &layer.content {
            Content::Audio { audio } => Some(audio.clone()),
            Content::Video { video } => self
                .video_assets
                .iter()
                .find(|a| a.id == video.asset)
                .and_then(|a| a.audio_asset)
                .map(|asset| crate::AudioClip {
                    asset,
                    source_offset_us: video.source_offset_us,
                    volume: video.volume,
                    muted: video.muted,
                }),
            _ => None,
        }
    }
    pub fn edit_frame(&self, object: u64, frame: u32) -> Result<i32> {
        ensure(frame < self.frames, "edit frame outside the composition")?;
        if object == 0 {
            return Ok(frame as i32);
        }
        self.layers
            .iter()
            .find(|l| l.id == object)
            .ok_or(crate::Error::Missing(object))?
            .clip(self.frames)
            .edit_frame(frame)
    }
    pub fn layer_mut(&mut self, id: u64) -> Result<&mut Layer> {
        self.layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or(crate::Error::Missing(id))
    }
    pub fn estimated_bytes(&self) -> usize {
        // Exact compact JSON size; excludes caches and heap capacity. The editor
        // reuses component counts so ordinary edits do not revisit this document.
        serialized_bytes(self)
    }
    pub fn rebuild_plugin_dependencies(&mut self) {
        self.plugin_dependencies = self.composition_dependencies();
    }
    fn composition_dependencies(&self) -> Vec<crate::PluginDependency> {
        crate::effects::dependencies(self.layers.iter().chain(self.compositions.iter().flat_map(|c|c.layers.iter())))
    }
}
fn validate_color(color: [f32; 4]) -> Result<()> {
    ensure(
        color
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v)),
        "invalid RGBA color",
    )
}

/// Exact compact JSON length without allocating a serialized document. Large
/// callers should cache immutable component sizes rather than recounting trees.
pub fn serialized_bytes<T: Serialize + ?Sized>(value: &T) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("serialized size overflow"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => counter.0,
        Err(error) => {
            static FAILURES: crate::observability::Failures = crate::observability::Failures::new();
            FAILURES.record("serialized_bytes", || {
                format!(
                    "fallback=usize::MAX written_bytes={} value_type={} reason={}",
                    counter.0,
                    std::any::type_name::<T>().chars().take(128).collect::<String>(),
                    error.to_string().chars().take(256).collect::<String>(),
                )
            });
            usize::MAX
        }
    }
}
