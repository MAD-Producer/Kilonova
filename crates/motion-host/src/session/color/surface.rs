//! Cold, immutable source/program ownership and two complete display groups.
//! No surface acquisition, provider IO or shader preparation occurs in render.
use super::*;
use crate::session::viewer::Key;
use motion_render::{
    AllocationEncoder, AuthoredSceneFrame, ManagedDisplayPresenter, ManagedDisplayTarget,
    RenderError, ResourceBudgetScope, ResourceRejection, TextureResourceLedger,
};
use std::sync::atomic::AtomicBool;

pub(crate) fn refuse(phase: &'static str, reason: &'static str) -> RenderError {
    motion_render::AuthoredSceneError::Admission {
        phase,
        object: None,
        reason,
    }
    .into()
}
fn budget(required_bytes: u64, budget_bytes: u64, message: &'static str) -> RenderError {
    RenderError::Resource {
        rejection: ResourceRejection::Budget {
            scope: ResourceBudgetScope::PreviewTarget,
            required_bytes,
            budget_bytes,
        },
        message: message.into(),
    }
}

pub(crate) fn qualify(
    config: &wgpu::SurfaceConfiguration,
    caps: &wgpu::SurfaceCapabilities,
) -> std::result::Result<(), RenderError> {
    if config.width == 0
        || config.height == 0
        || !caps.usages.contains(config.usage)
        || !matches!(
            config.format,
            wgpu::TextureFormat::Rgba8Unorm
                | wgpu::TextureFormat::Rgba8UnormSrgb
                | wgpu::TextureFormat::Bgra8Unorm
                | wgpu::TextureFormat::Bgra8UnormSrgb
        )
        || !caps
            .color_spaces(config.format)
            .contains(wgpu::SurfaceColorSpaces::SRGB)
        || !caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque)
        || !config
            .usage
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        return Err(refuse(
            "surface",
            "managed window requires an explicitly supported opaque sRGB SDR8 surface",
        ));
    }
    Ok(())
}

fn sources(project: &Project) -> Vec<motion_core::Asset> {
    project
        .assets
        .iter()
        .filter(|asset| {
            project.layers.iter().any(|layer|
        matches!(layer.content, motion_core::Content::Image { asset: id } if id == asset.id)
    )
        })
        .cloned()
        .collect()
}

struct Cell {
    program: [u8; 32],
    working: ColorImageTarget,
    display: ManagedDisplayTarget,
    presenter: ManagedDisplayPresenter,
}
impl Cell {
    fn bytes(&self) -> u64 {
        u64::from(self.display.dimensions()[0]) * u64::from(self.display.dimensions()[1]) * 32
    }
    fn collect(&self, ledger: &mut TextureResourceLedger) {
        ledger.merge(self.working.texture_resource_ledger());
        self.display.collect_texture_resources(ledger);
        self.presenter.collect_texture_resources(ledger);
    }
}

pub(crate) struct State {
    images: AuthoredImageSet,
    settings: Arc<motion_core::color_management::ProjectColorManagement>,
    sources: Vec<motion_core::Asset>,
    program: [u8; 32],
    prototype: Cell,
    front: Option<Vec<Cell>>,
    candidate: Option<Vec<Cell>>,
    published: Option<Key>,
    ready: Option<Key>,
    failure: Option<(Key, Arc<RenderError>)>,
}
impl State {
    pub(crate) fn prepare(
        prepared: &PreparedNativeAuthoredProject,
        project: &Project,
        root: &std::path::Path,
        renderer: &mut Renderer,
        format: wgpu::TextureFormat,
        request: u64,
        reserved: u64,
    ) -> Result<Self> {
        let observation = Observation::new(request);
        color_preparation::observed_operation(&observation, "prepare_surface_display", || {
            (|| -> Result<Self> {
                let program = prepared.colors.surface_display().ok_or_else(|| {
                    refuse(
                        "surface_display",
                        "project display declaration does not match the sRGB window endpoint",
                    )
                })?;
                if renderer.working_precision()
                    != motion_effects::image_contract::ChannelPrecision::Float32
                {
                    return Err(refuse(
                        "surface_display",
                        "managed window preparation requires Float32 working precision",
                    )
                    .into());
                }
                let required = reserved.saturating_add(32);
                if renderer.memory_policy().preview_target_budget_bytes < required {
                    return Err(budget(
                        required,
                        renderer.memory_policy().preview_target_budget_bytes,
                        "managed display prototype exceeds preview budget",
                    )
                    .into());
                }
                let images = prepared.prepare_images(
                    renderer,
                    project,
                    root,
                    &AtomicBool::new(false),
                    &Observation::new(request),
                )?;
                let working = renderer.color_image_target(1, 1, prepared.authored.contract())?;
                let display = ManagedDisplayTarget::new(renderer, &working, program)?;
                let presenter = ManagedDisplayPresenter::new(renderer, &display, format)?;
                Ok(Self {
                    images,
                    settings: prepared.colors.settings_owner().clone(),
                    sources: sources(project),
                    program: program.key(),
                    prototype: Cell {
                        working,
                        display,
                        presenter,
                        program: program.key(),
                    },
                    front: None,
                    candidate: None,
                    published: None,
                    ready: None,
                    failure: None,
                })
            })()
            .map_err(|source| color_preparation::Error::SurfaceDisplay(Box::new(source)))
        })
        .map_err(crate::Error::from)
    }
    pub(crate) fn matches(
        &self,
        prepared: &PreparedNativeAuthoredProject,
        project: &Project,
    ) -> bool {
        self.program_matches(prepared) && sources(project) == self.sources
    }
    pub(crate) fn program_matches(&self, prepared: &PreparedNativeAuthoredProject) -> bool {
        // Converted sources depend on input assignments as well as the display.
        // The retained immutable owner gives a constant-time hot-path check;
        // an equal display key alone cannot authorize old working-space pixels.
        Arc::ptr_eq(&self.settings, prepared.colors.settings_owner())
            && prepared
                .colors
                .surface_display()
                .is_some_and(|program| program.key() == self.program)
            && prepared.authored.contract() == self.prototype.working.contract()
    }
    pub(crate) fn bytes(&self) -> u64 {
        self.prototype.bytes()
            + self
                .front
                .iter()
                .flatten()
                .chain(self.candidate.iter().flatten())
                .map(Cell::bytes)
                .sum::<u64>()
    }
    pub(crate) fn collect(&self, ledger: &mut TextureResourceLedger) {
        ledger.merge(self.images.texture_resource_ledger());
        self.prototype.collect(ledger);
        for cell in self
            .front
            .iter()
            .flatten()
            .chain(self.candidate.iter().flatten())
        {
            cell.collect(ledger);
        }
    }
    pub(crate) fn clear_group(&mut self) {
        self.front = None;
        self.candidate = None;
        self.published = None;
        self.ready = None;
        self.failure = None;
    }
    pub(crate) fn discard_candidate(&mut self) {
        self.candidate = None;
        self.ready = None;
        self.failure = None;
    }
    pub(crate) fn inherit_front(&mut self, previous: &mut Self) {
        self.front = previous.front.take();
        self.published = previous.published.take();
    }
    pub(crate) fn published(&self) -> Option<&Key> {
        self.published.as_ref()
    }
    pub(crate) fn validate_comparisons(&self,renderer:&Renderer,inspections:&[motion_render::multiview::ViewInspection<'_>])
        ->std::result::Result<(),RenderError> {
        for inspection in inspections {
            if let Some((snapshot,_))=inspection.managed_comparison {
                self.prototype.presenter.check_snapshot(renderer,snapshot)?;
            }
        }
        Ok(())
    }
    pub(crate) fn snapshot(&self,renderer:&Renderer,index:usize)
        ->std::result::Result<motion_render::ManagedDisplaySnapshot,RenderError> {
        let cell=self.front.as_ref().filter(|_|self.published.is_some())
            .and_then(|cells|cells.get(index))
            .ok_or_else(||refuse("surface_snapshot","completed cell is unavailable"))?;
        cell.presenter.capture_snapshot(renderer,&cell.display)
    }
    pub(crate) fn capture_presenters(
        &self,
        renderer: &Renderer,
        format: wgpu::TextureFormat,
    ) -> std::result::Result<Vec<ManagedDisplayPresenter>, RenderError> {
        let cells = self
            .front
            .as_ref()
            .filter(|_| self.published.is_some())
            .ok_or_else(|| refuse("surface_capture", "complete display group is unavailable"))?;
        let first = cells
            .first()
            .ok_or_else(|| refuse("surface_capture", "complete display group is empty"))?;
        // Capture is an explicit cold command. Its attachment may differ from
        // the native surface; prepare one qualified pipeline and independent
        // per-cell uniforms, then retain them through the caller's submission.
        let prototype = ManagedDisplayPresenter::new(renderer, &first.display, format)?;
        let mut presenters = Vec::with_capacity(cells.len());
        for cell in cells.iter().skip(1) {
            presenters.push(prototype.rebind(renderer, &cell.display)?);
        }
        presenters.insert(0, prototype);
        Ok(presenters)
    }
    pub(crate) fn ready(&self, key: &Key) -> bool {
        self.ready.as_ref() == Some(key)
    }
    pub(crate) fn check_failure(&self, key: &Key) -> std::result::Result<(), RenderError> {
        if let Some((failed, cause)) = &self.failure {
            if failed == key {
                return Err(RenderError::Shared(cause.clone()));
            }
        }
        Ok(())
    }
    fn prepare_candidate(
        &mut self,
        renderer: &Renderer,
        sizes: &[[u32; 2]],
        reserved: u64,
    ) -> std::result::Result<(), RenderError> {
        if !matches!(sizes.len(), 1 | 2 | 4) || sizes.iter().any(|size| size.contains(&0)) {
            return Err(refuse(
                "surface_prepare",
                "expected one, two or four positive target extents",
            ));
        }
        let bytes = sizes.iter().try_fold(0u64, |sum, size| {
            u64::from(size[0])
                .checked_mul(u64::from(size[1]))
                .and_then(|v| v.checked_mul(32))
                .and_then(|v| sum.checked_add(v))
                .ok_or_else(|| {
                    refuse(
                        "surface_prepare",
                        "display group extent arithmetic overflow",
                    )
                })
        })?;
        let required = bytes
            .saturating_add(reserved)
            .saturating_add(self.prototype.bytes())
            .saturating_add(self.front.iter().flatten().map(Cell::bytes).sum::<u64>());
        let limit = renderer.memory_policy().preview_target_budget_bytes;
        if required > limit {
            return Err(budget(
                required,
                limit,
                "managed retained and candidate view groups exceed preview budget",
            ));
        }
        self.ready = None;
        if self.candidate.as_ref().is_some_and(|cells| {
            cells.len() == sizes.len()
                && cells.iter().zip(sizes).all(|(cell, size)| {
                    cell.display.dimensions() == *size
                        && cell.program == self.program
                        && cell.working.contract() == self.prototype.working.contract()
                })
        }) {
            return Ok(());
        }
        self.candidate = None;
        let cells = sizes
            .iter()
            .map(|[width, height]| {
                let working = renderer.color_image_target(
                    *width,
                    *height,
                    self.prototype.working.contract(),
                )?;
                let display = self.prototype.display.rebind(renderer, &working)?;
                let presenter = self.prototype.presenter.rebind(renderer, &display)?;
                Ok(Cell {
                    working,
                    display,
                    presenter,
                    program: self.program,
                })
            })
            .collect::<std::result::Result<Vec<_>, RenderError>>()?;
        self.candidate = Some(cells);
        Ok(())
    }
    pub(crate) fn render(
        &mut self,
        renderer: &mut Renderer,
        key: Key,
        frames: &[AuthoredSceneFrame],
        sizes: &[[u32; 2]],
        reserved: u64,
    ) -> std::result::Result<(), RenderError> {
        self.check_failure(&key)?;
        if self.ready(&key) {
            return Ok(());
        }
        let result = (|| {
            if frames.len() != sizes.len() {
                return Err(refuse("surface_render", "view and extent counts differ"));
            }
            self.prepare_candidate(renderer, sizes, reserved)?;
            let cells = self
                .candidate
                .as_ref()
                .ok_or_else(|| refuse("surface_render", "candidate group missing"))?;
            for (frame, cell) in frames.iter().zip(cells) {
                renderer.draw_authored_preview(frame, &cell.working, Some(&self.images))?;
            }
            let mut encoder =
                renderer.allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("managed display group conversion"),
                })?;
            for cell in cells {
                cell.display.encode(renderer, &mut encoder)?;
            }
            renderer.submit_allocated(encoder.finish(), |ledger| self.collect(ledger))?;
            renderer.check_health()?;
            self.ready = Some(key.clone());
            self.failure = None;
            Ok(())
        })();
        result.map_err(|error| {
            self.ready = None;
            let cause = Arc::new(error);
            self.failure = Some((key, cause.clone()));
            RenderError::Shared(cause)
        })
    }
    pub(crate) fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut AllocationEncoder,
        output: &wgpu::Texture,
        placements: &[motion_render::multiview::ViewPlacement],
        inspections: &[motion_render::multiview::ViewInspection<'_>],
        candidate: bool,
    ) -> std::result::Result<(), RenderError> {
        let cells = if candidate {
            self.candidate.as_ref().filter(|_| self.ready.is_some())
        } else {
            self.front.as_ref().filter(|_| self.published.is_some())
        }
        .ok_or_else(|| refuse("surface_present", "complete display group is unavailable"))?;
        if cells.len() != placements.len() || cells.len() != inspections.len() {
            return Err(refuse("surface_present", "layout and group counts differ"));
        }
        for (index, ((cell, placement), inspection)) in cells.iter().zip(placements).zip(inspections).enumerate() {
            cell.presenter.encode_compared_region_view_load(
                renderer,
                encoder,
                output,
                placement.viewport,
                placement.image,
                [0., 0., 0., 1.],
                inspection.channel,
                inspection.region,
                inspection.managed_comparison.map(|(source,split)|(source,index,split)),
                index != 0,
            )?;
        }
        Ok(())
    }
    /// Invoke only after the complete surface submission and health check.
    pub(crate) fn commit(&mut self) -> std::result::Result<(), RenderError> {
        if self.ready.is_none() || self.candidate.is_none() {
            return Err(refuse(
                "surface_commit",
                "candidate group was not completed",
            ));
        }
        std::mem::swap(&mut self.front, &mut self.candidate);
        self.published = self.ready.take();
        Ok(())
    }
}
