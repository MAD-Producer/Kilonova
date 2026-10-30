//! Borrow unmasked input and reuse each legacy mask once per source/window frame.
use motion_ui::{Input, Rect};

pub(super) struct Routes<'a> {
    source: &'a Input,
    masked: [Option<Input>; 3],
}
impl<'a> Routes<'a> {
    pub fn new(source: &'a Input) -> Self {
        Self {
            source,
            masked: Default::default(),
        }
    }
    pub fn for_panel(&mut self, focused: bool, area: Rect, owns_popup: bool) -> &Input {
        let keyboard = !focused && (!self.source.keys.is_empty() || !self.source.text.is_empty());
        let pressed = !owns_popup && self.source.pressed.is_some_and(|(_, p)| !area.contains(p));
        let mask = usize::from(keyboard) | (usize::from(pressed) << 1);
        if mask == 0 {
            return self.source;
        }
        self.masked[mask - 1].get_or_insert_with(|| {
            let mut input = self.source.clone();
            if keyboard {
                input.keys.clear();
                input.text.clear();
            }
            if pressed {
                input.pressed = None;
            }
            input
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_ui::{
        input::{Key, Modifiers, MouseButton},
        InputSource,
    };

    #[test]
    fn reused_masks_match_previous_routing_and_preserve_all_other_input_fields() {
        let area = Rect::new(10., 20., 100., 80.);
        for focused in [false, true] {
            for owns_popup in [false, true] {
                for point in [[30., 40.], [-1., -1.]] {
                    for mode in 0..4 {
                        let input = Input {
                            pending: true,
                            focus_lost: mode == 3,
                            mouse: [91., 72.],
                            pressed: Some((MouseButton::Right, point)),
                            released: Some((MouseButton::Middle, [80., 70.])),
                            held: Some(MouseButton::Left),
                            pointer_origin: Some([1., 2.]),
                            pointer_travel: 42.,
                            wheel: [3., -4.],
                            long_press: Some([4., 5.]),
                            modifiers: Modifiers {
                                control: true,
                                ..Default::default()
                            },
                            keys: vec![(Key::F8, Default::default())],
                            key_repeats: vec![true],
                            held_keys: vec![Key::F8],
                            released_keys: vec![Key::F7],
                            text: "测🙂e\u{301}".repeat(1024),
                            composing: mode == 1,
                            ime_consumed: mode == 2,
                            ..Default::default()
                        };
                        let before = format!("{:?}", input.input_view());
                        let mut expected = input.clone();
                        if !owns_popup {
                            expected.pressed = expected.pressed.filter(|(_, p)| area.contains(*p));
                        }
                        if !focused {
                            expected.keys.clear();
                            expected.text.clear();
                        }
                        let mut routes = Routes::new(&input);
                        let first = routes.for_panel(focused, area, owns_popup) as *const Input;
                        for _ in 0..64 {
                            let routed = routes.for_panel(focused, area, owns_popup);
                            assert_eq!(
                                format!("{:?}", routed.input_view()),
                                format!("{:?}", expected.input_view())
                            );
                            assert_eq!(
                                routed as *const Input, first,
                                "same mask must reuse its owned payload"
                            );
                        }
                        if focused && (owns_popup || area.contains(point)) {
                            assert_eq!(first, &input as *const Input);
                        }
                        assert_eq!(format!("{:?}", input.input_view()), before);
                    }
                }
            }
        }
    }
}
