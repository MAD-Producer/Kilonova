//! Original-file input preparation. The result is reusable working-space GPU
//! storage, not permission to run the still-unmanaged ordinary Session/Scene.
use super::*;
use motion_effects::image_contract::ChannelPrecision;
use motion_render::{
    image_resources::{self, Source},
    ColorImageContract, ColorImageTarget, ColorProcessorOutput, Renderer,
};

pub struct SourceImageRequest<'a> {
    pub project: &'a Project,
    pub root: &'a Path,
    pub asset: u64,
    pub cancel: &'a AtomicBool,
}

// Retain both the directory prefix and filename, escaping control characters.
// At most 64 Unicode scalars (640 escaped bytes) enter an error owner.
fn source_path(path: &str) -> String {
    let value = if path.chars().count() <= 64 {
        path.to_owned()
    } else {
        let first: String = path.chars().take(30).collect();
        let last: String = path
            .chars()
            .rev()
            .take(30)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{first}…{last}")
    };
    value.escape_debug().to_string()
}

#[cfg(test)]
mod tests;

pub struct PreparedSourceImage {
    asset: u64,
    source_precision: ChannelPrecision,
    working: ColorImageTarget,
}
impl PreparedSourceImage {
    pub fn asset(&self) -> u64 {
        self.asset
    }
    pub fn source_precision(&self) -> ChannelPrecision {
        self.source_precision
    }
    pub fn working(&self) -> &ColorImageTarget {
        &self.working
    }
    pub fn into_working(self) -> ColorImageTarget {
        self.working
    }
}

impl PreparedProjectColors {
    /// Run once per source installation in an observed non-Present operation.
    /// None means an explicitly declared numeric-data route; no file is opened
    /// and no color conversion or GPU allocation occurs for that route. Unknown
    /// assets, videos and stale settings are errors, never default-color fallbacks.
    /// Cancellation is checked through decoding and before the first queue write.
    /// After that commit point, finish one submission; no retry or restart occurs.
    pub fn prepare_source_image(
        &self,
        renderer: &mut Renderer,
        request: SourceImageRequest<'_>,
        observation: &Observation,
    ) -> Result<Option<PreparedSourceImage>, Error> {
        self.prepare_image_with(
            renderer,
            request,
            observation,
            image_resources::decode_color_f32,
        )
    }

    fn prepare_image_with(
        &self,
        renderer: &mut Renderer,
        request: SourceImageRequest<'_>,
        observation: &Observation,
        decode: impl FnOnce(
            &Source,
            u64,
            &AtomicBool,
        ) -> Result<image_resources::ColorPixels, image_resources::Error>,
    ) -> Result<Option<PreparedSourceImage>, Error> {
        if observation
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::Busy);
        }
        let mut flight = Flight {
            observation,
            operation: "prepare_source_color_image",
            started: Instant::now(),
            finished: false,
        };
        color_log!(Info, "color preparation request={} operation=prepare_source_color_image asset={} phase=start", observation.request, request.asset);
        let mut phase = "validate";
        let result = (|| {
            request.project.validate().map_err(Error::Project)?;
            let fail = |reason| Error::SourceBinding {
                asset: request.asset,
                reason,
            };
            if request.project.color_management.as_ref() != self.settings() {
                return Err(fail("prepared settings differ from current project"));
            }
            let asset = request
                .project
                .assets
                .iter()
                .find(|a| a.id == request.asset)
                .ok_or_else(|| fail("source is not a static project image"))?;
            let input = self
                .source(asset.id)
                .ok_or_else(|| fail("source has no prepared interpretation"))?;
            let PreparedInput::Color(processor) = input else {
                return Ok(None);
            };
            let context = |phase, source| Error::SourceImage {
                asset: request.asset,
                phase,
                path: source_path(&asset.path),
                source: Box::new(source),
            };
            let cancelled = || {
                if request.cancel.load(Ordering::Relaxed) {
                    Err(context(
                        "cancel",
                        crate::Error::ImageResource(image_resources::Error::Cancelled),
                    ))
                } else {
                    Ok(())
                }
            };
            cancelled()?;
            phase = "resolve_source";
            color_log!(Info, "color preparation request={} operation=prepare_source_color_image asset={} phase={phase} path={}", observation.request, asset.id, asset.path.chars().take(512).collect::<String>().escape_debug());
            let source = Source::new(request.root, asset).map_err(|e| context(phase, e.into()))?;
            phase = "decode_original";
            color_log!(Info, "color preparation request={} operation=prepare_source_color_image asset={} phase={phase}", observation.request, asset.id);
            let pixels = decode(
                &source,
                renderer.memory_policy().decoded_image_budget_bytes,
                request.cancel,
            )
            .map_err(|e| context(phase, e.into()))?;
            cancelled()?;
            let source_precision = pixels.source_precision;
            phase = "upload_float32";
            color_log!(Info, "color preparation request={} operation=prepare_source_color_image asset={} phase={phase} size={}x{} precision={source_precision:?}", observation.request, asset.id, pixels.width, pixels.height);
            let raw = renderer
                .upload_color_image_f32(
                    pixels.width,
                    pixels.height,
                    ColorImageContract {
                        space: processor.input_space(),
                        range: processor.input_range(),
                        alpha: processor.input_alpha(),
                        precision: ChannelPrecision::Float32,
                    },
                    &pixels.rgba,
                )
                .map_err(|e| context(phase, e.into()))?;
            drop(pixels);
            phase = "compile_input_transform";
            color_log!(Info, "color preparation request={} operation=prepare_source_color_image asset={} phase={phase}", observation.request, asset.id);
            let working = renderer
                .color_image_target(
                    source.width,
                    source.height,
                    ColorImageContract {
                        space: processor.output_space(),
                        range: processor.output_range(),
                        alpha: processor.output_alpha(),
                        precision: ChannelPrecision::Float32,
                    },
                )
                .map_err(|e| context(phase, e.into()))?;
            let transform =
                ColorProcessorOutput::new(renderer, &raw, processor, working.contract())
                    .map_err(|e| context(phase, e.into()))?;
            phase = "submit_input_transform";
            color_log!(Info, "color preparation request={} operation=prepare_source_color_image asset={} phase={phase}", observation.request, asset.id);
            let mut encoder = renderer
                .allocation_command_encoder_for(
                    motion_render::gpu_commands::CommandStage::MediaPreparation,
                    &Default::default(),
                )
                .map_err(|e| context(phase, e.into()))?;
            transform
                .encode(renderer, &mut encoder, &working)
                .map_err(|e| context(phase, e.into()))?;
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    ledger.merge(raw.texture_resource_ledger());
                    ledger.merge(transform.texture_resource_ledger());
                    ledger.merge(working.texture_resource_ledger());
                })
                .map_err(|e| context(phase, e.into()))?;
            Ok(Some(PreparedSourceImage {
                asset: asset.id,
                source_precision,
                working,
            }))
        })();
        match &result {
            Ok(_) => color_log!(Debug, "color preparation request={} operation=prepare_source_color_image asset={} phase=done elapsed_ms={}", observation.request, request.asset, flight.started.elapsed().as_millis()),
            Err(Error::SourceImage { source, .. }) if matches!(source.as_ref(), crate::Error::ImageResource(image_resources::Error::Cancelled)) => {
                color_log!(Debug, "color preparation request={} operation=prepare_source_color_image asset={} phase=cancelled elapsed_ms={}", observation.request, request.asset, flight.started.elapsed().as_millis());
            }
            Err(error) => color_log!(Error, "color preparation request={} operation=prepare_source_color_image asset={} phase={phase} elapsed_ms={} cause={}", observation.request, request.asset, flight.started.elapsed().as_millis(), brief(error)),
        }
        flight.finished = true;
        result
    }
}
