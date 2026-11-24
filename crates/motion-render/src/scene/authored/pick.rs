//! Convert a retained working pixel back to its declared authored endpoint.
//! Display/view transforms and inspection overlays never enter this path.
use super::*;
use motion_effects::color_config::OutputSelection;
use motion_model::color_management::OcioProject;

pub(super) type Program =
    std::cell::OnceCell<std::result::Result<NativeColorProgram, Arc<native_color::Error>>>;

fn prepare(
    native: &NativeColorProgram,
    config: &VerifiedColorConfig,
    settings: &OcioProject,
) -> std::result::Result<NativeColorProgram, Arc<native_color::Error>> {
    let mut selection = settings.selection.clone();
    selection.output = OutputSelection::Space {
        space: settings.authored_input.space.clone(),
    };
    native
        .runtime()
        .prepare(
            config,
            &selection,
            ColorProcessorPhase::WorkingToOutput,
            settings.working.alpha,
            settings.authored_input.signal.alpha,
        )
        .map_err(Arc::new)
}

impl AuthoredSceneSampler {
    /// Explicit cold eyedropper consumer. The input must be an uninspected
    /// pixel from this sampler's complete working contract, not display RGB.
    /// Preserve negative/over-one values; the editor decides parameter limits.
    /// Run inside an observed non-Present operation: the first nontransparent
    /// pick prepares the native CPU endpoint from retained verified bytes.
    pub fn working_color_to_authored(
        &self,
        config: &VerifiedColorConfig,
        contract: ColorImageContract,
        rgba: [f32; 4],
    ) -> Result<[f32; 4]> {
        if contract != self.contract() {
            return Err(AuthoredSceneError::Destination {
                expected: self.contract(),
                actual: contract,
            });
        }
        if rgba.iter().any(|v| !v.is_finite()) || !(0.0..=1.0).contains(&rgba[3]) {
            return Err(refuse(
                "color_pick",
                None,
                "working RGBA must be finite, with alpha in [0,1]",
            ));
        }
        if config.key() != self.native.source().config_key {
            return Err(refuse(
                "color_pick_prepare",
                None,
                "verified config differs from authored owner",
            ));
        }
        let mut rgb = if rgba[3] > 0. {
            let ProjectColorManagement::Ocio { settings } = self.settings.as_ref() else {
                return Err(refuse(
                    "color_pick_prepare",
                    None,
                    "managed authored owner is unavailable",
                ));
            };
            // Explicit cold operation only, never called by Present or hover.
            // Retain a terminal preparation failure too; no automatic retry.
            let program = self
                .pick
                .get_or_init(|| prepare(&self.native, config, settings))
                .as_ref()
                .map_err(|source| AuthoredSceneError::PickNative {
                    phase: "color_pick_prepare",
                    source: source.clone(),
                })?;
            program
                .apply_rgb([rgba[0] / rgba[3], rgba[1] / rgba[3], rgba[2] / rgba[3]])
                .map_err(|source| AuthoredSceneError::PickNative {
                    phase: "color_pick_transform",
                    source: Arc::new(source),
                })?
        } else {
            // A fully transparent premultiplied pixel has no recoverable RGB.
            [0.; 3]
        };
        for value in &mut rgb {
            *value = *value * (self.input_range.white() - self.input_range.black())
                + self.input_range.black();
            if self.input_alpha == AlphaMode::Premultiplied {
                *value *= rgba[3];
            }
        }
        if rgb.iter().any(|v| !v.is_finite()) {
            return Err(refuse(
                "color_pick",
                None,
                "authored RGB range conversion is nonfinite",
            ));
        }
        Ok([rgb[0], rgb[1], rgb[2], rgba[3]])
    }
}
