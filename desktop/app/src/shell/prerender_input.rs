//! Optional rendering yields before hit testing and action generation.
use crate::{engine::InputActivity, Shell};
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};

fn activity(event: &WindowEvent) -> Option<InputActivity> {
    match event {
        WindowEvent::MouseInput { state, button, .. } => Some(InputActivity::Button {
            // Match the editor's existing three-button translation.
            index: match button {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                _ => 2,
            },
            pressed: *state == ElementState::Pressed,
        }),
        WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
            Some(InputActivity::Pulse)
        }
        WindowEvent::MouseWheel { .. }
        | WindowEvent::Ime(Ime::Commit(_) | Ime::Preedit(_, _))
        | WindowEvent::DroppedFile(_) => Some(InputActivity::Pulse),
        WindowEvent::Focused(false) | WindowEvent::CloseRequested => Some(InputActivity::Cancel),
        _ => None,
    }
}

impl Shell {
    pub(crate) fn observe_prerender_input(&self, event: &WindowEvent) {
        let Some(activity) = activity(event) else {
            return;
        };
        static FAILURES: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        match self.engine.prerender_input(activity) {
            Ok(()) => {
                FAILURES.success();
                // Some keys and drag routes produce no translated UI event.
                // Admission still needs a subsequent successful foreground frame.
                self.redraw();
            }
            Err(error) => FAILURES.failure("prerender.input", format_args!("reason={error}")),
        }
    }
}
