//! Bounded, transactional outputs for one, two, or four views of one frame.
//! The host owns media/exposure admission and acquires its surface only after
//! every cell is ready. No UI, source IO, polling loop, or clock lives here.
use crate::{
    AllocationEncoder, Presenter, RenderError, RenderTarget, Renderer, ResourceBudgetScope,
    SdrPreviewFrameTargets, TextureResourceLedger,
};

#[derive(Default)]
pub struct ViewFrameTargets {
    slots: Vec<SdrPreviewFrameTargets>,
    visible: usize,
}

impl ViewFrameTargets {
    pub fn len(&self) -> usize {
        self.visible
    }
    pub fn is_empty(&self) -> bool {
        self.visible == 0
    }
    /// Last fully submitted group, never a mixture of candidate and old views.
    pub fn output(&self, index: usize) -> Option<&RenderTarget> {
        (index < self.visible).then(|| &*self.slots[index])
    }
    pub fn working_output(&self, index: usize) -> Option<&RenderTarget> {
        (index < self.visible)
            .then(|| self.slots[index].working_front())
            .flatten()
    }
    pub fn texture_bytes(&self) -> u64 {
        self.slots
            .iter()
            .map(SdrPreviewFrameTargets::texture_bytes)
            .sum()
    }
    pub fn collect_texture_resources(&self, ledger: &mut TextureResourceLedger) {
        for slot in &self.slots {
            slot.collect_texture_resources(ledger);
        }
    }
    /// Release target ownership on surface detach or explicit memory eviction.
    /// Submitted native commands retain their own allocation leases.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.visible = 0;
    }
    fn discard_candidates(&mut self) {
        for slot in &mut self.slots {
            slot.discard_candidate();
        }
        self.slots.truncate(self.visible);
    }
    fn prepare(
        &mut self,
        renderer: &Renderer,
        sizes: &[[u32; 2]],
        reserved: u64,
    ) -> Result<(), RenderError> {
        view_count(sizes.len())?;
        renderer.check_health()?;
        // Admit the complete group, including old fronts, old layout slots and
        // placeholder fronts for newly created slots. Individual Sdr targets'
        // per-slot checks cannot enforce this combined allowance.
        let mut required: u64 = self
            .slots
            .iter()
            .map(|slot| slot.retained_bytes())
            .sum::<u64>()
            .saturating_add(reserved);
        for (index, size) in sizes.iter().enumerate() {
            let bytes = SdrPreviewFrameTargets::estimated_slot_bytes(renderer, *size)?;
            required = required.saturating_add(bytes);
            if index >= self.slots.len() {
                required = required.saturating_add(bytes);
            }
        }
        let budget = renderer.memory_policy().preview_target_budget_bytes;
        if required > budget {
            return Err(RenderError::budget(
                ResourceBudgetScope::PreviewTarget,
                required,
                budget,
                "multiview retained and candidate targets exceed combined preview budget",
            ));
        }
        // Remove every obsolete spare before allocating any replacement; a
        // mixed grow/shrink update must not temporarily retain their old sizes.
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if sizes
                .get(index)
                .is_none_or(|size| !slot.candidate_matches(renderer, *size))
            {
                slot.discard_candidate();
            }
        }
        for (index, [width, height]) in sizes.iter().copied().enumerate() {
            if index == self.slots.len() {
                self.slots
                    .push(SdrPreviewFrameTargets::new(renderer, width, height)?);
            }
            self.slots[index].prepare(renderer, width, height)?;
        }
        Ok(())
    }

    /// Render each admitted cell, submit its native commands, then publish all
    /// fronts together. Separate submissions preserve the existing renderer's
    /// queued uniform/scratch semantics; they do not acquire a surface or wait.
    ///
    /// The callback encodes into the working target and returns false for normal
    /// pending media/exposure. No partial group becomes visible on false/Err.
    /// Callers retain primary errors and use their existing default error channel.
    pub fn update(
        &mut self,
        renderer: &mut Renderer,
        sizes: &[[u32; 2]],
        encode: impl FnMut(
            &mut Renderer,
            usize,
            &RenderTarget,
            &mut AllocationEncoder,
        ) -> Result<bool, RenderError>,
    ) -> Result<bool, RenderError> {
        self.update_with_reserved(renderer, sizes, 0, encode)
    }
    /// Other preview outputs retained by the same surface owner share this
    /// allowance. They remain owned by the caller and are never evicted here.
    pub fn update_with_reserved(
        &mut self,
        renderer: &mut Renderer,
        sizes: &[[u32; 2]],
        reserved: u64,
        mut encode: impl FnMut(
            &mut Renderer,
            usize,
            &RenderTarget,
            &mut AllocationEncoder,
        ) -> Result<bool, RenderError>,
    ) -> Result<bool, RenderError> {
        let result = (|| {
            self.prepare(renderer, sizes, reserved)?;
            for index in 0..sizes.len() {
                let mut encoder =
                    renderer.allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Motion Studio view candidate"),
                    })?;
                let target = self.slots[index].prepared().ok_or_else(|| {
                    RenderError::Invalid(format!(
                        "multiview view={index} phase=encode: candidate missing"
                    ))
                })?;
                if !encode(renderer, index, target, &mut encoder)? {
                    return Ok(false);
                }
                self.slots[index].encode_output(renderer, &mut encoder)?;
                renderer.submit_allocated(encoder.finish(), |_| {})?;
            }
            // All commits have the same checked precondition, with private slots
            // and no external mutation between validation and publication.
            if self.slots[..sizes.len()]
                .iter()
                .any(|slot| slot.prepared_output().is_none())
            {
                return Err(RenderError::Invalid(
                    "multiview phase=commit: display output missing".into(),
                ));
            }
            for slot in &mut self.slots[..sizes.len()] {
                slot.commit_submitted()?;
            }
            self.visible = sizes.len();
            self.slots.truncate(self.visible);
            Ok(true)
        })();
        if !matches!(result, Ok(true)) {
            self.discard_candidates();
        }
        result
    }
}

fn view_count(count: usize) -> Result<(), RenderError> {
    if matches!(count, 1 | 2 | 4) {
        Ok(())
    } else {
        Err(RenderError::Invalid(
            "multiview phase=prepare: expected 1, 2, or 4 views".into(),
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ViewPlacement {
    pub viewport: [f32; 4],
    pub image: Option<[f32; 4]>,
}

/// Independent source and uniform bindings per cell. All cells encode into one
/// caller-owned surface; only the first clears it. UI chrome can be drawn after.
#[derive(Default)]
pub struct ViewFramePresenter {
    presenters: Vec<Presenter>,
    format: Option<wgpu::TextureFormat>,
    owner: Option<std::sync::Arc<()>>,
}
impl ViewFramePresenter {
    pub fn clear(&mut self) {
        self.presenters.clear();
        self.format = None;
        self.owner = None;
    }
    pub fn encode(
        &mut self,
        renderer: &Renderer,
        frames: &ViewFrameTargets,
        encoder: &mut AllocationEncoder,
        output: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        surface: [u32; 2],
        placements: &[ViewPlacement],
        background: [f32; 4],
    ) -> Result<(), RenderError> {
        view_count(placements.len())?;
        if placements.len() != frames.len() {
            return Err(RenderError::Invalid(
                "multiview phase=present: layout and ready frame counts differ".into(),
            ));
        }
        if surface.contains(&0)
            || background.iter().any(|value| !value.is_finite())
            || surface != [output.texture().width(), output.texture().height()]
            || format != output.texture().format()
        {
            return Err(RenderError::Invalid(
                "multiview phase=present: invalid surface or background".into(),
            ));
        }
        encoder.check_owner(&renderer.allocation_owner)?;
        if self
            .owner
            .as_ref()
            .is_some_and(|owner| !std::sync::Arc::ptr_eq(owner, &renderer.allocation_owner))
        {
            return Err(RenderError::Invalid(
                "multiview phase=present: presenter belongs to another renderer".into(),
            ));
        }
        for (index, placement) in placements.iter().enumerate() {
            let [x, y, w, h] = placement.viewport;
            if !placement.viewport.iter().all(|v| v.is_finite())
                || x < 0.
                || y < 0.
                || w <= 0.
                || h <= 0.
                || x + w > surface[0] as f32
                || y + h > surface[1] as f32
            {
                return Err(RenderError::Invalid(format!(
                    "multiview view={index} phase=present: invalid viewport"
                )));
            }
            if placement.image.is_some_and(|image| {
                !image.iter().all(|v| v.is_finite()) || image[2] <= 0. || image[3] <= 0.
            }) {
                return Err(RenderError::Invalid(format!(
                    "multiview view={index} phase=present: invalid canvas image"
                )));
            }
            for previous in &placements[..index] {
                let [px, py, pw, ph] = previous.viewport;
                if x < px + pw && x + w > px && y < py + ph && y + h > py {
                    return Err(RenderError::Invalid(format!(
                        "multiview view={index} phase=present: overlapping viewports"
                    )));
                }
            }
            frames
                .output(index)
                .ok_or_else(|| RenderError::Invalid("multiview ready output missing".into()))?
                .check_owner(renderer)?;
        }
        if self.format != Some(format) {
            self.clear();
            self.format = Some(format);
        }
        self.owner = Some(renderer.allocation_owner.clone());
        for (index, placement) in placements.iter().enumerate() {
            let source = frames
                .output(index)
                .ok_or_else(|| RenderError::Invalid("multiview ready output missing".into()))?;
            if index == self.presenters.len() {
                self.presenters
                    .push(Presenter::new_admitted(renderer, &source.view, format)?);
            }
            let presenter = &mut self.presenters[index];
            presenter.set_admitted_source(&renderer.device, &source.view);
            presenter.encode_view_load(
                &renderer.queue,
                encoder,
                output,
                None,
                placement.viewport,
                placement.image,
                background,
                index != 0,
            )?;
        }
        self.presenters.truncate(placements.len());
        Ok(())
    }
}
