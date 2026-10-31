//! Viewer-only snapshots. They never modify the project or its edit history.
use super::{CompositionStamp, UiFrame};
use motion_host::session::Session;
use motion_render::{Presenter, Renderer, ViewChannel, ViewerSnapshot};
use serde_json::Value;
use std::{collections::HashMap, path::Path};

struct Snapshot {
    generation: u64,
    image: ViewerSnapshot,
}

#[derive(Default)]
pub(super) struct State {
    context: Option<(u64, u64, String)>,
    snapshots: HashMap<(u64, Option<usize>), Snapshot>,
    active_cells: HashMap<u64, Option<usize>>,
    next_generation: u64,
    bound: Option<(u64, u64, ViewChannel)>,
}

impl State {
    /// A snapshot from an old document, composition or GPU device is never reused.
    pub(super) fn reconcile(&mut self, session: &mut Session) {
        let context = (
            session.document_generation,
            session.surface_epoch,
            session.engine.project().composition_id.clone(),
        );
        if self.context.as_ref() != Some(&context) {
            if let Some(g) = session.graphics.as_mut() {
                g.presenter.clear_comparison_source();
            }
            self.snapshots.clear();
            self.active_cells.clear();
            self.bound = None;
            self.context = Some(context);
        }
    }

    pub(super) fn close_window(&mut self, session: &mut Session, window: u64) {
        if self.bound.is_some_and(|bound| bound.0 == window) {
            if let Some(g) = session.graphics.as_mut() {
                g.presenter.clear_comparison_source();
            }
            self.bound = None;
        }
        self.snapshots.retain(|(owner, _), _| *owner != window);
        self.active_cells.remove(&window);
    }

    pub(super) fn capture(
        &mut self,
        session: &mut Session,
        stamp: Option<&CompositionStamp>,
        window: u64,
    ) -> Result<Value, String> {
        self.reconcile(session);
        let cell = self.active_cells.get(&window).copied().flatten();
        let generation = self
            .next_generation
            .checked_add(1)
            .ok_or("viewer snapshot generation exhausted")?;
        let image = if let Some(index) = cell {
            session
                .capture_view_cell(window, index)
                .map_err(|error| error.to_string())?
        } else {
            let stamp = stamp.ok_or("no completed viewer frame is available")?;
            if stamp.revision != session.engine.revision()
                || stamp.view != session.view_revision
                || stamp.surface != session.surface_epoch
                || stamp.frame != session.frame.to_bits()
                || stamp.mode != session.preview.mode
                || stamp.tier != session.preview.tier()
                || stamp.quality
                    != session
                        .preview_memory
                        .quality(session.preview.mode == motion_render::PreviewMode::High)
                || session.preview_memory_held
                || session.last_error.is_some()
            {
                return Err("the completed viewer frame is no longer current".into());
            }
            let graphics = session
                .graphics
                .as_mut()
                .ok_or("desktop surface is not attached")?;
            let source = graphics
                .scratch
                .working_front()
                .ok_or("the working viewer frame is unavailable")?;
            ViewerSnapshot::capture(&graphics.renderer, source)
                .map_err(|error| error.to_string())?
        };
        let width = image.native().width;
        let height = image.native().height;
        if self.bound.is_some_and(|bound| bound.0 == window) {
            if let Some(graphics) = session.graphics.as_mut() {
                graphics.presenter.clear_comparison_source();
            }
            self.bound = None;
        }
        self.snapshots
            .insert((window, cell), Snapshot { generation, image });
        self.next_generation = generation;
        Ok(
            serde_json::json!({"window": window, "cell": cell, "generation": generation, "width": width, "height": height}),
        )
    }

    fn source(
        &self,
        ui: &UiFrame,
    ) -> Result<Option<(&Snapshot, &motion_render::RenderTarget)>, String> {
        if ui.inspection.comparison_split.is_none() {
            return Ok(None);
        }
        let snapshot = self
            .snapshots
            .get(&(ui.window, None))
            .ok_or("viewer comparison snapshot is unavailable")?;
        let target = if ui.inspection.channel == ViewChannel::Color {
            snapshot.image.display()
        } else {
            snapshot.image.native()
        };
        Ok(Some((snapshot, target)))
    }

    pub(super) fn configure(&mut self, session: &mut Session, ui: &UiFrame) -> Result<(), String> {
        self.reconcile(session);
        let selection = if ui.composition.is_some() && ui.views.is_none() {
            self.source(ui)?
        } else {
            None
        };
        let desired = selection
            .as_ref()
            .map(|(snapshot, _)| (ui.window, snapshot.generation, ui.inspection.channel));
        let graphics = session
            .graphics
            .as_mut()
            .ok_or("desktop surface is not attached")?;
        if desired != self.bound {
            if let Some((_, target)) = selection {
                graphics
                    .presenter
                    .set_comparison_source(&graphics.renderer.device, target);
            } else {
                graphics.presenter.clear_comparison_source();
            }
            self.bound = desired;
        }
        graphics.presenter.set_inspection_geometry(
            if ui.composition.is_some() {
                ui.inspection.region
            } else {
                None
            },
            if ui.composition.is_some() {
                ui.inspection.comparison_split
            } else {
                None
            },
        );
        Ok(())
    }

    pub(super) fn configure_capture(
        &self,
        ui: &UiFrame,
        renderer: &Renderer,
        presenter: &mut Presenter,
    ) -> Result<(), String> {
        if ui.composition.is_some() {
            if let Some((_, target)) = self.source(ui)? {
                presenter.set_comparison_source(&renderer.device, target);
            }
            presenter.set_inspection_geometry(ui.inspection.region, ui.inspection.comparison_split);
        }
        Ok(())
    }

    pub(super) fn did_present(&mut self, ui: &UiFrame) {
        if ui.composition.is_some() {
            self.active_cells
                .insert(ui.window, ui.views.as_ref().map(|plan| plan.active));
        }
    }

    pub(super) fn views<'a>(
        &'a self,
        ui: &UiFrame,
    ) -> Result<Vec<motion_render::multiview::ViewInspection<'a>>, String> {
        let Some(plan) = &ui.views else {
            return Ok(vec![]);
        };
        plan.cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let settings = cell.inspection;
                let comparison = if let Some(split) = settings.comparison_split {
                    let snapshot =
                        self.snapshots
                            .get(&(ui.window, Some(index)))
                            .ok_or_else(|| {
                                format!(
                                "viewer comparison window={} cell={index}: snapshot is unavailable",
                                ui.window
                            )
                            })?;
                    let source = if settings.channel == ViewChannel::Color {
                        snapshot.image.display()
                    } else {
                        snapshot.image.native()
                    };
                    Some((source, split))
                } else {
                    None
                };
                Ok(motion_render::multiview::ViewInspection {
                    channel: settings.channel,
                    region: settings.region,
                    comparison,
                })
            })
            .collect()
    }

    pub(super) fn save(
        &mut self,
        session: &mut Session,
        window: u64,
        path: &Path,
        channel: ViewChannel,
    ) -> Result<Value, String> {
        use motion_effects::{
            image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
            AlphaMode, WorkingSpace,
        };
        use motion_render::{ColorOutput, ColorOutputDescriptor, OutputTransform};
        self.reconcile(session);
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("png"))
        {
            return Err("phase=path: viewer snapshot requires a PNG file name".into());
        }
        let cell = self.active_cells.get(&window).copied().flatten();
        let snapshot = self
            .snapshots
            .get(&(window, cell))
            .ok_or("phase=source: viewer snapshot is unavailable")?;
        let source = snapshot.image.native();
        let graphics = session
            .graphics
            .as_ref()
            .ok_or("phase=surface: desktop surface is not attached")?;
        let renderer = &graphics.renderer;
        let target = renderer
            .image_capture_target(
                source.width,
                source.height,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Straight,
                    },
                    ChannelPrecision::Unorm8,
                ),
            )
            .map_err(|error| format!("phase=target: {error}"))?;
        let format = target.view().texture().format();
        if channel == ViewChannel::Color {
            let output = ColorOutput::for_render_target(
                renderer,
                source,
                ColorOutputDescriptor {
                    format,
                    alpha: AlphaMode::Straight,
                    transform: OutputTransform::SDR_CLIP,
                },
            )
            .map_err(|error| format!("phase=color_output: {error}"))?;
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .map_err(|error| format!("phase=encoder: {error}"))?;
            output
                .encode_image(renderer, &mut encoder, &target)
                .map_err(|error| format!("phase=encode: {error}"))?;
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    snapshot.image.collect_texture_resources(ledger);
                    ledger.merge(target.texture_resource_ledger());
                })
                .map_err(|error| format!("phase=submit: {error}"))?;
        } else {
            let mut presenter = Presenter::new_admitted(renderer, &source.view, format)
                .map_err(|error| format!("phase=presenter: {error}"))?;
            presenter
                .set_view_channel(&renderer.queue, channel)
                .map_err(|error| format!("phase=channel: {error}"))?;
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .map_err(|error| format!("phase=encoder: {error}"))?;
            presenter
                .encode_view(
                    &renderer.queue,
                    &mut encoder,
                    target.view(),
                    None,
                    [0., 0., source.width as f32, source.height as f32],
                    None,
                    [0., 0., 0., 0.],
                )
                .map_err(|error| format!("phase=encode: {error}"))?;
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    snapshot.image.collect_texture_resources(ledger);
                    ledger.merge(target.texture_resource_ledger());
                })
                .map_err(|error| format!("phase=submit: {error}"))?;
        }
        let pixels = renderer
            .read_image_target(&target)
            .map_err(|error| format!("phase=readback: {error}"))?
            .pixels;
        image::save_buffer_with_format(
            path,
            &pixels,
            source.width,
            source.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|error| format!("phase=save: {error}"))?;
        Ok(
            serde_json::json!({"window":window, "generation":snapshot.generation, "width":source.width, "height":source.height}),
        )
    }
}
