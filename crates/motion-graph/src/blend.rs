//! Separable color transfers for the graph's bounded source-over Blend node.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u32)]
pub enum ColorBlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
}

impl ColorBlendMode {
    pub const ALL: [Self; 15] = [
        Self::Normal,
        Self::Add,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::Difference,
        Self::Exclusion,
        Self::Subtract,
        Self::Divide,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::HardLight,
        Self::SoftLight,
    ];

    pub fn title(self) -> (&'static str, &'static str) {
        match self {
            Self::Normal => ("正常", "Normal"),
            Self::Add => ("相加", "Add"),
            Self::Multiply => ("正片叠底", "Multiply"),
            Self::Screen => ("滤色", "Screen"),
            Self::Overlay => ("叠加", "Overlay"),
            Self::Darken => ("变暗", "Darken"),
            Self::Lighten => ("变亮", "Lighten"),
            Self::Difference => ("差值", "Difference"),
            Self::Exclusion => ("排除", "Exclusion"),
            Self::Subtract => ("相减", "Subtract"),
            Self::Divide => ("相除", "Divide"),
            Self::ColorDodge => ("颜色减淡", "Color Dodge"),
            Self::ColorBurn => ("颜色加深", "Color Burn"),
            Self::HardLight => ("强光", "Hard Light"),
            Self::SoftLight => ("柔光", "Soft Light"),
        }
    }
}
