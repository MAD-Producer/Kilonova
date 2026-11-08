//! Actual authored-color sampling into an explicit working-space frame. The
//! ordinary Scene API remains separate until every asset/effect route is wired.
use super::*;
use crate::ColorImageContract;
use motion_effects::{
    color_config::{ColorConfigError, ReferenceSpace, VerifiedColorConfig},
    color_processor::{CodeValueRange, ColorProcessorPhase},
    image_contract::ChannelPrecision,
    native_color::{self, NativeColorProgram},
    AlphaMode,
};
use motion_model::color_management::{ProjectColorManagement, SignalRange};
use std::sync::Arc;
mod vectors;

#[derive(Debug, thiserror::Error)]
pub enum AuthoredSceneError {
    #[error("authored color phase={phase} object={object:?}: {reason}")]
    Admission {
        phase: &'static str,
        object: Option<u64>,
        reason: &'static str,
    },
    #[error("authored color phase={phase}: model failed: {source}")]
    Model {
        phase: &'static str,
        #[source]
        source: motion_model::Error,
    },
    #[error("authored color phase=binding: {0}")]
    Config(#[from] ColorConfigError),
    #[error("authored color phase=range black={black} white={white}")]
    Range {
        black: f64,
        white: f64,
        #[source]
        source: motion_effects::color_processor::ColorProcessorError,
    },
    #[error("authored color phase=destination expected={expected:?} actual={actual:?}")]
    Destination {
        expected: ColorImageContract,
        actual: ColorImageContract,
    },
    #[error("authored color phase=source_binding asset={asset:?} expected={expected:?} actual={actual:?}")]
    SourceContract {
        asset: Option<u64>,
        expected: ColorImageContract,
        actual: ColorImageContract,
    },
    #[error("authored color phase=destination expected_size={expected:?} actual_size={actual:?}")]
    Extent {
        expected: [u32; 2],
        actual: [u32; 2],
    },
    #[error("authored color phase=transform object={object:?}: native processor failed: {source}")]
    Native {
        object: Option<u64>,
        #[source]
        source: native_color::Error,
    },
}
type Result<T> = std::result::Result<T, AuthoredSceneError>;
fn refuse(phase: &'static str, object: Option<u64>, reason: &'static str) -> AuthoredSceneError {
    AuthoredSceneError::Admission {
        phase,
        object,
        reason,
    }
}

/// The complete output contract is immutable. Inspection contexts retain a
/// private identity marker, so legacy renderers refuse their working RGB.
pub struct AuthoredSceneFrame {
    pub(crate) scene: Scene,
    contract: ColorImageContract,
    assets: Arc<[motion_model::Asset]>,
}
impl AuthoredSceneFrame {
    /// Read geometry and sampled state without removing the color contract.
    /// Cloning this context does not authorize a legacy sRGB renderer.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn contract(&self) -> ColorImageContract {
        self.contract
    }
    pub fn size(&self) -> [u32; 2] {
        [self.scene.width, self.scene.height]
    }
    pub fn frame(&self) -> f64 {
        self.scene.frame
    }
    pub(crate) fn source(&self, id: u64) -> Option<&motion_model::Asset> {
        self.assets.iter().find(|asset| asset.id == id)
    }
}

/// A native CPU program has one exclusive owner and may move with it. Two reusable scenes
/// retain the last published frame while a new sample is being prepared. No
/// native runtime load, program preparation or file IO occurs during sampling.
pub struct AuthoredSceneSampler {
    vectors: vectors::WorkingVectors,
    settings: Arc<ProjectColorManagement>,
    native: NativeColorProgram,
    input_range: CodeValueRange,
    input_alpha: AlphaMode,
    current: AuthoredSceneFrame,
    spare: Scene,
    published: bool,
}
impl AuthoredSceneSampler {
    /// Called inside the host's observed preparation, using its exact retained
    /// program. No project clone with disabled color management is created.
    pub fn from_program(
        project: &Project,
        config: &VerifiedColorConfig,
        native: NativeColorProgram,
    ) -> Result<Self> {
        project
            .validate()
            .map_err(|source| AuthoredSceneError::Model {
                phase: "prepare",
                source,
            })?;
        admit(project)?;
        let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
            return Err(refuse(
                "prepare",
                None,
                "an explicit OCIO project is required",
            ));
        };
        if settings.working.encoding != "scene-linear"
            || settings.working.reference != ReferenceSpace::Scene
            || settings.working.range != (SignalRange::Full {})
            || settings.working.alpha != AlphaMode::Premultiplied
        {
            return Err(refuse("working", None, "authored composition requires declared scene-linear, full-range premultiplied working storage"));
        }
        let mut selection = settings.selection.clone();
        selection.input = settings.authored_input.space.clone();
        let resolved = config.resolve(&selection)?;
        let source = native.source();
        if config.manifest() != &settings.manifest
            || source.config_key != config.key()
            || source.selection_key != resolved.key
            || source.phase != ColorProcessorPhase::InputToWorking
            || source.input_alpha != settings.authored_input.signal.alpha
            || source.output_alpha != settings.working.alpha
        {
            return Err(refuse(
                "binding",
                None,
                "native program differs from the project's authored input/working contract",
            ));
        }
        let input_range = match settings.authored_input.signal.range {
            SignalRange::Full {} => CodeValueRange::FULL,
            SignalRange::Limited { black, white } => {
                CodeValueRange::new(black, white).map_err(|source| AuthoredSceneError::Range {
                    black,
                    white,
                    source,
                })?
            }
        };
        let camera = project
            .camera
            .pose(0., project.width, project.height)
            .map_err(|source| AuthoredSceneError::Model {
                phase: "camera",
                source,
            })?;
        let empty = || {
            Scene::empty(
                camera,
                project.background,
                project.width,
                project.height,
                project.fps,
                project.composition_id.clone(),
            )
        };
        let contract = ColorImageContract {
            space: config.space_identity(&settings.selection.working)?,
            range: CodeValueRange::FULL,
            alpha: AlphaMode::Premultiplied,
            precision: ChannelPrecision::Float32,
        };
        let mut current = empty();
        current.color_contract = Some(contract);
        let mut spare = empty();
        spare.color_contract = Some(contract);
        Ok(Self {
            vectors: Default::default(),
            settings: project.color_management.clone(),
            native,
            input_range,
            input_alpha: settings.authored_input.signal.alpha,
            current: AuthoredSceneFrame {
                scene: current,
                contract,
                assets: project.assets.clone().into(),
            },
            spare,
            published: false,
        })
    }

    /// Sample a validated project snapshot. Unsupported routes are refused,
    /// including changes made by a custom evaluator, instead of falling through
    /// to legacy image/effect conversions. On failure no new frame is published.
    pub fn sample(
        &mut self,
        project: &Project,
        frame: f64,
        evaluator: &dyn motion_model::FrameEvaluator,
    ) -> Result<&AuthoredSceneFrame> {
        self.validate_project(project)?;
        let cursor = SourceTimeSample {
            frame,
            rate: 1.,
            traversal: TimeTraversal::Later,
        };
        motion_model::ensure(
            frame.is_finite() && cursor.in_interval(0., f64::from(project.frames)),
            "invalid sample time",
        )
        .map_err(|source| AuthoredSceneError::Model {
            phase: "sample",
            source,
        })?;
        let evaluated =
            evaluator
                .evaluate(project, frame)
                .map_err(|source| AuthoredSceneError::Model {
                    phase: "evaluate",
                    source,
                })?;
        if evaluated.color_management != self.settings {
            return Err(refuse(
                "binding",
                None,
                "evaluator changed the prepared color contract",
            ));
        }
        self.validate_project(&evaluated)?;
        // Validate evaluated routes before Scene can resolve cameras, nested
        // compositions or raster sources. Evaluate exactly once per sample.
        self.spare
            .sample_inner(
                project,
                &evaluated,
                cursor,
                ViewCamera::Composition,
                0,
                &mut 0,
                &AlreadyEvaluated,
            )
            .map_err(|source| AuthoredSceneError::Model {
                phase: "sample",
                source,
            })?;
        if self.spare.has_native_lens() || self.spare.has_native_depth_of_field() {
            return Err(refuse(
                "sample",
                None,
                "native camera raster consumers require managed color integration",
            ));
        }
        self.spare.background = transform(
            &self.native,
            self.input_range,
            self.input_alpha,
            None,
            self.spare.background,
        )?;
        for layer in &mut self.spare.layers {
            // The working image is already transformed and premultiplied. Its
            // neutral tint is an identity multiplier, not an authored color.
            if layer.asset.is_some() || layer.vector.is_some() {
                continue;
            }
            layer.color = transform(
                &self.native,
                self.input_range,
                self.input_alpha,
                Some(layer.id),
                layer.color,
            )?;
        }
        self.vectors.apply(
            &mut self.spare,
            &self.native,
            self.input_range,
            self.input_alpha,
        )?;
        std::mem::swap(&mut self.current.scene, &mut self.spare);
        self.published = true;
        Ok(&self.current)
    }
    /// Check an edit candidate against the installed immutable binding without
    /// loading native libraries or reopening a configuration during sampling.
    pub fn validate_project(&self, project: &Project) -> Result<()> {
        if project.color_management != self.settings {
            return Err(refuse(
                "binding",
                None,
                "project color settings changed; prepare a new sampler",
            ));
        }
        admit(project)?;
        if project.assets.as_slice() != self.current.assets.as_ref() {
            return Err(refuse(
                "source_binding",
                None,
                "source declarations changed; prepare new image bindings and sampler",
            ));
        }
        Ok(())
    }
    /// Explicit inspection only; an Err from sample must still reach the host's
    /// error channel. This is not an automatic last-good-frame fallback.
    pub fn current(&self) -> Option<&AuthoredSceneFrame> {
        self.published.then_some(&self.current)
    }
    pub fn contract(&self) -> ColorImageContract {
        self.current.contract
    }
    /// Frozen declaration snapshot. External file changes require explicit
    /// source preparation; no filesystem probing occurs during frame sampling.
    pub fn source_declarations(&self) -> &[motion_model::Asset] {
        &self.current.assets
    }
}

struct AlreadyEvaluated;
impl motion_model::FrameEvaluator for AlreadyEvaluated {
    fn evaluate<'a>(
        &self,
        project: &'a Project,
        _: f64,
    ) -> motion_model::Result<std::borrow::Cow<'a, Project>> {
        Ok(std::borrow::Cow::Borrowed(project))
    }
}

fn admit(project: &Project) -> Result<()> {
    if project.working_precision != ChannelPrecision::Float32
        || project.width == 0
        || project.height == 0
        || project.layers.len() > motion_model::MAX_LAYERS
    {
        return Err(refuse(
            "admission",
            None,
            "authored composition requires valid Float32 project dimensions and layer capacity",
        ));
    }
    if project.camera.imported.is_some() || project.camera.tracking.is_some() {
        return Err(refuse(
            "admission",
            None,
            "source camera rendering requires managed color integration",
        ));
    }
    for layer in &project.layers {
        if !matches!(
            layer.content,
            Content::Solid { .. }
                | Content::Image { .. }
                | Content::Vector { .. }
                | Content::Null
                | Content::Audio { .. }
        ) || layer.three_d
            || !layer.effects.is_empty()
            || !layer.masks.is_empty()
            || layer.track_matte.is_some()
            || layer.blend != motion_model::compositing::LayerBlend::default()
            || !layer.sampling.is_default()
        {
            return Err(refuse(
                "admission",
                Some(layer.id),
                "this color consumer is not integrated with the authored composition path yet",
            ));
        }
        if let Content::Image { asset } = layer.content {
            if !project.assets.iter().any(|source| source.id == asset) {
                return Err(refuse(
                    "source_binding",
                    Some(asset),
                    "static image declaration is missing",
                ));
            }
            if let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() {
                if settings.sources.iter().any(|source| {
                    source.asset == asset
                        && matches!(
                            source.interpretation,
                            motion_model::color_management::SourceInterpretation::Data { .. }
                        )
                }) {
                    return Err(refuse(
                        "source_binding",
                        Some(asset),
                        "numeric data cannot enter the color image compositor",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn transform(
    native: &NativeColorProgram,
    range: CodeValueRange,
    alpha_mode: AlphaMode,
    object: Option<u64>,
    rgba: [f32; 4],
) -> Result<[f32; 4]> {
    if rgba.iter().any(|value| !value.is_finite()) || !(0.0..=1.0).contains(&rgba[3]) {
        return Err(refuse(
            "transform",
            object,
            "authored RGBA must be finite, with alpha in [0,1]",
        ));
    }
    let mut rgb = [rgba[0], rgba[1], rgba[2]];
    if alpha_mode == AlphaMode::Premultiplied {
        for value in &mut rgb {
            *value = if rgba[3] > 0. {
                *value / rgba[3]
            } else {
                range.black()
            };
        }
    }
    if !range.is_full() {
        for value in &mut rgb {
            *value = (*value - range.black()) / (range.white() - range.black());
        }
    }
    let rgb = native
        .apply_rgb(rgb)
        .map_err(|source| AuthoredSceneError::Native { object, source })?;
    // Scene tints are straight; the actual draw/clear multiplies alpha once.
    // Preserve negative and over-one working RGB without an sRGB round-trip.
    Ok([rgb[0], rgb[1], rgb[2], rgba[3]])
}
