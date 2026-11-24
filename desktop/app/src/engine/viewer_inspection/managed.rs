//! Typed managed snapshot selection and explicit display-code PNG save.
use super::*;

impl State {
    pub(in crate::engine) fn single(
        &self,
        ui: &UiFrame,
        phase: &'static str,
    ) -> Result<motion_render::multiview::ViewInspection<'_>, String> {
        self.inspected(ui.window, None, ui.inspection, phase)
    }
    pub(super) fn inspected(
        &self,
        window: u64,
        cell: Option<usize>,
        settings: super::super::ViewerInspectionFrame,
        phase: &'static str,
    ) -> Result<motion_render::multiview::ViewInspection<'_>, String> {
        let mut inspection = motion_render::multiview::ViewInspection {
            channel: settings.channel,
            region: settings.region,
            ..Default::default()
        };
        if let Some(split) = settings.comparison_split {
            let snapshot = self.snapshots.get(&(window, cell)).ok_or_else(|| {
                if let Some(index) = cell {
                    format!(
                        "viewer comparison window={window} cell={index}: snapshot is unavailable"
                    )
                } else {
                    format!(
                        "viewer comparison phase={phase} window={window}: snapshot is unavailable"
                    )
                }
            })?;
            match &snapshot.image {
                SnapshotImage::Legacy(image) => {
                    inspection.comparison = Some((
                        if settings.channel == ViewChannel::Color {
                            image.display()
                        } else {
                            image.native()
                        },
                        split,
                    ));
                }
                SnapshotImage::Managed(image) => {
                    inspection.managed_comparison = Some((image, split))
                }
            }
        }
        Ok(inspection)
    }
}

pub(super) fn save(
    session: &Session,
    window: u64,
    generation: u64,
    image: &motion_render::ManagedDisplaySnapshot,
    path: &Path,
    channel: ViewChannel,
) -> motion_host::Result<Value> {
    save_pixels(session, window, generation, image, path, channel).map_err(|cause| {
        motion_host::Error::with_context(
            format!("managed snapshot save window={window} generation={generation} channel={channel:?} dimensions={:?}", image.dimensions()),
            cause,
        )
    })
}

fn save_pixels(
    session: &Session,
    window: u64,
    generation: u64,
    image: &motion_render::ManagedDisplaySnapshot,
    path: &Path,
    channel: ViewChannel,
) -> motion_host::Result<Value> {
    use motion_effects::{
        image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
        AlphaMode, WorkingSpace,
    };
    let g = session
        .graphics
        .as_ref()
        .ok_or("phase=surface: desktop surface is not attached")?;
    let renderer = &g.renderer;
    let [width, height] = image.dimensions();
    let target = renderer.image_capture_target(
        width,
        height,
        ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            ChannelPrecision::Unorm8,
        ),
    )?;
    let prepared = image.for_format(renderer, target.view().texture().format())?;
    let mut encoder = renderer.allocation_command_encoder(&Default::default())?;
    prepared.encode(renderer, &mut encoder, target.view().texture(), channel)?;
    renderer.submit_allocated(encoder.finish(), |ledger| {
        prepared.collect_texture_resources(ledger);
        ledger.merge(target.texture_resource_ledger());
    })?;
    let pixels = renderer.read_image_target(&target)?.pixels;
    image::save_buffer_with_format(
        path,
        &pixels,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|cause| {
        motion_host::Error::with_context("phase=save managed display snapshot", cause)
    })?;
    Ok(
        serde_json::json!({"window":window,"generation":generation,"width":width,"height":height,"kind":"display"}),
    )
}
