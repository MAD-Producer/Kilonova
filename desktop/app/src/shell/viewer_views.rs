//! Bounded asynchronous projection and pick replies, owned by their native view.
use crate::*;
use crate::{
    engine::{
        viewer_frame::{Cell, Plan},
        viewer_input::{Query, Request},
    },
    panels::composition_views::Key,
};
use motion_ui::dock::Panel;
use std::sync::mpsc::{Receiver, TryRecvError};
#[cfg(all(test, target_os = "windows"))]
mod native_tests;

struct Pending {
    key: Key,
    receive: Receiver<Result<Value, String>>,
}
struct Pick {
    key: Key,
    cell: usize,
    receive: Receiver<Result<Value, String>>,
}
#[derive(Default)]
pub(crate) struct Queries {
    states: Option<Pending>,
    pick: Option<Pick>,
}
impl Queries {
    pub(crate) fn busy(&self) -> bool {
        self.states.is_some() || self.pick.is_some()
    }
}

impl Shell {
    pub(crate) fn projection_key(&self) -> Key {
        let mut key = self.panels.viewer_views.key(&self.state);
        key.window = self
            .floating
            .values()
            .find(|f| f.panel == Panel::Composition)
            .map_or(0, |f| f.key);
        key
    }
    pub(crate) fn poll_viewer_views(&mut self) {
        self.reconcile_view_cameras();
        let key = self.projection_key();
        if let Some(pending) = self.viewer_projection.states.take() {
            match pending.receive.try_recv() {
                Err(TryRecvError::Empty) => self.viewer_projection.states = Some(pending),
                result => {
                    let result = result
                        .unwrap_or_else(|_| Err("viewer projection worker disconnected".into()));
                    if pending.key == key {
                        match result {
                            Ok(reply) if reply["cancelled"] == true => {}
                            Ok(reply) => {
                                if let Some(states) = reply["views"].as_array().filter(|v| {
                                    v.len() == self.panels.viewer_views.settings.layout().count()
                                }) {
                                    self.panels.viewer_views.cache =
                                        Some((key.clone(), states.clone()));
                                    self.panels.viewer_views.failed = None;
                                } else {
                                    self.projection_failure(&key, "invalid projection group reply");
                                }
                            }
                            Err(error) => self.projection_failure(&key, &error),
                        }
                        self.dirty = true;
                    } else if let Err(error) = result {
                        // Superseded replies are normal, but an actual worker
                        // fault must retain its original cause even after a seek.
                        log::error!("viewer projection failed phase=stale_reply window={} frame={} reason={error}",pending.key.window,f64::from_bits(pending.key.frame));
                    }
                }
            }
        }
        if let Some(pending) = self.viewer_projection.pick.take() {
            match pending.receive.try_recv() {
                Err(TryRecvError::Empty) => self.viewer_projection.pick = Some(pending),
                result => {
                    let result =
                        result.unwrap_or_else(|_| Err("viewer pick worker disconnected".into()));
                    match result {
                        Ok(reply)
                            if pending.key == key
                                && pending.cell == self.panels.viewer_views.settings.active()
                                && reply["cancelled"] == false =>
                        {
                            self.panels
                                .select_only(crate::shell::actions::selected_candidate(
                                    &reply["pick"],
                                ));
                            self.dirty = true;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            log::error!(
                                "viewer pick failed phase=reply window={} view={} reason={error}",
                                pending.key.window,
                                pending.cell
                            );
                            self.panels.message = Some(self.workspace.locale.error(&error));
                            self.dirty = true;
                        }
                    }
                }
            }
        }
        let ready = self
            .panels
            .viewer_views
            .cache
            .as_ref()
            .is_some_and(|(cached, _)| *cached == key);
        // No window and the home screen need no projection work.
        if self.viewer_projection.states.is_none()
            && !ready
            && self.panels.viewer_views.failed.as_ref() != Some(&key)
            && self.playback.is_none()
            && self.editing.is_none()
            && self.window.is_some()
            && self.home.is_none()
            && self.state["project"]["width"].as_u64().is_some()
        {
            let canvas = [
                self.state["project"]["width"].as_u64().unwrap_or(0) as u32,
                self.state["project"]["height"].as_u64().unwrap_or(0) as u32,
            ];
            let cameras = (0..self.panels.viewer_views.settings.layout().count())
                .map(|i| self.panels.viewer_views.settings.camera(i, canvas))
                .collect::<Result<Vec<_>, _>>();
            let result = cameras.and_then(|cameras| {
                self.engine.viewer_input_async(Request {
                    window: key.window,
                    source: key.source.clone(),
                    frame: f64::from_bits(key.frame),
                    query: Query::States(cameras),
                })
            });
            match result {
                Ok(receive) => self.viewer_projection.states = Some(Pending { key, receive }),
                Err(error) => self.projection_failure(&key, &error),
            }
        }
    }
    // Camera deletion / a workspace opened with another project is a normal
    // source transition. Reconcile once before submitting new projection work.
    fn reconcile_view_cameras(&mut self) {
        let available = panels::composition_views::modes(&self.state, self.workspace.locale);
        let mut reset = false;
        for cell in 0..self.panels.viewer_views.settings.layout().count() {
            let view = self
                .panels
                .viewer_views
                .settings
                .view_mut(cell)
                .expect("validated view count");
            if matches!(view.mode, crate::viewer_views::Mode::Camera(_))
                && !available.iter().any(|(mode, _)| *mode == view.mode)
            {
                self.panels.viewer_views.history[cell].cancel(view);
                log::info!("viewer camera unavailable operation=viewer_reconcile phase=source_change view={cell} camera={:?}", view.mode);
                view.mode = crate::viewer_views::Mode::Composition;
                view.orbit = None;
                reset = true;
            }
        }
        if reset {
            self.panels.viewer_views.loaded = None;
            self.panels.message = Some(
                if self.workspace.locale == Locale::Zh {
                    "原视图的摄影机已不存在，已切换到合成视图"
                } else {
                    "A view camera is no longer available; using the composition view"
                }
                .into(),
            );
            self.dirty = true;
            self.present_due = true;
        }
    }
    fn persist_view_settings(&mut self) -> Result<(), String> {
        self.workspace.viewer_views = self.panels.viewer_views.settings.clone();
        self.workspace.save(&self.layout_path).map_err(|error| {
            log::error!("viewer preferences failed operation=save_view_settings phase=write path={} reason={error}",self.layout_path.display());
            error
        })
    }
    fn projection_failure(&mut self, key: &Key, error: &str) {
        log::error!(
            "viewer projection failed phase=states window={} frame={} reason={error}",
            key.window,
            f64::from_bits(key.frame)
        );
        self.panels.viewer_views.failed = Some(key.clone());
        self.panels.message = Some(self.workspace.locale.error(error));
    }
    pub(crate) fn pick_view(
        &mut self,
        key: Key,
        cell: usize,
        point: [f64; 2],
    ) -> Result<(), String> {
        self.stop();
        if key != self.projection_key() || cell != self.panels.viewer_views.settings.active() {
            return Ok(());
        }
        if self.viewer_projection.pick.is_some() {
            return Ok(());
        }
        let canvas = [
            self.state["project"]["width"].as_u64().unwrap_or(0) as u32,
            self.state["project"]["height"].as_u64().unwrap_or(0) as u32,
        ];
        let camera = self.panels.viewer_views.settings.camera(cell, canvas)?;
        let receive = self.engine.viewer_input_async(Request {
            window: key.window,
            source: key.source.clone(),
            frame: f64::from_bits(key.frame),
            query: Query::Pick { camera, point },
        })?;
        self.viewer_projection.pick = Some(Pick { key, cell, receive });
        Ok(())
    }
    pub(crate) fn viewer_plan(
        &self,
        viewport: motion_ui::Rect,
        scale: f32,
        styles: &motion_ui::style::StyleSheet,
        floating: bool,
    ) -> Result<Option<Plan>, String> {
        let canvas = [
            self.state["project"]["width"].as_u64().unwrap_or(0) as u32,
            self.state["project"]["height"].as_u64().unwrap_or(0) as u32,
        ];
        if canvas.contains(&0) || viewport.width() <= 0. || viewport.height() <= 0. {
            return Ok(None);
        }
        let settings = &self.panels.viewer_views.settings;
        let tiles = settings
            .layout()
            .tiles(viewport, styles.metric("viewer.multiview.gap", 2.))?;
        if tiles.is_empty() {
            return Ok(None);
        }
        let cells = tiles
            .iter()
            .enumerate()
            .map(|(i, tile)| {
                let image = settings
                    .view(i)
                    .ok_or("view navigation missing")?
                    .navigation
                    .mapping(*tile, canvas.map(f64::from))
                    .ok_or("invalid view navigation mapping")?
                    .image;
                let physical = |r: motion_ui::Rect| {
                    [
                        r.min[0] * scale,
                        r.min[1] * scale,
                        r.width() * scale,
                        r.height() * scale,
                    ]
                };
                Ok(Cell {
                    camera: settings.camera(i, canvas)?,
                    placement: motion_render::multiview::ViewPlacement {
                        viewport: physical(*tile),
                        image: Some(physical(image)),
                    },
                    inspection: self.panels.viewer_inspection.frame_for(
                        floating,
                        i,
                        self.panels.viewer_guides.channel_for(floating, i),
                    ),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Some(Plan {
            cells,
            active: settings.active(),
        }))
    }
    // The ordinary composition already has the complete single-view renderer,
    // including paced media/exposure and FrameRecorder attribution. Preserve that
    // path when its camera is identical; explicit observers and multiple cells
    // must continue through the independent group renderer.
    pub(crate) fn single_composition_frame(
        &self,
        plan: &mut Option<Plan>,
    ) -> (engine::ViewerInspectionFrame, Option<[f32; 4]>) {
        if self.state["observing"] != true
            && plan.as_ref().is_some_and(|p| {
                p.cells.len() == 1
                    && matches!(p.cells[0].camera, engine::viewer_frame::Camera::Composition)
            })
        {
            let cell = plan
                .take()
                .expect("checked single composition")
                .cells
                .remove(0);
            (cell.inspection, cell.placement.image)
        } else {
            (Default::default(), None)
        }
    }
    pub(crate) fn viewer_plan_for(
        &mut self,
        viewport: motion_ui::Rect,
        scale: f32,
        styles: &motion_ui::style::StyleSheet,
        floating: bool,
    ) -> Option<Plan> {
        match self.viewer_plan(viewport, scale, styles, floating) {
            Ok(plan) => plan,
            Err(error) => {
                let key = self.projection_key();
                if self.panels.viewer_views.failed.as_ref() != Some(&key) {
                    self.projection_failure(&key, &error);
                }
                None
            }
        }
    }
    pub(crate) fn save_view_settings(
        &mut self,
        mut settings: crate::viewer_views::Settings,
    ) -> Result<(), String> {
        if !settings.valid() {
            return Err("invalid viewer settings".into());
        }
        for i in 0..self.panels.viewer_views.settings.layout().count() {
            let view = self
                .panels
                .viewer_views
                .settings
                .view_mut(i)
                .ok_or("view missing during cancellation")?;
            let before = view.clone();
            self.panels.viewer_views.history[i].cancel(view);
            if before != *view {
                if let Some(next) = settings.view_mut(i).filter(|next| next.mode == view.mode) {
                    next.orbit = view.orbit.clone();
                }
            }
        }
        self.actions
            .extend(panels::cancel_interaction(&mut self.panels));
        self.panels.viewer_views.settings = settings;
        self.panels.viewer_views.loaded = None;
        self.persist_view_settings()?;
        self.dirty = true;
        self.present_due = true;
        Ok(())
    }
    pub(crate) fn camera_view(
        &mut self,
        key: Key,
        cell: usize,
        action: motion_host::ops::preview::CameraGesture,
    ) -> Result<(), String> {
        if !key.same_context(&self.projection_key())
            || cell != self.panels.viewer_views.settings.active()
        {
            return Ok(());
        }
        let canvas = [
            self.state["project"]["width"].as_u64().unwrap_or(0) as u32,
            self.state["project"]["height"].as_u64().unwrap_or(0) as u32,
        ];
        let persist = matches!(
            action,
            motion_host::ops::preview::CameraGesture::End { .. }
                | motion_host::ops::preview::CameraGesture::Undo
                | motion_host::ops::preview::CameraGesture::Redo
        );
        let view = self
            .panels
            .viewer_views
            .settings
            .view_mut(cell)
            .ok_or("camera view missing")?;
        self.panels.viewer_views.history[cell]
            .apply(view, key.source, key.frame, action, canvas).map_err(|error|{
                log::error!("viewer camera failed operation=viewer_camera phase=gesture window={} view={} frame={} reason={error}",key.window,cell,f64::from_bits(key.frame));error
            })?;
        if persist {
            self.persist_view_settings()?;
        }
        self.dirty = true;
        self.present_due = true;
        Ok(())
    }
    pub(crate) fn zoom_view(&mut self, ratio: f64) -> Result<(), String> {
        use motion_host::ops::preview::{CameraGesture as Op, CameraMode};
        if !ratio.is_finite() || ratio <= 0. {
            return Err("invalid viewer zoom".into());
        }
        let cell = self.panels.viewer_views.settings.active();
        let key = self.projection_key();
        self.camera_view(key, cell, Op::Begin { id: 0 })?;
        let size = [
            self.state["project"]["width"].as_u64().unwrap_or(0) as u32,
            self.state["project"]["height"].as_u64().unwrap_or(0) as u32,
        ];
        self.camera_view(
            self.projection_key(),
            cell,
            Op::Update {
                id: 0,
                mode: CameraMode::Dolly,
                delta: [0., (-ratio.ln() * 240.) as f32],
                viewport: size,
            },
        )?;
        self.camera_view(
            self.projection_key(),
            cell,
            Op::End {
                id: 0,
                commit: true,
            },
        )
    }
    pub(crate) fn drag_view(
        &mut self,
        key: Key,
        cell: usize,
        object: u64,
        delta: [f32; 2],
        size: [i32; 2],
    ) -> Result<(), String> {
        self.stop();
        if key != self.projection_key() || cell != self.panels.viewer_views.settings.active() {
            return Ok(());
        }
        let canvas = [
            self.state["project"]["width"].as_u64().unwrap_or(0) as u32,
            self.state["project"]["height"].as_u64().unwrap_or(0) as u32,
        ];
        let camera = self.panels.viewer_views.settings.camera(cell, canvas)?;
        self.editing = Some((
            Instant::now(),
            self.engine.viewer_drag_async(engine::viewer_input::Drag {
                window: key.window,
                source: key.source,
                frame: f64::from_bits(key.frame),
                camera,
                object,
                delta,
                size,
            })?,
        ));
        Ok(())
    }
}
