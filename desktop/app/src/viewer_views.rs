//! Bounded per-view state. Presentation must explicitly render each requested
//! camera; changing the global observer between tiles is not a multiview API.
use crate::viewer_navigation::Navigation;
use motion_core::{ObservationView, Observer};
use motion_ui::input::Rect;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Layout {
    #[default]
    Single,
    Two,
    Four,
}
impl Layout {
    pub(crate) fn count(self) -> usize {
        match self {
            Self::Single => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }
    /// Gap comes from the owning window's semantic token. Empty windows are
    /// normal; non-finite coordinates and negative gaps are caller errors.
    pub(crate) fn tiles(self, viewport: Rect, gap: f32) -> Result<Vec<Rect>, &'static str> {
        if !viewport
            .min
            .iter()
            .chain(&viewport.max)
            .all(|v| v.is_finite())
            || !gap.is_finite()
            || gap < 0.
            || !viewport.width().is_finite()
            || !viewport.height().is_finite()
        {
            return Err("invalid multi-view viewport or gap");
        }
        let columns = if self == Self::Single { 1 } else { 2 };
        let rows = if self == Self::Four { 2 } else { 1 };
        let width = (viewport.width() - gap * (columns - 1) as f32) / columns as f32;
        let height = (viewport.height() - gap * (rows - 1) as f32) / rows as f32;
        if width <= 0. || height <= 0. {
            return Ok(vec![]);
        }
        let mut tiles = Vec::with_capacity(self.count());
        for row in 0..rows {
            for column in 0..columns {
                let min = [
                    viewport.min[0] + column as f32 * (width + gap),
                    viewport.min[1] + row as f32 * (height + gap),
                ];
                // Assign the final edge directly, avoiding floating-point
                // overshoot that can send a tile outside its native window.
                let max = [
                    if column + 1 == columns {
                        viewport.max[0]
                    } else {
                        min[0] + width
                    },
                    if row + 1 == rows {
                        viewport.max[1]
                    } else {
                        min[1] + height
                    },
                ];
                let tile = Rect { min, max };
                if tile.width() <= 0. || tile.height() <= 0. {
                    return Ok(vec![]);
                }
                tiles.push(tile);
            }
        }
        Ok(tiles)
    }
    pub(crate) fn hit(
        self,
        viewport: Rect,
        gap: f32,
        point: [f32; 2],
    ) -> Result<Option<usize>, &'static str> {
        if !point.iter().all(|v| v.is_finite()) {
            return Ok(None);
        }
        Ok(self
            .tiles(viewport, gap)?
            .iter()
            .position(|tile| (0..2).all(|i| point[i] >= tile.min[i] && point[i] < tile.max[i])))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "object",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Mode {
    Composition,
    Camera(u64),
    Free,
    Top,
    Side,
}

/// Only the observer's authored scalars, never project tracks, source names or
/// a copy of the complete scene. All numeric validation uses the model camera.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Orbit {
    target: [f32; 3],
    radius: f32,
    azimuth: f32,
    elevation: f32,
    fov: f32,
}
impl Orbit {
    pub(crate) fn capture(observer: &Observer) -> Self {
        Self {
            target: observer.camera.target.value,
            radius: observer.camera.radius.value,
            azimuth: observer.camera.azimuth.value,
            elevation: observer.camera.elevation.value,
            fov: observer.camera.fov.value,
        }
    }
    fn install(&self, observer: &mut Observer) -> Result<(), String> {
        observer.camera.target.value = self.target;
        observer.camera.radius.value = self.radius;
        observer.camera.azimuth.value = self.azimuth;
        observer.camera.elevation.value = self.elevation;
        observer.camera.fov.value = self.fov;
        observer.camera.validate_local().map_err(|e| e.to_string())
    }
    fn valid(&self) -> bool {
        self.install(&mut Observer::new(1920, 1080)).is_ok()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct View {
    pub(crate) mode: Mode,
    pub(crate) navigation: Navigation,
    pub(crate) orbit: Option<Orbit>,
}
impl View {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            navigation: Default::default(),
            orbit: None,
        }
    }
    /// `None` means an authored camera, not a fallback view. The presenter must
    /// resolve `mode` explicitly, including the identity of `Mode::Camera`.
    pub(crate) fn observer(&self, canvas: [u32; 2]) -> Result<Option<Observer>, String> {
        if canvas.contains(&0) {
            return Err("invalid multi-view canvas".into());
        }
        let view = match self.mode {
            Mode::Free => ObservationView::Free,
            Mode::Top => ObservationView::Top,
            Mode::Side => ObservationView::Side,
            Mode::Composition | Mode::Camera(_) => return Ok(None),
        };
        let mut observer = Observer::new(canvas[0], canvas[1]);
        observer.view = view;
        if let Some(orbit) = &self.orbit {
            orbit.install(&mut observer)?;
        }
        let pose = observer
            .pose(canvas[0], canvas[1])
            .map_err(|e| e.to_string())?;
        if !pose.view_projection.is_finite() {
            return Err("invalid multi-view observer projection".into());
        }
        Ok(Some(observer))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SavedSettings")]
pub(crate) struct Settings {
    layout: Layout,
    active: usize,
    views: [View; 4],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedSettings {
    layout: Layout,
    active: usize,
    views: [View; 4],
}
impl TryFrom<SavedSettings> for Settings {
    type Error = &'static str;
    fn try_from(saved: SavedSettings) -> Result<Self, Self::Error> {
        let settings = Self {
            layout: saved.layout,
            active: saved.active,
            views: saved.views,
        };
        if settings.valid() {
            Ok(settings)
        } else {
            Err("invalid multi-view settings")
        }
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            layout: Layout::Single,
            active: 0,
            views: [
                View::new(Mode::Composition),
                View::new(Mode::Top),
                View::new(Mode::Side),
                View::new(Mode::Free),
            ],
        }
    }
}
impl Settings {
    pub(crate) fn valid(&self) -> bool {
        self.active < self.layout.count()
            && self
                .views
                .iter()
                .all(|v| v.orbit.as_ref().is_none_or(Orbit::valid))
    }
    pub(crate) fn layout(&self) -> Layout {
        self.layout
    }
    pub(crate) fn active(&self) -> usize {
        self.active
    }
    pub(crate) fn view(&self, index: usize) -> Option<&View> {
        self.views
            .get(index)
            .filter(|_| index < self.layout.count())
    }
    pub(crate) fn view_mut(&mut self, index: usize) -> Option<&mut View> {
        self.views
            .get_mut(index)
            .filter(|_| index < self.layout.count())
    }
    pub(crate) fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
        if self.active >= layout.count() {
            self.active = 0;
        }
    }
    pub(crate) fn activate(&mut self, index: usize) -> bool {
        if index >= self.layout.count() {
            return false;
        }
        self.active = index;
        true
    }
    pub(crate) fn camera(
        &self,
        index: usize,
        canvas: [u32; 2],
    ) -> Result<crate::engine::viewer_frame::Camera, String> {
        let view = self.view(index).ok_or("view is unavailable")?;
        Ok(match view.mode {
            Mode::Composition => crate::engine::viewer_frame::Camera::Composition,
            Mode::Camera(id) => crate::engine::viewer_frame::Camera::Camera(id),
            _ => crate::engine::viewer_frame::Camera::Observer(
                view.observer(canvas)?.ok_or("observer is unavailable")?,
            ),
        })
    }
}

/// Opaque, bounded identity supplied by the composition owner. Frame/revision
/// changes invalidate view-only navigation; no project data is retained here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) window: u64,
    pub(crate) document: u64,
    pub(crate) composition: u64,
    pub(crate) revision: u64,
    pub(crate) frame: u64,
}
#[derive(Clone)]
struct Capture {
    context: Context,
    tile: usize,
    viewport: Rect,
    gap: f32,
    layout: Layout,
    before: View,
}
#[derive(Default)]
pub(crate) struct Views {
    pub(crate) settings: Settings,
    capture: Option<Capture>,
}
impl Views {
    pub(crate) fn begin(
        &mut self,
        context: Context,
        viewport: Rect,
        gap: f32,
        point: [f32; 2],
    ) -> Result<bool, &'static str> {
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.context.window != context.window)
        {
            return Ok(false);
        }
        self.cancel();
        let Some(tile) = self.settings.layout.hit(viewport, gap, point)? else {
            return Ok(false);
        };
        self.settings.active = tile;
        self.capture = Some(Capture {
            context,
            tile,
            viewport,
            gap,
            layout: self.settings.layout,
            before: self.settings.views[tile].clone(),
        });
        Ok(true)
    }
    /// Foreign native-window input neither edits nor cancels the owning gesture.
    /// A changed document/frame/geometry in the owner cancels before any edit.
    pub(crate) fn captured(
        &mut self,
        context: Context,
        viewport: Rect,
        gap: f32,
    ) -> Option<&mut View> {
        let capture = self.capture.as_ref()?;
        if capture.context.window != context.window {
            return None;
        }
        if capture.context != context
            || capture.viewport != viewport
            || capture.gap != gap
            || capture.layout != self.settings.layout
        {
            self.cancel();
            return None;
        }
        let tile = capture.tile;
        Some(&mut self.settings.views[tile])
    }
    pub(crate) fn release(&mut self, context: Context, viewport: Rect, gap: f32) -> bool {
        if self.captured(context, viewport, gap).is_none() {
            return false;
        }
        self.capture.take();
        true
    }
    pub(crate) fn cancel(&mut self) -> bool {
        let Some(capture) = self.capture.take() else {
            return false;
        };
        self.settings.views[capture.tile] = capture.before;
        true
    }
    pub(crate) fn set_layout(&mut self, layout: Layout) {
        self.cancel();
        self.settings.set_layout(layout);
    }
    pub(crate) fn set_mode(&mut self, index: usize, mode: Mode) -> bool {
        if self.settings.view(index).is_none() {
            return false;
        }
        self.cancel();
        self.settings.views[index].mode = mode;
        true
    }
}

/// Per-cell observer navigation history. It never enters project undo.
#[derive(Default)]
pub(crate) struct CameraHistory {
    context: Option<(crate::layer_clipboard::KeyframeSource, u64, Mode)>,
    pending: Option<(u64, View)>,
    undo: std::collections::VecDeque<View>,
    redo: std::collections::VecDeque<View>,
}
impl CameraHistory {
    pub(crate) fn enabled(
        &self,
        source: crate::layer_clipboard::KeyframeSource,
        frame: u64,
        mode: Mode,
        redo: bool,
    ) -> bool {
        self.context.as_ref() == Some(&(source, frame, mode))
            && self.pending.is_none()
            && if redo {
                !self.redo.is_empty()
            } else {
                !self.undo.is_empty()
            }
    }
    pub(crate) fn cancel(&mut self, view: &mut View) {
        if let Some((_, before)) = self.pending.take() {
            *view = before;
        }
    }
    pub(crate) fn apply(
        &mut self,
        view: &mut View,
        source: crate::layer_clipboard::KeyframeSource,
        frame: u64,
        op: motion_host::ops::preview::CameraGesture,
        canvas: [u32; 2],
    ) -> Result<(), String> {
        use motion_host::ops::preview::{CameraGesture as Op, CameraMode};
        let context = (source, frame, view.mode);
        if self.context.as_ref() != Some(&context) {
            self.cancel(view);
            self.undo.clear();
            self.redo.clear();
            self.context = Some(context);
        }
        match op {
            Op::Begin { id } => {
                self.cancel(view);
                if view.observer(canvas)?.is_none() {
                    return Err("camera gestures require an observer view".into());
                }
                self.pending = Some((id, view.clone()));
            }
            Op::Update {
                id,
                mode,
                delta,
                viewport,
            } => {
                let Some((owner, before)) = self.pending.as_ref() else {
                    return Ok(());
                };
                if *owner != id {
                    return Err("view camera gesture owner changed".into());
                }
                if viewport.contains(&0)
                    || viewport.iter().any(|v| *v > 65536)
                    || delta.iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.)
                {
                    return Err("invalid view camera gesture coordinates".into());
                }
                let mut observer = before.observer(canvas)?.ok_or("observer unavailable")?;
                match mode {
                    CameraMode::Orbit => {
                        if before.mode != Mode::Free {
                            return Err("orbit requires a free view".into());
                        }
                        observer
                            .orbit(delta[0] * 0.2, delta[1] * 0.2)
                            .map_err(|e| e.to_string())?;
                    }
                    CameraMode::Dolly => observer
                        .zoom((-delta[1] / 240.).clamp(-20., 20.).exp())
                        .map_err(|e| e.to_string())?,
                    CameraMode::Pan => {
                        let pose = observer
                            .pose(canvas[0], canvas[1])
                            .map_err(|e| e.to_string())?;
                        let point = observer.camera.target.value;
                        let world = glam::DVec3::new(
                            point[0] as f64 - canvas[0] as f64 / 2.,
                            canvas[1] as f64 / 2. - point[1] as f64,
                            -point[2] as f64,
                        );
                        let vp = pose.view_projection.as_dmat4();
                        let mut clip = vp * world.extend(1.);
                        if !clip.is_finite() || clip.w <= 1e-8 {
                            return Err("view pan target is behind camera".into());
                        }
                        let scale = (viewport[0] as f64 / canvas[0] as f64)
                            .min(viewport[1] as f64 / canvas[1] as f64);
                        clip.x += 2. * delta[0] as f64 / (canvas[0] as f64 * scale) * clip.w;
                        clip.y -= 2. * delta[1] as f64 / (canvas[1] as f64 * scale) * clip.w;
                        let moved = vp.inverse() * clip;
                        if !moved.is_finite() || moved.w.abs() <= 1e-8 {
                            return Err("invalid view pan projection".into());
                        }
                        let offset = moved.truncate() / moved.w - world;
                        observer.camera.target.value = [
                            point[0] - offset.x as f32,
                            point[1] + offset.y as f32,
                            point[2] + offset.z as f32,
                        ];
                    }
                }
                let mut next = before.clone();
                next.orbit = Some(Orbit::capture(&observer));
                next.observer(canvas)?;
                *view = next;
            }
            Op::End { id, commit } => {
                if let Some((owner, before)) = self.pending.take() {
                    if owner != id {
                        self.pending = Some((owner, before));
                        return Err("view camera gesture owner changed".into());
                    }
                    if !commit {
                        *view = before;
                    } else if before != *view {
                        retain_view(&mut self.undo, before);
                        self.redo.clear();
                    }
                }
            }
            Op::Undo | Op::Redo => {
                self.cancel(view);
                let (from, to) = if matches!(op, Op::Redo) {
                    (&mut self.redo, &mut self.undo)
                } else {
                    (&mut self.undo, &mut self.redo)
                };
                if let Some(before) = from.pop_back() {
                    retain_view(to, std::mem::replace(view, before));
                }
            }
        }
        Ok(())
    }
}
fn retain_view(stack: &mut std::collections::VecDeque<View>, view: View) {
    if stack.len() == 64 {
        stack.pop_front();
    }
    stack.push_back(view);
}

#[cfg(test)]
mod tests;
