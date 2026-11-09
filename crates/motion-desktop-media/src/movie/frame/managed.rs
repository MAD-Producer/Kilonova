//! Explicit full-range PNG output from the project's native colour program.
//! Display/inspection programs never participate in this export path.
use super::*;
use motion_core::color_management::{
    ColorSignal, ColorTransfer, ProjectColorManagement, SignalRange,
};
use motion_effects::{AlphaMode, image_contract::ChannelPrecision};
use motion_host::color_preparation::{self, Observation};
use motion_render::{ColorImageContract, ColorProcessorOutput};

pub(super) struct ManagedPng {
    pub description: serde_json::Value,
    signal: ColorSignal,
}

fn qualify(signal: &ColorSignal) -> Result<()> {
    if signal.range != (SignalRange::Full {}) {
        return Err("PNG colour phase=output_contract: full-range output is required".into());
    }
    match signal.transfer {
        ColorTransfer::Linear => {
            if signal.primaries.iter().flatten().chain(signal.white_point.iter())
                .any(|value| !value.is_finite() || !(0. ..=1.).contains(value)) {
                return Err("PNG colour phase=output_contract: chromaticities cannot be represented by PNG cHRM".into());
            }
        }
        ColorTransfer::Srgb => {
            if signal.primaries != [[0.64,0.33],[0.3,0.6],[0.15,0.06]]
                || signal.white_point != [0.3127,0.3290] {
                return Err("PNG colour phase=output_contract: sRGB transfer with other primaries needs an explicit profile".into());
            }
        }
        ColorTransfer::ConfigDefined => return Err(
            "PNG colour phase=output_contract: config-defined transfer needs an explicit PNG profile".into()),
    }
    Ok(())
}

pub(super) fn render(
    renderer: &mut Renderer,
    project: &Project,
    root: &Path,
    platform: &dyn motion_host::Platform,
    frame: f64,
    request: u64,
    control: &ExportControl,
) -> Result<(Vec<u8>, ManagedPng)> {
    let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
        return Err("PNG colour phase=output_contract: managed document is missing".into());
    };
    qualify(&settings.output)?;
    control.check()?;
    let manifest = platform
        .color_runtime_manifest()
        .map_err(|e| format!("PNG colour phase=runtime: {e}"))?
        .ok_or("PNG colour phase=runtime: native OCIO runtime is not installed")?;
    let observation = Observation::new(request);
    let mut prepared =
        color_preparation::prepare_native_authored_project(project, root, &manifest, &observation)
            .map_err(|e| format!("PNG colour phase=prepare: {e}"))?
            .ok_or("PNG colour phase=prepare: managed programs are missing")?;
    control.check()?;
    let images = prepared
        .prepare_images(
            renderer,
            project,
            root,
            control.cancellation_flag(),
            &observation,
        )
        .map_err(|error| {
            match &error {
                color_preparation::Error::SourceImage { source, .. }
                    if matches!(
                        source.as_ref(),
                        motion_host::Error::ImageResource(
                            motion_render::image_resources::Error::Cancelled
                        )
                    ) =>
                {
                    // Preserve the existing normal-cancellation reply. A concurrent
                    // cancellation must not replace any other primary failure.
                    "movie export cancelled".into()
                }
                _ => format!("PNG colour phase=sources: {error}"),
            }
        })?;
    control.check()?;
    let input = renderer
        .color_image_target(project.width, project.height, prepared.authored.contract())
        .map_err(|e| format!("PNG colour phase=working_target: {e}"))?;
    let program = prepared.colors.output();
    let output_contract = ColorImageContract {
        space: program.output_space(),
        range: program.output_range(),
        alpha: program.output_alpha(),
        precision: ChannelPrecision::Float32,
    };
    let output = renderer
        .color_image_target(project.width, project.height, output_contract)
        .map_err(|e| format!("PNG colour phase=output_target: {e}"))?;
    let conversion = ColorProcessorOutput::new(renderer, &input, program, output_contract)
        .map_err(|e| format!("PNG colour phase=output_program: {e}"))?;
    let program_key = program.key();
    let context = prepared
        .authored
        .sample(project, frame, &motion_core::ExpressionEvaluator)
        .map_err(|e| format!("PNG colour phase=sample: {e}"))?;
    renderer
        .draw_authored_scene_with_images(context, &input, &images)
        .map_err(|e| format!("PNG colour phase=compose: {e}"))?;
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("managed PNG output conversion"),
        })
        .map_err(|e| format!("PNG colour phase=encoder: {e}"))?;
    conversion
        .encode(renderer, &mut encoder, &output)
        .map_err(|e| format!("PNG colour phase=output: {e}"))?;
    control.check()?;
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(input.texture_resource_ledger());
            ledger.merge(output.texture_resource_ledger());
            ledger.merge(images.texture_resource_ledger());
            ledger.merge(conversion.texture_resource_ledger());
        })
        .map_err(|e| format!("PNG colour phase=submit: {e}"))?;
    renderer
        .check_health()
        .map_err(|e| format!("PNG colour phase=health: {e}"))?;
    let image = renderer
        .read_color_image_target(&output)
        .map_err(|e| format!("PNG colour phase=readback: {e}"))?;
    control.check()?;
    let expected_bytes = u64::from(project.width) * u64::from(project.height) * 16;
    if image.contract != output_contract || image.pixels.len() as u64 != expected_bytes {
        return Err("PNG colour phase=readback: output contract or pixel length mismatch".into());
    }
    let mut pixels = Vec::with_capacity(project.width as usize * project.height as usize * 4);
    for (index, pixel) in image.pixels.chunks_exact(16).enumerate() {
        if index % 1024 == 0 {
            control.check()?;
        }
        let mut value: [f32; 4] = std::array::from_fn(|c| {
            f32::from_le_bytes([
                pixel[c * 4],
                pixel[c * 4 + 1],
                pixel[c * 4 + 2],
                pixel[c * 4 + 3],
            ])
        });
        if !value.iter().all(|v| v.is_finite()) || !(0. ..=1.).contains(&value[3]) {
            return Err(format!(
                "PNG colour phase=quantize pixel={index}: non-finite output or invalid alpha"
            ));
        }
        if image.contract.alpha == AlphaMode::Premultiplied {
            for channel in 0..3 {
                value[channel] = if value[3] > 0. {
                    value[channel] / value[3]
                } else {
                    0.
                };
            }
        }
        if !value.iter().all(|v| v.is_finite()) {
            return Err(format!(
                "PNG colour phase=quantize pixel={index}: unassociation overflow"
            ));
        }
        pixels.extend(value.map(|v| (v.clamp(0., 1.) * 255.).round() as u8));
    }
    Ok((
        pixels,
        ManagedPng {
            signal: settings.output.clone(),
            description: serde_json::json!({
                "version":1,"configKey":prepared.colors.config().key(),"programKey":program_key,
                "output":settings.selection.output,"signal":settings.output,
                "storage":{"format":"rgba8","alpha":"straight","range":"full","quantization":"clip_round"},
            }),
        },
    ))
}

#[cfg(test)]
mod tests;

impl ManagedPng {
    pub fn encode(&self, path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
        qualify(&self.signal)?;
        let file =
            fs::File::create(path).map_err(|e| format!("PNG colour phase=encode_file: {e}"))?;
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        if self.signal.transfer == ColorTransfer::Srgb {
            encoder.set_source_srgb(png::SrgbRenderingIntent::RelativeColorimetric);
        } else {
            let pair = |p: [f64; 2]| (p[0] as f32, p[1] as f32);
            encoder.set_source_gamma(png::ScaledFloat::new(1.));
            encoder.set_source_chromaticities(png::SourceChromaticities::new(
                pair(self.signal.white_point),
                pair(self.signal.primaries[0]),
                pair(self.signal.primaries[1]),
                pair(self.signal.primaries[2]),
            ));
        }
        encoder
            .add_itxt_chunk("MotionStudioColor".into(), self.description.to_string())
            .map_err(|e| format!("PNG colour phase=metadata: {e}"))?;
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG colour phase=header: {e}"))?;
        writer
            .write_image_data(pixels)
            .map_err(|e| format!("PNG colour phase=encode_pixels: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("PNG colour phase=encode_finish: {e}"))
    }
}
