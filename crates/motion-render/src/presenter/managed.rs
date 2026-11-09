//! Typed working-to-display endpoint and already-encoded surface transport.
//! This does not assert the OS/monitor gamut, HDR mode or ICC configuration.
use super::*;
use crate::{
    AllocationEncoder, ColorImageContract, ColorImageTarget, ColorProcessorOutput, RenderError,
    TextureResourceLedger,
};
use motion_effects::{
    color_processor::{CodeValueRange, ColorProcessorPhase, CompiledColorImageProcessor},
    image_contract::ChannelPrecision,
};

/// Only a checked WorkingToDisplay processor can create this producer. Its
/// private image cannot be relabelled as a legacy linear/sRGB render target.
pub struct ManagedDisplayTarget {
    image: ColorImageTarget,
    conversion: ColorProcessorOutput,
    // A numeric inspection source, with the original allocation retained. This
    // never acquires the transformed display target's colour identity.
    working: crate::AdmittedTextureView,
}
impl ManagedDisplayTarget {
    /// Cold pipeline/target preparation. Reuse the result between Present calls.
    pub fn new(
        renderer: &Renderer,
        working: &ColorImageTarget,
        processor: &CompiledColorImageProcessor,
    ) -> Result<Self, RenderError> {
        if processor.phase() != ColorProcessorPhase::WorkingToDisplay {
            return Err(RenderError::Invalid(
                "managed display requires a WorkingToDisplay processor".into(),
            ));
        }
        let output = ColorImageContract {
            space: processor.output_space(),
            range: processor.output_range(),
            alpha: processor.output_alpha(),
            precision: ChannelPrecision::Float32,
        };
        let conversion = ColorProcessorOutput::new(renderer, working, processor, output)?;
        let image = renderer.color_image_target(
            working.image().width(),
            working.image().height(),
            output,
        )?;
        let working = working.image().texture().admitted_view()?;
        Ok(Self { image, conversion, working })
    }
    pub fn contract(&self) -> ColorImageContract {
        self.image.contract()
    }
    /// Another target for the same prepared transform, including resized views.
    /// No native shader, pipeline or numeric table is created here.
    pub fn rebind(&self, renderer: &Renderer, working: &ColorImageTarget) -> Result<Self, RenderError> {
        let conversion = self.conversion.rebind(renderer, working)?;
        let image = renderer.color_image_target(
            working.image().width(), working.image().height(), self.contract(),
        )?;
        let working = working.image().texture().admitted_view()?;
        Ok(Self { image, conversion, working })
    }
    pub fn dimensions(&self) -> [u32; 2] {
        [self.image.image().width(), self.image.image().height()]
    }
    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
    ) -> Result<(), RenderError> {
        self.conversion.encode(renderer, encoder, &self.image)
    }
    pub fn collect_texture_resources(&self, ledger: &mut TextureResourceLedger) {
        ledger.merge(self.image.texture_resource_ledger());
        ledger.merge(self.conversion.texture_resource_ledger());
    }
}

/// Surface transport for already-transformed display values. Its private
/// Presenter cannot be rebound through the legacy raw-source API. The embedding
/// host must separately qualify that the selected display matches the surface.
pub struct ManagedDisplayPresenter {
    presenter: Presenter,
    working_group: wgpu::BindGroup,
    resources: TextureResourceLedger,
    owner: std::sync::Arc<()>,
    contract: ColorImageContract,
    processor: [u8; 32],
}
impl ManagedDisplayPresenter {
    pub fn new(
        renderer: &Renderer,
        source: &ManagedDisplayTarget,
        format: wgpu::TextureFormat,
    ) -> Result<Self, RenderError> {
        source.image.image().check_owner(renderer)?;
        if source.contract().range != CodeValueRange::FULL {
            return Err(RenderError::Invalid(
                "surface display transport requires full normalized code range".into(),
            ));
        }
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(RenderError::Invalid(
                "managed display surface format is not qualified SDR8 transport".into(),
            ));
        }
        let view = source.image.image().texture().admitted_view()?;
        let buffers = memory::Buffers::admitted_display_codes(renderer, format)?;
        let mut presenter = Presenter::with_encoding(renderer, &view, format, buffers, true);
        presenter.source_allocation = Some(view.lease.clone());
        let working_group = Presenter::source_group(&renderer.device, &presenter.layout,
            &presenter.sampler, &presenter.view_buffer, &source.working);
        let mut resources = TextureResourceLedger::default();
        source.collect_texture_resources(&mut resources);
        Ok(Self {
            presenter,
            working_group,
            resources,
            owner: renderer.allocation_owner.clone(),
            contract: source.contract(),
            processor: source.conversion.key(),
        })
    }
    /// Reuse the qualified transport pipeline with independent per-view
    /// uniforms. Sharing the prototype's writable uniform would make cells
    /// overwrite each other's geometry before queue submission.
    pub fn rebind(&self, renderer: &Renderer, source: &ManagedDisplayTarget) -> Result<Self, RenderError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            return Err(RenderError::Invalid("managed display pipeline belongs to another renderer".into()));
        }
        source.image.image().check_owner(renderer)?;
        if source.contract() != self.contract || source.conversion.key() != self.processor {
            return Err(RenderError::Invalid("managed display source differs from the prepared display contract or program".into()));
        }
        let view = source.image.image().texture().admitted_view()?;
        let buffers = memory::Buffers::admitted_display_codes(renderer, self.presenter.format)?;
        let mut presenter = self.presenter.with_prepared_pipeline(renderer, &view, buffers);
        presenter.source_allocation = Some(view.lease.clone());
        let working_group = Presenter::source_group(&renderer.device, &presenter.layout,
            &presenter.sampler, &presenter.view_buffer, &source.working);
        let mut resources = TextureResourceLedger::default();
        source.collect_texture_resources(&mut resources);
        Ok(Self {
            presenter, working_group, resources, owner: self.owner.clone(),
            contract: self.contract, processor: self.processor,
        })
    }
    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        output: &wgpu::Texture,
    ) -> Result<(), RenderError> {
        self.check_output(renderer, encoder, output)?;
        let view = output.create_view(&Default::default());
        self.presenter.encode(encoder, &view, None, None, [0.; 4]);
        Ok(())
    }
    pub fn encode_view(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        output: &wgpu::Texture,
        viewport: [f32; 4],
        image: Option<[f32; 4]>,
        background: [f32; 4],
    ) -> Result<(), RenderError> {
        self.encode_view_load(renderer, encoder, output, viewport, image, background, false)
    }
    /// Encode another independently bound cell, retaining earlier cells when
    /// `load` is true. The host clears once and owns the complete group commit.
    pub fn encode_view_load(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        output: &wgpu::Texture,
        viewport: [f32; 4],
        image: Option<[f32; 4]>,
        background: [f32; 4],
        load: bool,
    ) -> Result<(), RenderError> {
        self.encode_region_view_load(renderer, encoder, output, viewport, image, background, None, load)
    }
    /// Region coordinates are normalized to the composition image, then clipped
    /// to this cell. They never alter the retained display image or its uniforms
    /// for a later call. Earlier cells are preserved when `load` is true.
    pub fn encode_region_view_load(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        output: &wgpu::Texture,
        viewport: [f32; 4],
        image: Option<[f32; 4]>,
        background: [f32; 4],
        region: Option<[f32; 4]>,
        load: bool,
    ) -> Result<(), RenderError> {
        self.encode_inspected_region_view_load(renderer, encoder, output, viewport, image,
            background, ViewChannel::Color, region, load)
    }
    /// Colour uses the prepared native display result. Other channels expose
    /// clamped raw working components as opaque numeric intensities, including
    /// pre-matte alpha. They do not pass through an OCIO colour transform.
    pub fn encode_inspected_region_view_load(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        output: &wgpu::Texture,
        viewport: [f32; 4],
        image: Option<[f32; 4]>,
        background: [f32; 4],
        channel: ViewChannel,
        region: Option<[f32; 4]>,
        load: bool,
    ) -> Result<(), RenderError> {
        self.check_output(renderer, encoder, output)?;
        let [x, y, w, h] = viewport;
        if !viewport.iter().all(|v| v.is_finite())
            || x < 0.
            || y < 0.
            || w <= 0.
            || h <= 0.
            || x + w > output.width() as f32
            || y + h > output.height() as f32
            || image.is_some_and(|v| !v.iter().all(|v| v.is_finite()) || v[2] <= 0. || v[3] <= 0.)
            || !background.iter().all(|v| v.is_finite())
        {
            return Err(RenderError::Invalid(
                "invalid managed display viewport or image".into(),
            ));
        }
        let view = output.create_view(&Default::default());
        let group = if channel == ViewChannel::Color { &self.presenter.view_group }
            else { &self.working_group };
        self.presenter.encode_view_region_group_load(
            &renderer.queue,
            encoder,
            &view,
            None,
            viewport,
            image,
            background,
            region,
            load,
            group,
            Some(channel),
        )
    }
    fn check_output(
        &self,
        renderer: &Renderer,
        encoder: &AllocationEncoder,
        output: &wgpu::Texture,
    ) -> Result<(), RenderError> {
        if !std::sync::Arc::ptr_eq(&self.owner, &renderer.allocation_owner) {
            return Err(RenderError::Invalid(
                "managed display presenter belongs to another renderer".into(),
            ));
        }
        encoder.check_owner(&self.owner)?;
        if output.format() != self.presenter.format
            || output.sample_count() != 1
            || output.dimension() != wgpu::TextureDimension::D2
            || output.depth_or_array_layers() != 1
            || !output
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(RenderError::Invalid("managed display surface format, dimensions or usage differs from its prepared attachment".into()));
        }
        Ok(())
    }
    pub fn collect_texture_resources(&self, ledger: &mut TextureResourceLedger) {
        ledger.merge(self.resources.clone());
    }
}
