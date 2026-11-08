//! Prepared document ownership, cold surface installation and explicit consumers.
//! Legacy consumers require their own integration, never colour relabelling.
use super::*;
use crate::color_preparation::{self, Observation, PreparedNativeAuthoredProject};
use motion_core::color_management::ProjectColorManagement;
use motion_render::{AuthoredImageSet, ColorImageTarget, RenderStats};
pub(crate) mod surface;

pub(crate) fn prepare_surface_change(
    graphics: &mut Graphics,
    prepared: Option<&PreparedNativeAuthoredProject>,
    project: &Project,
    root: &std::path::Path,
    request: u64,
) -> Result<Option<Option<surface::State>>> {
    let Some(prepared) = prepared else {
        return Ok(graphics.views.colors.is_some().then_some(None));
    };
    if graphics
        .views
        .colors
        .as_ref()
        .is_some_and(|state| state.matches(prepared, project))
    {
        return Ok(None);
    }
    surface::qualify(
        &graphics.config,
        &graphics
            .surface
            .get_capabilities(&graphics.renderer.adapter),
    )?;
    if graphics.config.color_space != wgpu::SurfaceColorSpace::Srgb
        || graphics.config.alpha_mode != wgpu::CompositeAlphaMode::Opaque
    {
        return Err(surface::refuse(
            "surface_prepare",
            "reattach the surface to install an explicit opaque sRGB endpoint",
        )
        .into());
    }
    let reserved = graphics.scratch.texture_bytes() + graphics.views.texture_bytes();
    surface::State::prepare(
        prepared,
        project,
        root,
        &mut graphics.renderer,
        graphics.config.format,
        request,
        reserved,
    )
    .map(|state| Some(Some(state)))
}

pub(crate) fn install_surface_change(graphics: &mut Graphics, change: Option<surface::State>) {
    let mut change = change;
    if let (Some(next), Some(previous)) = (change.as_mut(), graphics.views.colors.as_mut()) {
        next.inherit_front(previous);
    }
    graphics.views.colors = change;
}

pub(super) fn prepare(
    project: &Project,
    root: &std::path::Path,
    platform: &dyn Platform,
    phase: &'static str,
    request: u64,
) -> Result<Option<PreparedNativeAuthoredProject>> {
    if matches!(
        project.color_management.as_ref(),
        ProjectColorManagement::Unmanaged {}
    ) {
        return Ok(None);
    }
    let Some(manifest) = platform.color_runtime_manifest()? else {
        project
            .color_management
            .require_unmanaged(phase)
            .map_err(motion_core::Error::from)?;
        return Err("managed color preparation did not produce a runtime".into());
    };
    color_preparation::prepare_native_authored_project(
        project,
        root,
        &manifest,
        &Observation::new(request),
    )
    .map_err(crate::Error::from)
}

pub(super) fn initial(
    prepared: &mut PreparedNativeAuthoredProject,
    project: &Project,
) -> Result<(Scene, Option<String>)> {
    let result = motion_core::prepare_expression_sources(project)
        .map_err(crate::Error::from)
        .and_then(|()| sample(prepared, project, 0.));
    match result {
        Ok(scene) => Ok((scene, None)),
        Err(error)
            if matches!(
                &error,
                crate::Error::Model(motion_core::Error::Expression { .. })
                    | crate::Error::Render(motion_render::RenderError::AuthoredScene(
                        motion_render::AuthoredSceneError::Model {
                            source: motion_core::Error::Expression { .. },
                            ..
                        }
                    ))
            ) =>
        {
            let mut base = project.clone();
            for expression in &mut base.expressions {
                expression.enabled = false;
            }
            Ok((sample(prepared, &base, 0.)?, Some(error.to_string())))
        }
        Err(error) => Err(error),
    }
}

pub(super) fn require_surface(prepared: &Option<PreparedNativeAuthoredProject>) -> Result<()> {
    if prepared.is_some() {
        return Err(motion_render::RenderError::from(
            motion_render::AuthoredSceneError::Admission {
                phase: "surface",
                object: None,
                reason: "this consumer requires its explicit managed color integration",
            },
        )
        .into());
    }
    Ok(())
}

pub(super) fn sample(
    prepared: &mut PreparedNativeAuthoredProject,
    project: &Project,
    frame: f64,
) -> Result<Scene> {
    Ok(prepared
        .authored
        .sample(project, frame, &motion_core::ExpressionEvaluator)
        .map_err(motion_render::RenderError::from)?
        .scene()
        .clone())
}

/// None means reuse the installed owner. Some(None) explicitly clears managed
/// state; Some(Some(_)) installs a completely prepared replacement after commit.
pub(crate) fn prepare_edit(
    current: Option<&PreparedNativeAuthoredProject>,
    project: &Project,
    root: &std::path::Path,
    platform: &dyn Platform,
    request: u64,
) -> Result<Option<Option<PreparedNativeAuthoredProject>>> {
    if matches!(
        project.color_management.as_ref(),
        ProjectColorManagement::Unmanaged {}
    ) {
        return Ok(current.is_some().then_some(None));
    }
    if let Some(current) = current {
        if project.color_management.as_ref() == current.colors.settings()
            && project.assets.as_slice() == current.authored.source_declarations()
        {
            current
                .authored
                .validate_project(project)
                .map_err(motion_render::RenderError::from)?;
            return Ok(None);
        }
    }
    Ok(Some(prepare(
        project,
        root,
        platform,
        "edit_prepare",
        request,
    )?))
}

impl Session {
    pub(crate) fn prepare_color_history(&mut self, operation: i32, active: &str) -> Result<()> {
        let mut candidate = None;
        let mut candidate_surface = None;
        let mut candidate_scene = None;
        let mut primary = None;
        let current = self.colors.as_ref();
        let platform = self.platform.clone();
        let root = &self.root;
        let request = self.document_generation;
        let graphics = &mut self.graphics;
        let prepare = |project: &Project| {
            if current.is_none()
                && matches!(
                    project.color_management.as_ref(),
                    ProjectColorManagement::Unmanaged {}
                )
            {
                return Ok(());
            }
            // Prepare the exact active composition before history changes.
            let mut destination = project.clone();
            let target = if destination.composition_ids().contains(&active.to_owned()) {
                active
            } else {
                motion_core::MAIN_COMPOSITION
            };
            destination.activate_composition(target)?;
            prepare_edit(current, &destination, root, platform.as_ref(), request)
                .and_then(|prepared| {
                    if let Some(graphics) = graphics.as_mut() {
                        let colors = match prepared.as_ref() {
                            Some(colors) => colors.as_ref(),
                            None => current,
                        };
                        candidate_surface =
                            prepare_surface_change(graphics, colors, &destination, root, request)?;
                    }
                    if prepared.as_ref().is_some_and(Option::is_none) {
                        candidate_scene = Some(Scene::new(&destination)?);
                    }
                    candidate = prepared;
                    Ok(())
                })
                .map_err(|error| {
                    let bridge = motion_core::Error::Invalid(error.to_string());
                    primary = Some(error);
                    bridge
                })
        };
        use crate::ops::editing::{HISTORY_CANCEL, HISTORY_REDO, HISTORY_UNDO};
        let result = match operation {
            HISTORY_UNDO => self.engine.undo_with_prepare(prepare).map(|_| ()),
            HISTORY_REDO => self.engine.redo_with_prepare(prepare).map(|_| ()),
            HISTORY_CANCEL => self.engine.end_gesture_with_prepare(false, prepare),
            _ => return Err("invalid color history operation".into()),
        };
        result.map_err(|error| primary.unwrap_or_else(|| error.into()))?;
        if let Some(mut scene) = candidate_scene {
            scene.set_model_source_catalog(self.scene.model_source_catalog());
            self.scene = scene;
        }
        if let Some(colors) = candidate {
            self.install_colors(colors);
        }
        if let (Some(graphics), Some(change)) = (&mut self.graphics, candidate_surface) {
            install_surface_change(graphics, change);
        }
        Ok(())
    }

    pub(crate) fn install_colors(&mut self, colors: Option<PreparedNativeAuthoredProject>) {
        self.colors = colors;
        self.pipeline_preparation.cancel();
        self.last_presented_frame = None;
        self.view_revision += 1;
        if let Some(graphics) = &mut self.graphics {
            graphics.scratch.invalidate();
            graphics.views.invalidate_content();
            graphics.actual_preview_quality = None;
        }
    }

    /// Immutable programs for explicit working/display/output consumers. This
    /// accessor does not authorize the legacy surface or export path.
    pub fn color_programs(&self) -> Option<&color_preparation::PreparedProjectColors> {
        self.colors.as_ref().map(|prepared| &prepared.colors)
    }

    /// Cold non-Present source preparation. The caller retains these bindings
    /// for this renderer and passes them to draw_color_frame; no per-frame IO.
    pub fn prepare_color_sources(
        &self,
        renderer: &mut Renderer,
        cancel: &std::sync::atomic::AtomicBool,
        observation: &Observation,
    ) -> Result<AuthoredImageSet> {
        self.check_thread()?;
        self.check_color_renderer(renderer)?;
        self.colors
            .as_ref()
            .ok_or("project has no prepared managed color programs")?
            .prepare_images(
                renderer,
                self.engine.project(),
                &self.root,
                cancel,
                observation,
            )
            .map_err(crate::Error::from)
    }

    /// Sample the ordinary document and draw through its actual managed path.
    /// Explicit target/source ownership and complete color contracts are checked
    /// by the renderer before writes. Errors remain errors, never retained frames.
    pub fn draw_color_frame(
        &mut self,
        renderer: &mut Renderer,
        target: &ColorImageTarget,
        images: &AuthoredImageSet,
    ) -> Result<RenderStats> {
        self.check_thread()?;
        self.check_color_renderer(renderer)?;
        self.sample()?;
        let frame = self
            .colors
            .as_ref()
            .and_then(|prepared| prepared.authored.current())
            .ok_or("project has no sampled managed color frame")?;
        renderer
            .draw_authored_scene_with_images(frame, target, images)
            .map_err(crate::Error::from)
    }

    fn check_color_renderer(&self, renderer: &Renderer) -> Result<()> {
        if !self
            .allocation_budget()
            .same_budget(renderer.allocation_budget())
        {
            return Err(
                "managed renderer was not constructed with the session allocation budget".into(),
            );
        }
        Ok(())
    }
}
