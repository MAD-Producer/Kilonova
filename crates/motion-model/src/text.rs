//! Authored text layout, in source pixels. UI DPI never enters this model.
use crate::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextStyle {
    pub font_size: f32,
    pub width: u32,
    /// None uses the selected font's line metrics.
    pub line_height: Option<f32>,
    /// Additional space between characters, in source pixels.
    pub tracking: f32,
    pub alignment: TextAlign,
    pub indent_left: f32,
    pub indent_right: f32,
    pub indent_first: f32,
    /// An outside square-join outline using the text's fill/tint color.
    pub stroke_width: f32,
}

impl TextStyle {
    pub fn new(font_size: f32, width: u32) -> Self {
        Self {
            font_size,
            width,
            line_height: None,
            tracking: 0.,
            alignment: TextAlign::Left,
            indent_left: 0.,
            indent_right: 0.,
            indent_first: 0.,
            stroke_width: 0.,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.font_size.is_finite() && (4. ..=256.).contains(&self.font_size),
            "font size must be 4..256 pixels",
        )?;
        ensure(
            (1..=8192).contains(&self.width),
            "text width must be 1..8192 pixels",
        )?;
        ensure(
            self.line_height
                .is_none_or(|v| v.is_finite() && (4. ..=1024.).contains(&v)),
            "line height must be automatic or 4..1024 pixels",
        )?;
        ensure(
            self.tracking.is_finite() && (-256. ..=256.).contains(&self.tracking),
            "tracking must be -256..256 pixels",
        )?;
        ensure(
            self.stroke_width.is_finite() && (0. ..=32.).contains(&self.stroke_width),
            "text stroke width must be 0..32 pixels",
        )?;
        ensure(
            [self.indent_left, self.indent_right, self.indent_first]
                .iter()
                .all(|v| v.is_finite()),
            "text indentation must be finite",
        )?;
        let available = self.width as f32 - self.stroke_width.ceil() * 2.;
        ensure(
            self.indent_left >= 0.
                && self.indent_right >= 0.
                && self.indent_left + self.indent_right < available
                && self.indent_left + self.indent_first >= 0.
                && self.indent_left + self.indent_first + self.indent_right < available,
            "text indentation leaves no writable line",
        )
    }
}
