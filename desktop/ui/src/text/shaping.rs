//! Positioned glyphs for a single, directionally uniform font run.
//!
//! Paragraph bidi resolution, font fallback and atlas rasterization belong to
//! the caller. This module does not silently substitute scalar layout on error.

use harfrust::{Buffer, ClusterLevel, Font, ShapeOptions, ShaperFont};
use std::{fmt, sync::Arc};

pub use harfrust::{Direction, GlyphInfo, GlyphPosition};

/// Reject oversized runs before allocating shaping scratch storage.
pub const MAX_RUN_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub enum ShapeError {
    InvalidFont { face_index: u32, bytes: usize },
    RunTooLong { bytes: usize, limit: usize },
    InvalidDirection,
    Engine(harfrust::ShapeError),
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFont { face_index, bytes } => write!(
                f,
                "UI shaping font invalid: face_index={face_index} bytes={bytes}"
            ),
            Self::RunTooLong { bytes, limit } => write!(
                f,
                "UI shaping run exceeds limit: bytes={bytes} limit={limit}"
            ),
            Self::InvalidDirection => f.write_str("UI shaping run requires a horizontal direction"),
            Self::Engine(cause) => write!(f, "UI shaping engine failed: {cause}"),
        }
    }
}

impl std::error::Error for ShapeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Engine(cause) => Some(cause),
            _ => None,
        }
    }
}

/// Glyphs are in visual order; cluster values remain UTF-8 source byte offsets.
/// Metrics are in font units, with positive Y pointing upward.
pub struct ShapedRun<'a> {
    pub glyphs: &'a [GlyphInfo],
    pub positions: &'a [GlyphPosition],
    pub units_per_em: u16,
    pub direction: Direction,
}

impl ShapedRun<'_> {
    pub fn advance(&self) -> [i64; 2] {
        self.positions.iter().fold([0, 0], |sum, p| {
            [
                sum[0] + i64::from(p.x_advance),
                sum[1] + i64::from(p.y_advance),
            ]
        })
    }
}

/// Keeps parsed font data and scratch storage for subsequent runs.
pub struct RunShaper {
    font: Font,
    buffer: Buffer,
}

impl RunShaper {
    pub fn new(data: Arc<[u8]>, face_index: u32) -> Result<Self, ShapeError> {
        let bytes = data.len();
        let source: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(data);
        let font =
            Font::new(source, face_index).ok_or(ShapeError::InvalidFont { face_index, bytes })?;
        Ok(Self {
            font,
            buffer: Buffer::new(),
        })
    }

    /// The caller supplies a resolved direction and splits mixed-script text
    /// into suitable runs. Length and direction checks happen before clearing
    /// the last buffer; engine errors propagate without scalar substitution.
    pub fn shape(&mut self, text: &str, direction: Direction) -> Result<ShapedRun<'_>, ShapeError> {
        if text.len() > MAX_RUN_BYTES {
            return Err(ShapeError::RunTooLong {
                bytes: text.len(),
                limit: MAX_RUN_BYTES,
            });
        }
        if !matches!(direction, Direction::LeftToRight | Direction::RightToLeft) {
            return Err(ShapeError::InvalidDirection);
        }
        self.buffer.clear();
        self.buffer.push_str(text);
        self.buffer
            .set_cluster_level(ClusterLevel::MonotoneGraphemes);
        self.buffer.set_direction(direction);
        self.buffer.guess_segment_properties();
        harfrust::shape(
            &ShaperFont::new(&self.font),
            &mut self.buffer,
            ShapeOptions::default(),
        )
        .map_err(ShapeError::Engine)?;
        Ok(ShapedRun {
            glyphs: self.buffer.glyph_infos(),
            positions: self.buffer.glyph_positions(),
            units_per_em: self.font.units_per_em(),
            direction,
        })
    }
}
