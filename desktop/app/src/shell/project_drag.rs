//! Reuse the native window/DPI projection used by effect drags. No panel owns
//! an OS window or infers a release from passive repainting.
use crate::*;

impl Shell {
    pub(crate) fn route_project_drag_event(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        if !self.panels.project_drag.pending() {
            return false;
        }
        if !self.window.as_ref().is_some_and(|w| w.id() == id) && !self.floating.contains_key(&id) {
            return false;
        }
        let (physical, released) = match event {
            WindowEvent::CursorMoved { position, .. } => ([position.x, position.y], false),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: WinitMouseButton::Left,
                ..
            } => {
                let (point, scale) = self
                    .floating
                    .get(&id)
                    .map_or((self.input.mouse, self.scale), |f| (f.input.mouse, f.scale));
                (
                    [
                        f64::from(point[0]) * f64::from(scale),
                        f64::from(point[1]) * f64::from(scale),
                    ],
                    true,
                )
            }
            WindowEvent::CursorLeft { .. } => return true,
            WindowEvent::CloseRequested => {
                self.panels.project_drag.cancel();
                return false;
            }
            _ => return false,
        };
        let pointer = match self.drag_window_point(id, physical) {
            Ok(Some(pointer)) => Some(pointer),
            Ok(None) => {
                self.panels.project_drag.cancel();
                None
            }
            Err(cause) => {
                log::error!("project asset drag failed phase=window_position cause={cause}");
                self.panels.message = Some(self.workspace.locale.error(&cause));
                self.panels.project_drag.cancel();
                None
            }
        };
        // Captured release may arrive on the source window after crossing into
        // another owner. Retain that window's last physical cursor position.
        if let WindowEvent::CursorMoved { position, .. } = event {
            if let Some(f) = self.floating.get_mut(&id) {
                f.input.mouse = [position.x as f32 / f.scale, position.y as f32 / f.scale];
                f.paint_input.mouse = f.input.mouse;
            } else {
                self.input.mouse = [
                    position.x as f32 / self.scale,
                    position.y as f32 / self.scale,
                ];
                self.paint_input.mouse = self.input.mouse;
            }
        }
        if let Some((owner, point)) = pointer {
            let styles = self
                .panels
                .styles
                .clone()
                .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
            self.panels.project_drag.pointer(
                owner,
                point,
                released,
                styles.metric("motion.drag_threshold", 3.),
            );
            if let Some(panel) = owner {
                if let Some(f) = self.floating.values_mut().find(|f| f.panel == panel) {
                    f.input.mouse = point;
                    f.paint_input.mouse = point;
                }
            } else {
                self.input.mouse = point;
                self.paint_input.mouse = point;
            }
        }
        for input in [&mut self.input, &mut self.paint_input] {
            super::effect_drag::clear_pointer(input);
        }
        for f in self.floating.values_mut() {
            super::effect_drag::clear_pointer(&mut f.input);
            super::effect_drag::clear_pointer(&mut f.paint_input);
        }
        self.dirty = true;
        self.redraw();
        true
    }
}
