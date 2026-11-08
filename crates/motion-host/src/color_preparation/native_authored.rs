//! One observed preparation retains both immutable GPU image programs and the
//! exact native authored-color CPU program. Nothing is installed in Session yet.
use super::*;
use motion_effects::native_color::NativeColorRuntime;
use motion_render::AuthoredSceneSampler;

pub struct PreparedNativeAuthoredProject {
    pub colors: PreparedProjectColors,
    pub authored: AuthoredSceneSampler,
}

/// Prepare on the owning render worker. The native sampler is thread-confined;
/// it is not smuggled across threads with an unsafe Send implementation. The
/// whole result is published only after GPU programs and authored binding pass.
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
