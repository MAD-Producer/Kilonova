//! Explicit project-output RGB for the existing SDR H.264/AAC encoder.
use super::*;
use motion_core::color_management::{ColorTransfer, ProjectColorManagement, SignalRange};
use motion_effects::image_contract::ChannelPrecision;
use motion_host::color_preparation::{self, Observation, PreparedNativeAuthoredProject};
use motion_render::{
    AuthoredImageSet, ColorImageContract, ColorImageTarget, ColorProcessorOutput, Renderer,
};

pub(super) fn qualify(project: &Project) -> Result<()> {
    let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
        return Err("MP4 colour phase=output_contract: managed document is missing".into());
    };
    if settings.output.transfer != ColorTransfer::Srgb
        || settings.output.primaries != [[0.64, 0.33], [0.3, 0.6], [0.15, 0.06]]
        || settings.output.white_point != [0.3127, 0.3290]
        || settings.output.range != (SignalRange::Full {})
    {
        return Err("MP4 colour phase=output_contract: encoder requires declared full-range sRGB RGB with BT.709 primaries and D65 white; select a matching project output".into());
    }
    if motion_host::render_sampling::needs_exposure(project)? {
        return Err(
            "MP4 colour phase=exposure: managed temporal exposure requires explicit integration"
                .into(),
        );
    }
    Ok(())
}

pub(super) struct ManagedMovie {
    prepared: PreparedNativeAuthoredProject,
    images: AuthoredImageSet,
    working: ColorImageTarget,
    output: ColorImageTarget,
    conversion: ColorProcessorOutput,
    pub description: serde_json::Value,
}
impl ManagedMovie {
    pub fn prepare(
        renderer: &mut Renderer,
        project: &Project,
        root: &Path,
        platform: &dyn motion_host::Platform,
        request: u64,
        control: &ExportControl,
    ) -> Result<Self> {
        qualify(project)?;
        control.check()?;
        let manifest = platform
            .color_runtime_manifest()
            .map_err(|e| format!("MP4 colour phase=runtime: {e}"))?
            .ok_or("MP4 colour phase=runtime: native OCIO runtime is not installed")?;
        let observation = Observation::new(request);
        let prepared = color_preparation::prepare_native_authored_project(
            project,
            root,
            &manifest,
            &observation,
        )
        .map_err(|e| format!("MP4 colour phase=prepare: {e}"))?
        .ok_or("MP4 colour phase=prepare: managed programs are missing")?;
        control.check()?;
        let program = color_preparation::prepare_native_opaque_output(
            &prepared.colors,
            &manifest,
            &observation,
        )
        .map_err(|e| format!("MP4 colour phase=opaque_output: {e}"))?;
        let images = prepared
            .prepare_images(
                renderer,
                project,
                root,
                control.cancellation_flag(),
                &observation,
            )
            .map_err(|e| match &e {
                color_preparation::Error::SourceImage { source, .. }
                    if matches!(
                        source.as_ref(),
                        motion_host::Error::ImageResource(
                            motion_render::image_resources::Error::Cancelled
                        )
                    ) =>
                {
                    "movie export cancelled".into()
                }
                _ => format!("MP4 colour phase=sources: {e}"),
            })?;
        control.check()?;
        let working = renderer
            .color_image_target(project.width, project.height, prepared.authored.contract())
            .map_err(|e| format!("MP4 colour phase=working_target: {e}"))?;
        let contract = ColorImageContract {
            space: program.output_space(),
            range: program.output_range(),
            alpha: program.output_alpha(),
            precision: ChannelPrecision::Float32,
        };
        let output = renderer
            .color_image_target(project.width, project.height, contract)
            .map_err(|e| format!("MP4 colour phase=output_target: {e}"))?;
        let conversion = ColorProcessorOutput::new(renderer, &working, &program, contract)
            .map_err(|e| format!("MP4 colour phase=output_program: {e}"))?;
        let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
            return Err("MP4 colour phase=output_contract: managed declaration changed".into());
        };
        let description = serde_json::json!({
            "version":1,"configKey":prepared.colors.config().key(),"programKey":program.key(),
            "output":settings.selection.output,"signal":settings.output,
            "rgbTransport":{"format":"rgba8","alpha":"opaque","range":"full","quantization":"clip_round","matte":"working_black_before_output"},
            "encoded":{"codec":"h264","matrix":"bt709","primaries":"bt709","transfer":"iec61966_2_1","range":"limited","hdr":false}
        });
        Ok(Self {
            prepared,
            images,
            working,
            output,
            conversion,
            description,
        })
    }

    pub fn render(
        &mut self,
        renderer: &mut Renderer,
        project: &Project,
        frame: f64,
        control: &ExportControl,
    ) -> Result<Vec<u8>> {
        control.check()?;
        let context = self
            .prepared
            .authored
            .sample(project, frame, &motion_core::ExpressionEvaluator)
            .map_err(|e| format!("MP4 colour frame={frame} phase=sample: {e}"))?;
        renderer
            .draw_authored_scene_with_images(context, &self.working, &self.images)
            .map_err(|e| format!("MP4 colour frame={frame} phase=compose: {e}"))?;
        let mut encoder = renderer
            .allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("managed movie output conversion"),
            })
            .map_err(|e| format!("MP4 colour frame={frame} phase=encoder: {e}"))?;
        self.conversion
            .encode(renderer, &mut encoder, &self.output)
            .map_err(|e| format!("MP4 colour frame={frame} phase=output: {e}"))?;
        control.check()?;
        renderer
            .submit_allocated(encoder.finish(), |ledger| {
                ledger.merge(self.working.texture_resource_ledger());
                ledger.merge(self.output.texture_resource_ledger());
                ledger.merge(self.images.texture_resource_ledger());
                ledger.merge(self.conversion.texture_resource_ledger());
            })
            .map_err(|e| format!("MP4 colour frame={frame} phase=submit: {e}"))?;
        renderer
            .check_health()
            .map_err(|e| format!("MP4 colour frame={frame} phase=health: {e}"))?;
        let image = renderer
            .read_color_image_target(&self.output)
            .map_err(|e| format!("MP4 colour frame={frame} phase=readback: {e}"))?;
        control.check()?;
        let count = u64::from(project.width) * u64::from(project.height);
        if image.contract != self.output.contract() || image.pixels.len() as u64 != count * 16 {
            return Err(format!(
                "MP4 colour frame={frame} phase=readback: output contract or extent changed"
            ));
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(
                usize::try_from(count * 4)
                    .map_err(|e| format!("MP4 colour frame={frame} phase=quantize: {e}"))?,
            )
            .map_err(|e| format!("MP4 colour frame={frame} phase=quantize: {e}"))?;
        for (index, pixel) in image.pixels.chunks_exact(16).enumerate() {
            if index % 1024 == 0 {
                control.check()?;
            }
            let value: [f32; 4] = std::array::from_fn(|c| {
                f32::from_le_bytes([
                    pixel[c * 4],
                    pixel[c * 4 + 1],
                    pixel[c * 4 + 2],
                    pixel[c * 4 + 3],
                ])
            });
            if !value.iter().all(|v| v.is_finite()) || value[3] != 1. {
                return Err(format!("MP4 colour frame={frame} phase=quantize pixel={index}: output must be finite and opaque"));
            }
            pixels.extend(value.map(|v| (v.clamp(0., 1.) * 255.).round() as u8));
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests;
