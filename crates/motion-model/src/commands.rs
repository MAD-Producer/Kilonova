use crate::{Asset, CameraMode, Content, Ease, Easing, Layer};
use serde::{Deserialize, Serialize};

fn deserialize_camera_binding<'de, D: serde::Deserializer<'de>>(deserializer: D)
    -> std::result::Result<Option<crate::ImportedCamera>, D::Error> {
    Option::<crate::ImportedCamera>::deserialize(deserializer)
}
fn deserialize_depth_calibration<'de, D: serde::Deserializer<'de>>(deserializer: D)
    -> std::result::Result<Option<crate::depth::SavedDepthCalibration>, D::Error> {
    Option::<crate::depth::SavedDepthCalibration>::deserialize(deserializer)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Anchor,
    Position,
    Rotation,
    Scale,
    Opacity,
    Target,
    Roll,
    Fov,
    Radius,
    Azimuth,
    Elevation,
}

impl Property {
    pub fn is_scalar(self) -> bool {
        matches!(self, Self::Opacity | Self::Roll | Self::Fov | Self::Radius | Self::Azimuth | Self::Elevation)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    ApplyAnimationPreset { preset: Box<crate::AnimationPreset>, objects: Vec<u64>, at_frame: u32 },
    SetWorkingPrecision { precision: motion_effects::image_contract::ChannelPrecision },
    DepthCalibration {
        object: u64,
        #[serde(deserialize_with = "deserialize_depth_calibration")]
        calibration: Option<crate::depth::SavedDepthCalibration>,
    },
    VideoDepthCalibration { object: u64, sample: crate::depth::VideoDepthSample },
    /// Zero edits the active composition background. Visual layer source
    /// colors remain static; animated color uses a color effect or vector paint.
    SetColor { object:u64, value:[f32;4] },
    AddAdjustment {
        id: u64,
        name: String,
    },
    AddShape {
        id: u64,
        name: String,
        shape: crate::vector::ShapeKind,
        size: [f32; 2],
        position: [f32; 3],
    },
    Vector {
        object: u64,
        action: crate::vector::VectorAction,
    },
    Mask { object: u64, action: crate::masks::MaskAction },
    SetLayerBlend {object:u64,#[serde(default)]mode:Option<crate::compositing::BlendMode>,#[serde(default)]space:Option<crate::compositing::BlendSpace>},
    SetTrackMatte {object:u64,matte:Option<crate::compositing::TrackMatte>},
    InComposition { composition: String, command: Box<Command> },
    Composition { action: crate::CompositionAction },
    SetWorkArea { in_frame: u32, out_frame: u32 },
    SetGuides {
        #[serde(deserialize_with = "crate::guides::deserialize")]
        guides: Vec<crate::Guide>,
    },
    RegisterAudioAsset {
        asset: crate::AudioAsset,
    },
    RegisterVideoAsset {
        asset: crate::VideoAsset,
    },
    MediaAsset { action: crate::media_assets::AssetAction },
    ProjectLibrary { action: crate::LibraryAction },
    SetAudio {
        object: u64,
        #[serde(default)]
        volume: Option<f32>,
        #[serde(default)]
        muted: Option<bool>,
    },
    AudioLevels { object:u64, action:crate::AudioLevelsAction },
    AudioMarkers { object: u64, action: crate::AudioMarkersAction },
    SplitAudioChannels { object: u64 },
    ExtractAudio { object: u64 },
    #[serde(rename = "set_layer_3d")]
    SetLayer3d {
        object: u64,
        enabled: bool,
    },
    SetLayerSampling {
        object: u64,
        sampling: crate::render_sampling::LayerSampling,
    },
    SetExpression {
        expression: crate::PropertyExpression,
        frame: u32,
    },
    RemoveExpression {
        target: crate::ExpressionTarget,
    },
    Effect {
        object: u64,
        action: crate::EffectAction,
    },
    SeparateDimensions {
        object: u64,
        property: Property,
    },
    MergeDimensions {
        object: u64,
        property: Property,
    },
    SetComponent {
        object: u64,
        property: Property,
        axis: crate::Axis,
        frame: u32,
        value: f32,
    },
    MoveLayerClip {
        object: u64,
        in_frame: i32,
    },
    TrimLayerClip {
        object: u64,
        in_frame: i32,
        out_frame: i32,
    },
    SetLayerRange {object:u64,in_frame:f64,out_frame:f64},
    SetRotationOrder {object:u64,order:crate::geometry::RotationOrder},
    SplitLayerClip {
        object: u64,
        frame: u32,
    },
    Curve {
        object: u64,
        property: Property,
        #[serde(default)]
        axis: Option<crate::Axis>,
        frame: u32,
        easing: Easing,
    },
    Spatial {
        object: u64,
        property: Property,
        frame: u32,
        tangents: Option<crate::SpatialTangents<[f32; 3]>>,
    },
    CreateCamera,
    Remove {
        object: u64,
        frame: u32,
    },
    Parent {
        object: u64,
        parent: Option<u64>,
        frame: u32,
    },
    ParentWithMode {
        object: u64,
        parent: Option<u64>,
        frame: u32,
        mode: crate::hierarchy::ParentMode,
    },
    CopyKey {
        object: u64,
        property: Property,
        #[serde(default)]
        axis: Option<crate::Axis>,
        from: u32,
        to: u32,
    },
    RegisterAsset {
        asset: Asset,
    },
    RegisterModelAsset {
        asset: crate::ModelAsset,
    },
    RegisterFontAsset {
        asset: crate::FontAsset,
    },
    Content {
        object: u64,
        content: Content,
        size: [f32; 2],
    },
    SetVector {
        object: u64,
        property: Property,
        frame: u32,
        value: [f32; 3],
    },
    SetScalar {
        object: u64,
        property: Property,
        frame: u32,
        value: f32,
    },
    Animate {
        object: u64,
        property: Property,
        #[serde(default)]
        axis: Option<crate::Axis>,
        frame: u32,
        enabled: bool,
    },
    MoveKey {
        object: u64,
        property: Property,
        #[serde(default)]
        axis: Option<crate::Axis>,
        from: u32,
        to: u32,
    },
    DeleteKey {
        object: u64,
        property: Property,
        #[serde(default)]
        axis: Option<crate::Axis>,
        frame: u32,
    },
    Ease {
        object: u64,
        property: Property,
        #[serde(default)]
        axis: Option<crate::Axis>,
        frame: u32,
        ease: Ease,
    },
    Add {
        layer: Layer,
    },
    PasteLayers {
        clipboard: Box<crate::LayerClipboard>,
        #[serde(default)]
        at_frame: Option<u32>,
        keep_external_references: bool,
    },
    RemoveLayers { objects: Vec<u64>, frame: u32 },
    PasteKeyframes { clipboard: Box<crate::KeyframeClipboard>, targets: Vec<crate::KeyframeTarget>, at_frame: Option<u32> },
    RemoveKeyframes { clipboard: Box<crate::KeyframeClipboard> },
    KeyframeInterpolation { selections: Vec<crate::KeyframeSelection>, ease: Ease },
    KeyframeAutomatic { selections: Vec<crate::KeyframeSelection>, enabled: bool },
    /// Continuous spatial edits address a property and persistent key IDs.
    SpatialKeyframes { target:crate::KeyframeTarget, edit:crate::keyframe_clipboard::spatial::SpatialKeyEdit },
    /// `frame` addresses the interval's first key in composition time. Incoming
    /// selects the second key's incoming handle; otherwise edit the first key's
    /// outgoing handle. Only that control point and its owner's type change.
    KeyframeHandle { target: crate::KeyframeTarget, frame: u32, incoming: bool, control: [f64; 2] },
    PasteEasing { clipboard: Box<crate::CurveClipboard>, selections: Vec<crate::KeyframeSelection> },
    KeyframeTiming { selections: Vec<crate::KeyframeSelection>, settings: crate::keyframe_clipboard::timing::TimingSettings },
    EaseKeyframes { selections: Vec<crate::KeyframeSelection>, incoming: bool, outgoing: bool },
    DeleteKeyframes { selections: Vec<crate::KeyframeSelection> },
    TransformKeyframes { selections: Vec<crate::KeyframeSelection>, delta_frame: i32, value_offset: Option<crate::KeyframeValueOffset>, copy: bool },
    PastePaths { clipboard: Box<crate::PathClipboard>, target: crate::PathDestination, at_frame: Option<u32> },
    RemovePaths { clipboard: Box<crate::PathClipboard> },
    CopyPathAs { target: crate::PathTarget, path: u64 },
    Delete {
        object: u64,
    },
    Duplicate {
        object: u64,
    },
    Rename {
        object: u64,
        name: String,
    },
    Reorder {
        object: u64,
        index: usize,
    },
    Flags {
        object: u64,
        visible: bool,
        locked: bool,
    },
    SetLayerShy { object: u64, shy: bool },
    SetLayerSwitch { object: u64, switch: crate::LayerSwitch, enabled: bool },
    Anchor {
        object: u64,
        anchor: [f32; 2],
    },
    /// Move the sampled anchor with position compensation at composition time.
    AnchorAt { object: u64, frame: u32, anchor: [f32; 2] },
    CameraLens {
        #[serde(default)]
        object: u64,
        frame: u32,
        film_width_mm: f32,
        focal_length_mm: f32,
    },
    CameraImported {
        #[serde(default)]
        object: u64,
        #[serde(deserialize_with = "deserialize_camera_binding")]
        binding: Option<crate::ImportedCamera>,
    },
    CameraMode {
        #[serde(default)]
        object: u64,
        mode: CameraMode,
    },
    Dolly {
        #[serde(default)]
        object: u64,
        frame: u32,
        amount: f32,
    },
    Pan {
        #[serde(default)]
        object: u64,
        frame: u32,
        x: f32,
        y: f32,
    },
}

impl Command {
    pub fn changes_resources(&self) -> bool {
        match self {
            Self::InComposition { command, .. } => command.changes_resources(),
            Self::RegisterAsset { .. }
            | Self::RegisterModelAsset { .. }
            | Self::RegisterAudioAsset { .. }
            | Self::RegisterVideoAsset { .. }
            | Self::MediaAsset { .. }
            | Self::RegisterFontAsset { .. }
            | Self::Content { .. }
            | Self::Add { .. }
            | Self::PasteLayers { .. }
            | Self::ApplyAnimationPreset { .. }
            | Self::Composition { .. } => true,
            _ => false,
        }
    }
    pub fn composition_transaction(&self) -> bool {
        if matches!(self, Self::ApplyAnimationPreset { .. } | Self::ProjectLibrary { .. }) { return true; }
        match self { Self::InComposition {command,..} => command.composition_transaction(), Self::MediaAsset {..} | Self::Composition {..} | Self::PasteLayers {..} | Self::RemoveLayers {..} | Self::PasteKeyframes {..} | Self::RemoveKeyframes {..} | Self::KeyframeInterpolation {..} | Self::KeyframeAutomatic {..} | Self::SpatialKeyframes {..} | Self::PasteEasing {..} | Self::KeyframeTiming {..} | Self::EaseKeyframes {..} | Self::DeleteKeyframes {..} | Self::TransformKeyframes {..} | Self::PastePaths {..} | Self::RemovePaths {..} | Self::CopyPathAs {..} => true, _ => false }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditResult {
    PasteLayers { objects: Vec<u64> },
    CreateCamera { object: u64 },
    SplitLayerClip { left_object: u64, right_object: u64 },
    Composition { result: serde_json::Value },
}
