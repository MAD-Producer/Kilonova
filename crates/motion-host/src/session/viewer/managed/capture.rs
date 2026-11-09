//! Explicit display capture of a completed, exactly owned managed group.
use super::*;
use motion_effects::{
    AlphaMode, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::ManagedDisplayPresenter;

/// Retains the capture pipeline, independent uniforms and source allocations.
/// Keep this owner alive through submission and merge its resources into the
/// submitted command ledger. No captured image is published as a viewer frame.
pub struct ManagedViewCapture {
    presenters: Vec<ManagedDisplayPresenter>,
}
impl ManagedViewCapture {
    pub fn collect_texture_resources(&self, ledger: &mut TextureResourceLedger) {
        for presenter in &self.presenters {
            presenter.collect_texture_resources(ledger);
        }
    }
}

impl Session {
    /// Encode an explicit capture from the completed display group, without
    /// sampling, reading configuration, changing the shared clock or consuming
    /// the legacy scratch. The caller owns the target, submission and readback.
    pub fn encode_managed_view_capture(
        &self,
        owner: u64,
        views: &[View<'_>],
        inspection: &[ViewInspection<'_>],
        encoder: &mut motion_render::AllocationEncoder,
        output: &motion_render::ImageCaptureTarget,
        surface: [u32; 2],
    ) -> Result<ManagedViewCapture> {
        self.check_thread()?;
        let result: Result<ManagedViewCapture> = (|| {
            inspections(views, inspection)?;
            if self.colors.is_none() {
                return Err(refuse("surface_capture", "managed display owner is missing").into());
            }
            let key = self
                .views_key(owner, self.frame, views, Some(surface))?
                .ok_or_else(|| refuse("surface_capture", "graphics are detached"))?;
            if [output.width(), output.height()] != surface {
                return Err(
                    refuse("surface_capture", "capture extent differs from the request").into(),
                );
            }
            let g = self
                .graphics
                .as_ref()
                .ok_or_else(|| refuse("surface_capture", "graphics are detached"))?;
            output.check_owner(&g.renderer)?;
            if output.contract()
                != ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    ChannelPrecision::Unorm8,
                )
            {
                return Err(refuse(
                    "surface_capture",
                    "expected the qualified sRGB display capture contract",
                )
                .into());
            }
            let state =
                g.views.colors.as_ref().ok_or_else(|| {
                    refuse("surface_capture", "prepared display owner is missing")
                })?;
            if state.published() != Some(&key) {
                return Err(
                    refuse("surface_capture", "completed group is no longer current").into(),
                );
            }
            let presenters =
                state.capture_presenters(&g.renderer, output.view().texture().format())?;
            let placements = placements(&self.scene, views);
            if presenters.len() != placements.len() {
                return Err(
                    refuse("surface_capture", "layout and display group counts differ").into(),
                );
            }
            for (index, (presenter, placement)) in presenters.iter().zip(placements).enumerate() {
                presenter.encode_view_load(
                    &g.renderer,
                    encoder,
                    output.view().texture(),
                    placement.viewport,
                    placement.image,
                    [0., 0., 0., 1.],
                    index != 0,
                )?;
            }
            Ok(ManagedViewCapture { presenters })
        })();
        result.map_err(|cause| {
            let published=self.graphics.as_ref().and_then(|g|g.views.colors.as_ref())
                .and_then(|state|state.published())
                .map(|key|(key.document,key.revision,key.owner,f64::from_bits(key.frame)));
            crate::Error::with_context(format!(
                "managed capture phase=surface_capture document={} revision={} owner={owner} frame={} surface={surface:?} target={}x{} contract={:?} published={published:?}",
                self.document_generation,self.engine.revision(),self.frame,
                output.width(),output.height(),output.contract(),
            ),cause)
        })
    }
}
