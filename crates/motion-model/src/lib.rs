//! Stable project data, validation and deterministic sampling; no editing runtime or GPU APIs.
mod animation;
mod audio;
pub mod audio_levels;
pub mod audio_edit;
pub mod media_assets;
pub use audio_edit::{AudioMarker, AudioMarkersAction, AudioSourceChannel};
pub mod boundaries;
mod camera;
pub use camera::lens;
pub mod composition;
mod curve;
pub mod color_curves;
pub mod depth;
mod commands;
pub mod layer_clipboard;
pub mod keyframe_clipboard;
pub mod path_clipboard;
pub mod effects;
pub mod expressions;
pub mod path_conversion;
pub mod geometry;
pub mod hierarchy;
mod model;
pub mod layer_switches;
pub use layer_switches::LayerSwitch;
mod observability;
mod clip_range;
pub mod particle_history;
pub mod masks;
pub mod compositing;
pub mod render_sampling;
pub mod storage;
mod timeline;
pub mod text;
pub use text::{TextAlign, TextStyle};
mod video;
mod work_area;
pub use work_area::{FrameScope, WorkArea};
pub mod vector;

pub use boundaries::{EditSink, FrameEvaluator, PackageExportSource, PackagePublication};
pub use commands::{Command, EditResult, Property};
pub use layer_clipboard::LayerClipboard;
pub use keyframe_clipboard::{KeyframeClipboard, KeyframeData, KeyframeSelection, KeyframeTarget};
pub use keyframe_clipboard::KeyframeValueOffset;
pub use path_clipboard::{PathClipboard, PathDestination, PathEntry, PathSelection, PathTarget};
pub use animation::{Axis, AxisTracks, Ease, Keyframe, SpatialTangents, Track, Tween};
pub use audio::{AudioAsset, AudioClip};
pub use audio_levels::{AudioLevels, AudioLevelsAction, AudioChannel};
pub use camera::{
    to_project, to_world, Camera, CameraMode, CameraPose, ObservationView, Observer, ProjectionKind,
};
pub use composition::{Composition, CompositionAction, CompositionClip, CompositionSettings, MAIN_COMPOSITION};
pub use curve::{Curve, CurveSample, CurveShape, CurveSpace, Easing};
pub use effects::{
    CurveLut, CurveObject, CurveTrack, SampledColorCurve, HIGH_PRECISION_CURVE_SAMPLES,
    EffectAction, EffectImageInput, EffectImageStage, EffectInstance, EffectParam, PluginDependency,
    SampledEffect,
};
pub use expressions::{
    ExpressionTarget, ExpressionValue, PropertyExpression, EXPRESSION_PROFILE, MAX_EXPRESSIONS,
};
pub use model::{
    serialized_bytes,
    FontAsset,
    Asset, Content, Layer, LayerTimeline, ParentLink, Project, Transform, MAX_COMPOSITION_FPS,
    MAX_FRAMES, MAX_LAYERS, CURRENT_PROJECT_VERSION,
};
pub use clip_range::ClipRange;
pub use geometry::RotationOrder;
pub use timeline::{TimelineKey, TimelineLayer, TimelineProperties, TimelineTrack};
pub use video::{
    VideoAsset, VideoClip, VideoSample, MAX_VIDEO_DIMENSION, MAX_VIDEO_FPS, MAX_VIDEO_FRAMES,
    MAX_VIDEO_PIXELS,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Shared(#[source] SharedCause),
    #[error(transparent)]
    Composition(#[from] composition::CompositionError),
    #[error("expression {target} at frame {frame}: {source}")]
    Expression {
        target: String,
        frame: f64,
        #[source]
        source: Box<Error>,
    },
    #[error("expression sampling {target} at frame {frame}: {source}")]
    ExpressionSampling {
        target: String,
        frame: f64,
        #[source]
        source: Box<Error>,
    },
    #[error("color curve sampling at frame {frame}: {source}")]
    ColorCurveSampling {
        frame: f64,
        #[source]
        source: Box<Error>,
    },
    #[error("{0}")]
    Invalid(String),
    #[error("timeline object={object} cause={source}")]
    Timeline {
        object: u64,
        #[source]
        source: Box<Error>,
    },
    #[error("audio timeline composition={composition} object={object} phase={phase} cause={source}")]
    AudioTimeline {
        composition: String,
        object: u64,
        phase: &'static str,
        #[source]
        source: Box<Error>,
    },
    #[error("object {0} does not exist")]
    Missing(u64),
    #[error("object {0} is locked")]
    Locked(u64),
    #[error("keyframe clipboard phase={phase} object={object} channel={channel}: {source}")]
    KeyframeClipboard {
        phase: &'static str,
        object: u64,
        channel: String,
        #[source]
        source: Box<Error>,
    },
    #[error("keyframe edit operation={operation} object={object} channel={channel}: {source}")]
    KeyframeEdit {
        operation: &'static str,
        object: u64,
        channel: String,
        #[source]
        source: Box<Error>,
    },
    #[error("path clipboard phase={phase} object={object} path={path}: {source}")]
    PathClipboard {
        phase: &'static str,
        object: u64,
        path: u64,
        #[source]
        source: Box<Error>,
    },
    #[error("path edit operation={operation} object={object} path={path} node={node:?}: {source}")]
    PathOperation {
        operation: &'static str,
        object: u64,
        path: u64,
        node: Option<u64>,
        #[source]
        source: Box<Error>,
    },
    #[error("parent edit object={object} parent={parent:?} frame={frame} mode={mode:?}: {source}")]
    ParentOperation {
        object: u64,
        parent: Option<u64>,
        frame: u32,
        mode: hierarchy::ParentMode,
        #[source]
        source: Box<Error>,
    },
    #[error("edit gesture already active or history unavailable during a gesture")]
    Gesture,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("project save phase={phase} path={} published={published} cause={source}", .path.display())]
    Storage {
        phase: &'static str,
        path: std::path::PathBuf,
        published: bool,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}
pub type Result<T> = std::result::Result<T, Error>;

/// A cached failure keeps the original error reachable through `source()`.
/// Using an Arc directly as a source would skip a leaf error when traversing
/// its sources, because Arc delegates `source()` to the contained error.
#[derive(Clone, Debug)]
pub struct SharedCause(std::sync::Arc<Error>);

impl std::fmt::Display for SharedCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.0.as_ref(), f)
    }
}

impl std::error::Error for SharedCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

impl From<std::sync::Arc<Error>> for Error {
    fn from(source: std::sync::Arc<Error>) -> Self {
        Self::Shared(SharedCause(source))
    }
}

pub fn ensure(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Invalid(message.to_owned()))
    }
}
