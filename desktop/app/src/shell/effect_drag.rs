//! Internal effect drags cross native windows using physical client coordinates.
//! Other gestures retain their existing pointer-leave and focus-loss cancellation.
use crate::*;

fn client_point(
    screen: [f64; 2],
    origin: [f64; 2],
    size: [u32; 2],
    scale: f32,
) -> Option<[f32; 2]> {
    let local = [screen[0] - origin[0], screen[1] - origin[1]];
    (scale.is_finite()
        && scale > 0.
        && local[0] >= 0.
        && local[1] >= 0.
        && local[0] < size[0] as f64
        && local[1] < size[1] as f64)
        .then(|| [local[0] as f32 / scale, local[1] as f32 / scale])
}
#[cfg(target_os = "windows")]
fn native_window_at(screen: [f64; 2]) -> isize {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn WindowFromPoint(point: Point) -> *mut std::ffi::c_void;
    }
    // Winit supplies physical screen coordinates; a null result is a normal desktop miss.
    unsafe {
        WindowFromPoint(Point {
            x: screen[0] as i32,
            y: screen[1] as i32,
        }) as isize
    }
}
fn native_hit(source: &Window, screen: [f64; 2]) -> Option<isize> {
    #[cfg(test)]
    if source.is_visible() == Some(false) {
        return None;
    } // Hidden GPU fixtures have no OS hit surface.
    #[cfg(target_os = "windows")]
    {
        let _ = source;
        Some(native_window_at(screen))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (source, screen);
        None
    }
}
fn matches_native_hit(window: &Window, hit: Option<isize>) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    if let Some(hit) = hit {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = window
            .window_handle()
            .map_err(|e| format!("target native handle: {e}"))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("expected Win32 client handle".into());
        };
        return Ok(handle.hwnd.get() == hit);
    }
    let _ = (window, hit);
    Ok(true)
}

fn clear_pointer(input: &mut Input) {
    input.pressed = None;
    input.released = None;
    input.held = None;
    input.pointer_origin = None;
    input.pointer_travel = 0.;
    input.long_press = None;
}
impl Shell {
    fn effect_drag_window_point(
        &self,
        id: WindowId,
        point: [f64; 2],
    ) -> Result<Option<(Option<dock::Panel>, [f32; 2])>, String> {
        let source = self
            .window
            .as_ref()
            .filter(|w| w.id() == id)
            .map(|w| (w, self.scale, None))
            .or_else(|| {
                self.floating
                    .get(&id)
                    .map(|f| (&f.window, f.scale, Some(f.panel)))
            });
        let Some((source, scale, owner)) = source else {
            return Ok(None);
        };
        if cfg!(not(target_os = "windows")) {
            let size = source.inner_size();
            if let Some(point) = client_point(point, [0., 0.], [size.width, size.height], scale) {
                return Ok(Some((owner, point)));
            }
        }
        let origin = source
            .inner_position()
            .map_err(|e| format!("source client position: {e}"))?;
        let screen = [origin.x as f64 + point[0], origin.y as f64 + point[1]];
        let native = native_hit(source, screen);
        // Win32 resolves the actual visible window, including overlapping clients and external windows.
        // Other platforms and hidden GPU fixtures use client geometry; sorting makes that projection deterministic.
        let mut floats: Vec<_> = self.floating.values().collect();
        floats.sort_by_key(|f| f.key);
        for f in floats
            .into_iter()
            .rev()
            .filter(|f| f.window.is_minimized() != Some(true))
        {
            let origin = f
                .window
                .inner_position()
                .map_err(|e| format!("target window={} client position: {e}", f.key))?;
            let size = f.window.inner_size();
            if let Some(point) = client_point(
                screen,
                [origin.x as f64, origin.y as f64],
                [size.width, size.height],
                f.scale,
            ) {
                if matches_native_hit(&f.window, native)? {
                    return Ok(Some((Some(f.panel), point)));
                }
            }
        }
        if let Some(main) = self
            .window
            .as_ref()
            .filter(|w| w.is_minimized() != Some(true))
        {
            let origin = main
                .inner_position()
                .map_err(|e| format!("main client position: {e}"))?;
            let size = main.inner_size();
            if let Some(point) = client_point(
                screen,
                [origin.x as f64, origin.y as f64],
                [size.width, size.height],
                self.scale,
            ) {
                if matches_native_hit(main, native)? {
                    return Ok(Some((None, point)));
                }
            }
        }
        if native.is_some() {
            return Ok(Some((owner, [f32::MIN; 2])));
        }
        // Outside every client: retain the source space so release cancels normally.
        Ok(Some((
            owner,
            [point[0] as f32 / scale, point[1] as f32 / scale],
        )))
    }
    fn cancel_effect_drag_coordinates(&mut self, id: WindowId, cause: &str) {
        // One error terminates this gesture, so healthy pointer moves never emit records.
        let bounded: String = cause.chars().take(512).collect();
        log::error!(
            "effect drag cancelled phase=window_coordinates window={id:?} reason={bounded}"
        );
        self.panels.message = Some(format!(
            "{}: {bounded}",
            if self.workspace.locale == Locale::Zh {
                "无法定位拖放窗口，已取消效果拖动"
            } else {
                "Could not locate drop window; effect drag cancelled"
            }
        ));
        self.panels.effect_drag.cancel();
        self.panels.browser.cancel_library_click();
    }
    pub(crate) fn route_effect_drag_event(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        if !self.panels.effect_drag.pending() {
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
                // Recheck the destination on release: a native window may have moved or covered it.
                let (mouse, scale) = if let Some(f) = self.floating.get(&id) {
                    (f.input.mouse, f.scale)
                } else {
                    (self.input.mouse, self.scale)
                };
                (
                    [
                        mouse[0] as f64 * scale as f64,
                        mouse[1] as f64 * scale as f64,
                    ],
                    true,
                )
            }
            WindowEvent::CursorLeft { .. } => return true,
            WindowEvent::CloseRequested => {
                self.panels.effect_drag.cancel();
                return false;
            }
            _ => return false,
        };
        let pointer = match self.effect_drag_window_point(id, physical) {
            Ok(Some(pointer)) => Some((pointer, released)),
            Ok(None) => {
                self.panels.effect_drag.cancel();
                None
            }
            Err(cause) => {
                self.cancel_effect_drag_coordinates(id, &cause);
                None
            }
        };
        if let WindowEvent::CursorMoved { position, .. } = event {
            if self.window.as_ref().is_some_and(|w| w.id() == id) {
                self.input.mouse = [
                    position.x as f32 / self.scale,
                    position.y as f32 / self.scale,
                ];
                self.paint_input.mouse = self.input.mouse;
            } else if let Some(f) = self.floating.get_mut(&id) {
                f.input.mouse = [position.x as f32 / f.scale, position.y as f32 / f.scale];
                f.paint_input.mouse = f.input.mouse;
            }
        }
        if let Some(((owner, point), released)) = pointer {
            if point != [f32::MIN; 2] {
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
            let styles = self
                .panels
                .styles
                .clone()
                .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
            self.panels.effect_drag.pointer(
                owner,
                point,
                released,
                styles.metric("motion.drag_threshold", 3.),
            );
        }
        clear_pointer(&mut self.input);
        clear_pointer(&mut self.paint_input);
        for f in self.floating.values_mut() {
            clear_pointer(&mut f.input);
            clear_pointer(&mut f.paint_input);
        }
        self.dirty = true;
        self.redraw();
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_client_projection_handles_negative_monitors_and_mixed_dpi() {
        assert_eq!(
            client_point([-850., 240.], [-1000., 90.], [900, 600], 1.5),
            Some([100., 100.])
        );
        assert_eq!(
            client_point([1100., 300.], [900., 100.], [800, 600], 2.),
            Some([100., 100.])
        );
        assert_eq!(client_point([900., 300.], [0., 0.], [900, 600], 1.), None);
        assert_eq!(client_point([0., -1.], [0., 0.], [900, 600], 1.), None);
        assert_eq!(client_point([0., 0.], [0., 0.], [900, 600], 0.), None);
    }
}

#[cfg(test)]
mod integration_tests;
