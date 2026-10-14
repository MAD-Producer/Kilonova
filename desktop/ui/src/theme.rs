//! Colour palette and metrics for the desktop shell.
//!
//! The shell uses a near-black panel chrome, one accent colour for selection
//! and focus, and a muted grey scale for type. Values are kept here rather
//! than inline so a future light theme is a data change.

/// Linear RGBA colour with sRGB-encoded components.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Color(pub [f32; 4]);

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a])
    }
    pub const fn grey(v: u8) -> Self {
        Self::rgb(v, v, v)
    }
    pub fn with_alpha(self, alpha: f32) -> Self {
        Self([self.0[0], self.0[1], self.0[2], alpha])
    }
}

/// Chrome colours shared by every panel.
#[allow(non_snake_case)]
#[derive(Clone, Copy)]
pub struct Colors {
    pub APP_BACKGROUND: Color,
    pub PANEL: Color,
    pub SUNKEN: Color,
    pub CHROME: Color,
    pub HEADER: Color,
    pub BORDER: Color,
    pub ACCENT: Color,
    pub TEXT: Color,
    pub TEXT_MUTED: Color,
    pub KEYFRAME: Color,
    pub TRACK_CAMERA: Color,
    pub TRANSPARENT_LIGHT: Color,
    pub TRANSPARENT_DARK: Color,
    pub TRACKS: [Color; 3],
}
impl Colors {
    pub fn from_sheet(sheet: &crate::style::StyleSheet) -> Self {
        Self {
            APP_BACKGROUND: sheet.color("app"),
            PANEL: sheet.color("panel"),
            SUNKEN: sheet.color("well"),
            CHROME: sheet.color("raised"),
            HEADER: sheet.color("header"),
            BORDER: sheet.color("border"),
            ACCENT: sheet.color("focus"),
            TEXT: sheet.color("text"),
            TEXT_MUTED: sheet.color("muted"),
            KEYFRAME: sheet.color("keyframe"),
            TRACK_CAMERA: sheet.color("camera"),
            TRANSPARENT_LIGHT: sheet.color("checker.light"),
            TRANSPARENT_DARK: sheet.color("checker.dark"),
            TRACKS: [
                sheet.color("track.0"),
                sheet.color("track.1"),
                sheet.color("track.2"),
            ],
        }
    }
}

/// Panel metrics in logical pixels at 100% UI scale.
pub mod metrics {
    /// Height of the application menu bar.
    pub const TITLE_BAR: f32 = 24.0;
    pub const MENU_BAR: f32 = 24.0;
    /// Height of the compact tool bar under the menu bar.
    pub const TOOL_BAR: f32 = 32.0;
    /// Height of a panel tab strip.
    pub const TAB_BAR: f32 = 24.0;
    /// Height of one timeline layer row.
    pub const ROW: f32 = 24.0;
    /// Width of the timeline ruler gutter.
    pub const RULER: f32 = 92.0;
    /// Minimum width a dock may be dragged to.
    pub const MIN_DOCK: f32 = 160.0;
    /// Minimum height the timeline may be dragged to.
    pub const MIN_TIMELINE: f32 = 90.0;
    /// Width of a docked splitter grab handle.
    pub const SPLITTER: f32 = 4.0;
    /// Height of a property row in the effect and property panels.
    pub const PROPERTY_ROW: f32 = 22.0;
}

/// UI scale factor. Hosts expose this so a HiDPI display and a large-text
/// preference both map onto one number instead of every widget guessing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Scale(pub f32);

impl Default for Scale {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Scale {
    pub fn pixels(&self, value: f32) -> f32 {
        value * self.0
    }
    /// Clamp to the range where the dense timeline stays legible.
    pub fn clamped(value: f32) -> Self {
        Self(value.clamp(0.75, 2.0))
    }
}
