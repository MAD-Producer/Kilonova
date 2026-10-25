//! Native per-window coordinates, separate from the widget's invalid outside hit point.
use motion_ui::input::Event;
use winit::dpi::PhysicalPosition;

#[derive(Default)]
pub(crate) struct PointerPosition(Option<PhysicalPosition<f64>>);

impl PointerPosition {
    pub(crate) fn moved(&mut self, position: PhysicalPosition<f64>, scale: f32) -> Event {
        self.0 = Some(position);
        Self::motion(position, scale)
    }

    // winit 0.30's Windows WM_MOUSEMOVE handler emits CursorEntered but suppresses
    // CursorMoved when the client coordinate equals its last cached position.
    // Restore that same coordinate on entry; an ensuing actual move replaces it.
    // CursorLeft still invalidates widget hits and cancels the previous gesture.
    #[cfg(target_os = "windows")]
    pub(crate) fn entered(&self, scale: f32) -> Option<Event> {
        self.0.map(|position| Self::motion(position, scale))
    }

    fn motion(position: PhysicalPosition<f64>, scale: f32) -> Event {
        Event::MouseMoved {
            position: [position.x as f32 / scale, position.y as f32 / scale],
        }
    }
}
