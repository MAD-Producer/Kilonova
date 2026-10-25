//! Input events normalised from winit.
//!
//! The widget layer never sees a winit type, which keeps the layout testable
//! without a window and lets the Android-derived interaction rules (long press
//! to move a keyframe, tap versus drag on the timeline) be expressed once.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    A,
    B,
    C,
    E,
    H,
    L,
    Q,
    V,
    W,
    F,
    G,
    T,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    N,
    O,
    I,
    S,
    Z,
    Y,
    D,
    K,
    P,
    R,
    U,
    J,
    F3,
    F1,
    F2,
    F5,
    F6,
    F7,
    F8,
    F10,
    F11,
    F12,
    F9,
    M,
    X,
    Equal,
    Minus,
    Semicolon,
    Backquote,
    PageUp,
    PageDown,
    Escape,
    Enter,
    Tab,
    Space,
    Delete,
    Backspace,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Comma,
    Period,
    BracketLeft,
    BracketRight,
    F4,
    Shift,
    Control,
    Alt,
}

/// A key with its modifiers folded in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
}

/// Exact chord matching prevents Alt/Shift variants from invoking another command.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyChord {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyChord {
    pub const fn new(key: Key, control: bool, shift: bool, alt: bool) -> Self {
        Self {
            key,
            modifiers: Modifiers {
                control,
                shift,
                alt,
            },
        }
    }

    pub fn matches(self, key: Key, modifiers: Modifiers) -> bool {
        self.key == key && self.modifiers == modifiers
    }

    pub fn pressed(self, input: &impl crate::InputSource) -> bool {
        let view = input.input_view();
        let input = &view;
        input
            .key_presses()
            .any(|(key, modifiers, repeat)| !repeat && self.matches(key, modifiers))
    }
}

impl Default for Modifiers {
    fn default() -> Self {
        Self {
            shift: false,
            control: false,
            alt: false,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    MouseMoved {
        position: [f32; 2],
    },
    MousePressed {
        position: [f32; 2],
        button: MouseButton,
    },
    MouseReleased {
        position: [f32; 2],
        button: MouseButton,
    },
    MouseWheel {
        delta: [f32; 2],
    },
    KeyPressed {
        key: Key,
        modifiers: Modifiers,
    },
    KeyRepeated {
        key: Key,
        modifiers: Modifiers,
    },
    KeyReleased {
        key: Key,
        modifiers: Modifiers,
    },
    ImeComposition(bool),
    TextCommit(String),
    TextInput(String),
    /// A press held past the long-press threshold; the timeline and preview use
    /// this to distinguish a scrub from an intended clip move.
    LongPress {
        position: [f32; 2],
    },
    /// The pointer left the native window. The last in-window coordinate must
    /// no longer be eligible for release hit testing.
    PointerLeft,
    FocusLost,
}

/// Accumulated input for one frame.
#[derive(Default, Clone)]
pub struct Input {
    pub text_clipboard: Option<std::sync::Arc<crate::text_clipboard::TextClipboard>>,
    /// New events await a real interaction pass, independently of paint dirtiness.
    pub pending: bool,
    pub focus_lost: bool,
    pub held: Option<MouseButton>,
    pub mouse: [f32; 2],
    /// Press origin and peak travel survive coalesced pointer events and redraws.
    pub pointer_origin: Option<[f32; 2]>,
    pub pointer_travel: f32,
    pub pressed: Option<(MouseButton, [f32; 2])>,
    pub released: Option<(MouseButton, [f32; 2])>,
    pub wheel: [f32; 2],
    pub modifiers: Modifiers,
    pub long_press: Option<[f32; 2]>,
    pub keys: Vec<(Key, Modifiers)>,
    /// Parallel to `keys`; absent entries are initial presses for synthetic input.
    pub key_repeats: Vec<bool>,
    pub held_keys: Vec<Key>,
    pub released_keys: Vec<Key>,
    pub composing: bool,
    pub ime_consumed: bool,
    pub text: String,
}

/// A frame snapshot with borrowed payloads and independently maskable scalar fields.
/// Views never own events and cannot outlive their source input. Routing and
/// clipping copy this bounded value while sharing the text and key slices.
/// A view cannot be retained after its event source is dropped:
/// ```compile_fail
/// use motion_ui::{Input, InputSource};
/// let view = {
///     let input = Input::default();
///     input.input_view()
/// };
/// assert!(view.text.is_empty());
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct InputView<'a> {
    pub text_clipboard: Option<&'a std::sync::Arc<crate::text_clipboard::TextClipboard>>,
    pub pending: bool,
    pub focus_lost: bool,
    pub held: Option<MouseButton>,
    pub mouse: [f32; 2],
    pub pointer_origin: Option<[f32; 2]>,
    pub pointer_travel: f32,
    pub pressed: Option<(MouseButton, [f32; 2])>,
    pub released: Option<(MouseButton, [f32; 2])>,
    pub wheel: [f32; 2],
    pub modifiers: Modifiers,
    pub long_press: Option<[f32; 2]>,
    pub keys: &'a [(Key, Modifiers)],
    pub key_repeats: &'a [bool],
    pub held_keys: &'a [Key],
    pub released_keys: &'a [Key],
    pub composing: bool,
    pub ime_consumed: bool,
    pub text: &'a str,
}

/// Read-only input accepted by shared controls. Event accumulation stays in Input.
pub trait InputSource {
    fn input_view(&self) -> InputView<'_>;
}
impl InputSource for Input {
    fn input_view(&self) -> InputView<'_> {
        InputView {
            text_clipboard: self.text_clipboard.as_ref(),
            pending: self.pending,
            focus_lost: self.focus_lost,
            held: self.held,
            mouse: self.mouse,
            pointer_origin: self.pointer_origin,
            pointer_travel: self.pointer_travel,
            pressed: self.pressed,
            released: self.released,
            wheel: self.wheel,
            modifiers: self.modifiers,
            long_press: self.long_press,
            keys: &self.keys,
            key_repeats: &self.key_repeats,
            held_keys: &self.held_keys,
            released_keys: &self.released_keys,
            composing: self.composing,
            ime_consumed: self.ime_consumed,
            text: &self.text,
        }
    }
}
impl InputSource for InputView<'_> {
    fn input_view(&self) -> InputView<'_> { *self }
}
impl<'a> InputView<'a> {
    /// Explicit ownership boundary for callers retaining a frame snapshot.
    /// Routing and per-control filtering should keep the borrowed view instead.
    pub fn to_owned(self) -> Input {
        Input {
            text_clipboard: self.text_clipboard.cloned(),
            pending: self.pending,
            focus_lost: self.focus_lost,
            held: self.held,
            mouse: self.mouse,
            pointer_origin: self.pointer_origin,
            pointer_travel: self.pointer_travel,
            pressed: self.pressed,
            released: self.released,
            wheel: self.wheel,
            modifiers: self.modifiers,
            long_press: self.long_press,
            keys: self.keys.to_owned(),
            key_repeats: self.key_repeats.to_owned(),
            held_keys: self.held_keys.to_owned(),
            released_keys: self.released_keys.to_owned(),
            composing: self.composing,
            ime_consumed: self.ime_consumed,
            text: self.text.to_owned(),
        }
    }

    pub fn dragging(&self) -> bool {
        self.held == Some(MouseButton::Left)
    }
    /// Shortcut input excludes IME confirmation keys, including the commit frame.
    pub fn key_presses(self) -> impl Iterator<Item = (Key, Modifiers, bool)> + 'a {
        self.keys
            .iter()
            .enumerate()
            .filter(move |_| !self.composing && !self.ime_consumed)
            .map(move |(i, (key, modifiers))| {
                (
                    *key,
                    *modifiers,
                    self.key_repeats.get(i).copied().unwrap_or(false),
                )
            })
    }
}

impl Input {
    pub fn begin_frame(&mut self) {
        self.pending = false;
        self.focus_lost = false;
        self.pressed = None;
        self.released = None;
        self.wheel = [0.0, 0.0];
        self.long_press = None;
        self.keys.clear();
        self.key_repeats.clear();
        self.released_keys.clear();
        self.ime_consumed = false;
        self.text.clear();
    }

    /// True while the primary button is held, whether or not it moved.
    pub fn dragging(&self) -> bool {
        self.held == Some(MouseButton::Left)
    }

    /// Shortcut input excludes IME confirmation keys, including the commit frame.
    pub fn key_presses(&self) -> impl Iterator<Item = (Key, Modifiers, bool)> + '_ {
        self.input_view().key_presses()
    }
}

/// A rectangular region with hit testing.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            min: [x, y],
            max: [x + width, y + height],
        }
    }
    pub fn width(&self) -> f32 {
        self.max[0] - self.min[0]
    }
    pub fn height(&self) -> f32 {
        self.max[1] - self.min[1]
    }
    pub fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.min[0]
            && point[0] < self.max[0]
            && point[1] >= self.min[1]
            && point[1] < self.max[1]
    }
    /// True when the point is within `slack` of the horizontal edge.
    pub fn near_edge(&self, point: [f32; 2], slack: f32) -> bool {
        point[1] >= self.min[1] && point[1] < self.max[1] && (point[0] - self.min[0]).abs() <= slack
    }
    pub fn contains_rect(&self, other: Rect) -> bool {
        other.min[0] >= self.min[0]
            && other.min[1] >= self.min[1]
            && other.max[0] <= self.max[0]
            && other.max[1] <= self.max[1]
    }
    pub fn intersection(&self, other: Rect) -> Rect {
        let x = self.min[0].max(other.min[0]);
        let y = self.min[1].max(other.min[1]);
        Rect::new(
            x,
            y,
            (self.max[0].min(other.max[0]) - x).max(0.),
            (self.max[1].min(other.max[1]) - y).max(0.),
        )
    }
    /// Shrink symmetrically, used to inset widget contents from panel chrome.
    pub fn shrink(&self, amount: f32) -> Self {
        Self {
            min: [self.min[0] + amount, self.min[1] + amount],
            max: [self.max[0] - amount, self.max[1] - amount],
        }
    }
}

/// Scroll state for a panel that owns a scroll offset.
#[derive(Clone, Copy, Default, Debug)]
pub struct Scroll {
    pub offset: f32,
    /// Accumulated but unapplied wheel delta, in the same units as `offset`.
    pending: f32,
    pub max: f32,
}

impl Scroll {
    /// Recompute the scrollable distance without moving the offset.
    pub fn set_extent(&mut self, viewport: f32, content: f32) {
        self.max = (content - viewport).max(0.0);
        self.offset = self.offset.clamp(0.0, self.max);
    }

    /// Consume this frame's wheel delta, clamped to the available range.
    pub fn consume(&mut self, wheel: f32, viewport: f32, content: f32) {
        self.pending += wheel;
        self.set_extent(viewport, content);
        self.offset = (self.offset - self.pending).clamp(0.0, self.max);
        self.pending = 0.0;
    }
    /// Total scrollable distance, used to draw the scrollbar thumb.
    pub fn bar(&self, viewport: f32, content: f32) -> (f32, f32) {
        if content <= viewport || content <= 0.0 {
            return (0.0, 0.0);
        }
        let fraction = (viewport / content).clamp(0.05, 1.0);
        let travel = self.max;
        (fraction, (self.offset / travel.max(1.0)).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;
    use crate::ui::accumulate;
    #[test]
    fn key_repeat_release_and_focus_have_distinct_lifetimes() {
        let mut input = Input::default();
        let modifiers = Modifiers {
            control: true,
            ..Default::default()
        };
        let duplicate = KeyChord::new(Key::D, true, false, false);
        accumulate(
            &mut input,
            Event::KeyPressed {
                key: Key::D,
                modifiers,
            },
        );
        assert!(duplicate.pressed(&input));
        input.begin_frame();
        assert_eq!(input.held_keys, [Key::D]);
        accumulate(
            &mut input,
            Event::KeyRepeated {
                key: Key::D,
                modifiers,
            },
        );
        assert!(!duplicate.pressed(&input));
        assert_eq!(input.key_presses().next(), Some((Key::D, modifiers, true)));
        accumulate(
            &mut input,
            Event::KeyReleased {
                key: Key::D,
                modifiers,
            },
        );
        assert!(input.held_keys.is_empty());
        assert_eq!(input.released_keys, [Key::D]);
        accumulate(
            &mut input,
            Event::MousePressed {
                position: [10., 10.],
                button: MouseButton::Left,
            },
        );
        accumulate(
            &mut input,
            Event::MouseMoved {
                position: [30., 10.],
            },
        );
        assert_eq!(input.pointer_travel, 20.);
        accumulate(&mut input, Event::FocusLost);
        assert!(input.keys.is_empty());
        assert!(input.released_keys.is_empty());
        assert!(input.key_repeats.is_empty());
        assert!(input.held.is_none());
        assert!(input.pointer_origin.is_none());
        assert_eq!(input.pointer_travel, 0.);
        assert_eq!(input.modifiers, Modifiers::default());
    }
    #[test]
    fn ime_confirmation_cannot_become_a_shortcut_and_commit_keeps_text() {
        let mut input = Input::default();
        accumulate(&mut input, Event::ImeComposition(true));
        input.begin_frame();
        accumulate(
            &mut input,
            Event::KeyPressed {
                key: Key::Space,
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
            },
        );
        assert!(input.keys.is_empty());
        accumulate(&mut input, Event::TextCommit("效果".into()));
        accumulate(
            &mut input,
            Event::KeyPressed {
                key: Key::Enter,
                modifiers: Modifiers::default(),
            },
        );
        assert!(input.key_presses().next().is_none());
        assert_eq!(input.text, "效果");
        input.begin_frame();
        accumulate(
            &mut input,
            Event::KeyPressed {
                key: Key::Enter,
                modifiers: Modifiers::default(),
            },
        );
        assert!(KeyChord::new(Key::Enter, false, false, false).pressed(&input));
    }
}
