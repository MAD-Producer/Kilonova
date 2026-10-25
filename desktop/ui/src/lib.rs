//! Desktop widget toolkit for the Motion Studio shell.
//!
//! The shell is a native winit/wgpu application rather than a web view, so the
//! composition preview is presented by the engine's own swapchain with no copy
//! and no IPC hop. Panels are drawn here through a fixed two-pass pipeline:
//!
//! 1. [`paint`] records solid shapes and atlas-sampled glyphs.
//! 2. [`dock`] resolves an After Effects style tab/dock tree into rectangles.
//! 3. [`ui`] draws widgets and reports interaction through [`input`].
//!
//! Keeping the layout model free of rendering means a layout test needs no GPU,
//! which is why [`dock`] carries its own assertions.

// Compile the actual desktop adapter in native allocation tests without making
// the widget toolkit depend on the renderer in production.
#[cfg(test)]
extern crate self as motion_ui;

pub mod dock;
mod observability;
pub mod icons;
pub mod input;
pub mod interaction;
pub mod motion;
pub mod cursor;
pub mod snap;
pub mod overlay;
pub mod paint;
pub mod panel;
pub mod registry;
pub mod style;
pub mod tabs;
pub mod text;
pub mod text_clipboard;
pub mod theme;
pub mod ui;
pub mod widgets;
pub mod workspace;

pub use input::{Input, InputSource, InputView, Key, MouseButton, Rect};
pub use paint::{PaintList, Painter};
pub use text::TextAtlas;
pub use theme::{Color, Scale};
