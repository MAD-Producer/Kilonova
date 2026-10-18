//! Native mixed-button and keyboard-focus acceptance.
use super::*;

impl App {
    pub(super) fn input_frame(
        &mut self,
        id: WindowId,
        event: motion_ui::input::Event,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        let frames = self.windows[&id].frames;
        ui::accumulate(&mut self.windows.get_mut(&id).unwrap().input, event);
        self.redraw(id, event_loop)?;
        if self.windows[&id].frames <= frames {
            return Err("input ownership probe did not present a native frame".into());
        }
        Ok(())
    }

    pub(super) fn secondary_buttons_probe(
        &mut self,
        id: WindowId,
        event_loop: &ActiveEventLoop,
        path: &std::path::Path,
    ) -> Result<(), String> {
        use motion_ui::{input::Event, MouseButton};
        let before = self.model.borrow().clone();
        let captured = self.router.captured();
        if before.transaction != Some(25.) || before.value != 35. || captured.is_none() {
            return Err("secondary-button probe has no active numeric transaction".into());
        }
        for button in [MouseButton::Right, MouseButton::Middle] {
            let position = self.windows[&id].input.mouse;
            for event in [
                Event::MousePressed { position, button },
                Event::MouseReleased { position, button },
            ] {
                self.input_frame(id, event, event_loop)?;
                if *self.model.borrow() != before
                    || self.router.captured() != captured
                    || self.windows[&id].input.held != Some(MouseButton::Left)
                {
                    return Err(
                        "secondary button changed the primary gesture or committed history".into(),
                    );
                }
            }
        }
        self.capture(id, path)
    }

    pub(super) fn tab_probe(
        &mut self,
        id: WindowId,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        use motion_ui::{
            input::{Event, Modifiers},
            Key,
        };
        let original = self.router.focus().ok_or("numeric field is not focused")?;
        let before = self.model.borrow().clone();
        self.input_frame(
            id,
            Event::KeyRepeated {
                key: Key::Tab,
                modifiers: Modifiers::default(),
            },
            event_loop,
        )?;
        if self.router.focus() != Some(original) {
            return Err("repeated Tab changed native focus".into());
        }
        self.input_frame(
            id,
            Event::KeyPressed {
                key: Key::Tab,
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
            },
            event_loop,
        )?;
        if self.router.focus() != Some(original) {
            return Err("Ctrl+Tab changed widget focus".into());
        }
        self.input_frame(
            id,
            Event::KeyPressed {
                key: Key::Tab,
                modifiers: Modifiers::default(),
            },
            event_loop,
        )?;
        let text = motion_ui::interaction::Target {
            widget: 2,
            ..original
        };
        if self.router.focus() != Some(text) {
            return Err("Tab did not focus the adjacent native text field".into());
        }
        self.input_frame(
            id,
            Event::KeyPressed {
                key: Key::Tab,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
            event_loop,
        )?;
        self.input_frame(
            id,
            Event::KeyReleased {
                key: Key::Tab,
                modifiers: Modifiers::default(),
            },
            event_loop,
        )?;
        if self.router.focus() != Some(original) || *self.model.borrow() != before {
            return Err("Shift+Tab failed to restore focus without editing the model".into());
        }
        Ok(())
    }

    pub(super) fn floating_input_probe(
        &mut self,
        id: WindowId,
        event_loop: &ActiveEventLoop,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let area = self.layouts(&self.windows[&id])[0].bounds;
        let r = field_rect(area, 0);
        use motion_ui::{input::Event, MouseButton};
        self.input_frame(
            id,
            Event::MousePressed {
                position: [r.min[0] + 10., r.min[1] + 10.],
                button: MouseButton::Left,
            },
            event_loop,
        )?;
        self.input_frame(
            id,
            Event::MouseMoved {
                position: [r.min[0] + 30., r.min[1] + 10.],
            },
            event_loop,
        )?;
        self.secondary_buttons_probe(id, event_loop, &path.with_extension("input-floating.png"))?;
        self.input_frame(id, Event::FocusLost, event_loop)?;
        let model = self.model.borrow();
        if model.value != 25.
            || model.transaction.is_some()
            || !model.undo.is_empty()
            || model.commits != 1
        {
            return Err(
                "floating focus loss did not cancel the original numeric transaction".into(),
            );
        }
        drop(model);
        // Refocus the number without opening its text editor, then cancel.
        self.input_frame(
            id,
            Event::MousePressed {
                position: [r.min[0] + 10., r.min[1] + 10.],
                button: MouseButton::Left,
            },
            event_loop,
        )?;
        self.input_frame(
            id,
            Event::KeyPressed {
                key: motion_ui::Key::Escape,
                modifiers: Default::default(),
            },
            event_loop,
        )?;
        self.input_frame(
            id,
            Event::MouseReleased {
                position: [r.min[0] + 10., r.min[1] + 10.],
                button: MouseButton::Left,
            },
            event_loop,
        )?;
        self.tab_probe(id, event_loop)?;
        Ok(())
    }
}
