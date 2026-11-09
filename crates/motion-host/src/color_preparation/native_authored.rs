//! One observed preparation retains both immutable GPU image programs and the
//! exact native authored-color CPU program. Nothing is installed in Session yet.
use super::*;
use motion_effects::native_color::NativeColorRuntime;
use motion_render::AuthoredSceneSampler;

pub struct PreparedNativeAuthoredProject {
    pub colors: PreparedProjectColors,
    pub authored: AuthoredSceneSampler,
}
impl PreparedNativeAuthoredProject {
    /// Reuse immutable programs and verified configuration, but give the next
    /// composition an exclusive native sampler. Nothing is published here.
    pub fn prepare_composition(
        &self,
        project: &Project,
        observation: &Observation,
    ) -> Result<Self, Error> {
        observed_operation(observation, "prepare_color_composition", || {
            let authored = self.authored.for_composition(project, &self.colors.config)
                .map_err(motion_render::RenderError::from)
                .map_err(|source| Error::Composition {
                    composition: project.composition_id.clone(), source: Box::new(source.into()),
                })?;
            Ok(Self { colors: self.colors.clone(), authored })
        })
    }

    /// Prepare each referenced original static image once, then retain immutable
    /// working targets and sampling bindings. No partial set is published on
    /// failure or cancellation. Replacing a source requires a new explicit call.
    pub fn prepare_images(
        &self,
        renderer: &mut motion_render::Renderer,
        project: &Project,
        root: &Path,
        cancel: &AtomicBool,
        observation: &Observation,
    ) -> Result<motion_render::AuthoredImageSet, Error> {
        observed_operation(observation, "prepare_authored_images", || {
            project.validate().map_err(Error::Project)?;
            if project.color_management.as_ref() != self.colors.settings()
                || project.assets.as_slice() != self.authored.source_declarations()
            {
                return Err(Error::Binding {
                    target: Target::AuthoredInput,
                    phase: ColorProcessorPhase::InputToWorking,
                    reason: "source or color declarations changed; prepare a new authored project",
                });
            }
            let mut sources = Vec::new();
            for asset in &project.assets {
                if !project.layers.iter().any(|layer| matches!(layer.content, motion_core::Content::Image { asset: id } if id == asset.id)) {
                    continue;
                }
                // The parent remains visibly in flight while each per-source
                // operation publishes its asset/phase with the same request ID.
                let child = Observation::new(observation.request());
                let image = self
                    .colors
                    .prepare_source_image(
                        renderer,
                        SourceImageRequest {
                            project,
                            root,
                            asset: asset.id,
                            cancel,
                        },
                        &child,
                    )?
                    .ok_or(Error::SourceBinding {
                        asset: asset.id,
                        reason: "numeric data cannot enter the color image compositor",
                    })?;
                sources.push((asset.clone(), Arc::new(image.into_working())));
            }
            renderer
                .prepare_authored_images(self.authored.contract(), sources)
                .map_err(|source| Error::WorkingImages(Box::new(source.into())))
        })
    }
}

/// Prepare on the owning worker, then transfer the complete exclusive result if
/// needed. Native handles are never shared or exposed. The whole result is
/// published only after GPU programs and authored binding pass.
pub fn prepare_native_authored_project(
    project: &Project,
    root: &Path,
    runtime_manifest: &Path,
    observation: &Observation,
) -> Result<Option<PreparedNativeAuthoredProject>, Error> {
    observed_prepare(observation, || {
        let selection = match project.color_management.as_ref() {
            ProjectColorManagement::Ocio { settings } => {
                let mut selection = settings.selection.clone();
                selection.input = settings.authored_input.space.clone();
                Some((
                    selection,
                    settings.authored_input.signal.alpha,
                    settings.working.alpha,
                ))
            }
            ProjectColorManagement::Unmanaged {} => None,
        };
        let mut runtime = None;
        let mut authored = None;
        let colors = prepare(
            project,
            root,
            &mut |request| {
                if runtime.is_none() {
                    runtime = Some(NativeColorRuntime::load(runtime_manifest)?);
                }
                let native = runtime.as_ref().ok_or_else(|| {
                    crate::Error::from(motion_effects::native_color::Error::Admission(
                        "native runtime was not retained",
                    ))
                })?;
                let program = native.prepare(
                    request.config,
                    request.selection,
                    request.phase,
                    request.input_alpha,
                    request.output_alpha,
                )?;
                let source = program.source().clone();
                // AuthoredInput may share DefaultInput's exact key. Retain by full
                // selection and alpha identity, rather than the first target label.
                if authored.is_none()
                    && request.phase == ColorProcessorPhase::InputToWorking
                    && selection
                        .as_ref()
                        .is_some_and(|(selection, input, output)| {
                            request.selection == selection
                                && request.input_alpha == *input
                                && request.output_alpha == *output
                        })
                {
                    authored = Some(
                        AuthoredSceneSampler::from_program(project, request.config, program)
                            .map_err(motion_render::RenderError::from)?,
                    );
                }
                Ok(source)
            },
            observation,
        )?;
        match (colors, authored) {
            (None, None) => Ok(None),
            (Some(colors), Some(authored)) => {
                Ok(Some(PreparedNativeAuthoredProject { colors, authored }))
            }
            _ => Err(Error::Binding {
                target: Target::AuthoredInput,
                phase: ColorProcessorPhase::InputToWorking,
                reason: "native authored program was not retained; no partial result was installed",
            }),
        }
    })
}
