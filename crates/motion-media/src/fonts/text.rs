//! Bounded source-pixel paragraph layout. Font bytes and glyphs use FontStore's
//! existing validated import and cache. Missing glyphs fail before publication.
use super::*;
use motion_model::{TextAlign, TextStyle};

struct Line {
    first: bool,
    glyphs: Vec<(Arc<Glyph>, f32)>,
    advance: f32,
}
impl Line {
    fn new(first: bool) -> Self {
        Self {
            first,
            glyphs: Vec::new(),
            advance: 0.,
        }
    }
}

impl FontStore {
    pub fn raster_styled_text(
        &mut self,
        id: &str,
        text: &str,
        style: &TextStyle,
    ) -> Result<Raster> {
        style.validate()?;
        if text.chars().count() > 4096 {
            return Err("text layout exceeds 4096 characters".into());
        }
        // Preserve the established unstyled raster's exact pixel semantics.
        if style == &TextStyle::new(style.font_size, style.width) && !text.contains(['\r', '\t']) {
            return self.raster_text(id, text, style.font_size, style.width);
        }
        let font = self.face(id)?;
        let metrics = font
            .horizontal_line_metrics(style.font_size)
            .ok_or("font has no line metrics")?;
        if ![metrics.ascent, metrics.descent, metrics.new_line_size]
            .iter()
            .all(|v| v.is_finite())
            || metrics.new_line_size <= 0.
        {
            return Err("font has invalid line metrics".into());
        }
        let leading = style
            .line_height
            .unwrap_or_else(|| metrics.new_line_size.ceil().max(1.));
        let pad = style.stroke_width.ceil();
        let available = |first| {
            style.width as f32
                - pad * 2.
                - style.indent_left
                - style.indent_right
                - if first { style.indent_first } else { 0. }
        };
        let mut lines = vec![Line::new(true)];
        let mut previous = None;
        let mut characters = text.chars().peekable();
        while let Some(ch) = characters.next() {
            if ch == '\n' || ch == '\r' {
                if ch == '\r' && characters.peek() == Some(&'\n') {
                    characters.next();
                }
                lines.push(Line::new(true));
                previous = None;
                continue;
            }
            let glyph = self.glyph(id, if ch == '\t' { ' ' } else { ch }, style.font_size)?;
            let advance = glyph.metrics.advance * if ch == '\t' { 4. } else { 1. };
            let line = lines.last().expect("initial line exists");
            let kern = previous
                .and_then(|p| font.horizontal_kern(p, ch, style.font_size))
                .unwrap_or(0.);
            let x = if previous.is_some() {
                line.advance + style.tracking + kern
            } else {
                0.
            };
            let x = if !line.glyphs.is_empty() && x + advance > available(line.first) {
                lines.push(Line::new(false));
                0.
            } else {
                x
            };
            let line = lines.last_mut().expect("initial line exists");
            line.glyphs.push((glyph, x));
            line.advance = x + advance;
            previous = Some(ch);
        }
        let baseline = metrics.ascent.ceil() + pad;
        let height = (baseline + (lines.len() - 1) as f32 * leading - metrics.descent + pad)
            .ceil()
            .max(1.);
        if !height.is_finite() || height > 8192. {
            return Err("font raster exceeds dimension or memory limits".into());
        }
        let mut raster = blank(style.width, height as u32)?;
        for (index, line) in lines.into_iter().enumerate() {
            let extra = (available(line.first) - line.advance).max(0.);
            let origin = pad
                + style.indent_left
                + if line.first { style.indent_first } else { 0. }
                + match style.alignment {
                    TextAlign::Left => 0.,
                    TextAlign::Center => extra * 0.5,
                    TextAlign::Right => extra,
                };
            for (glyph, x) in line.glyphs {
                let y = (baseline + index as f32 * leading).floor() as i32
                    - glyph.metrics.bearing_y
                    - glyph.metrics.height as i32;
                blit(
                    &mut raster,
                    &glyph,
                    origin.floor() as i32 + x.floor() as i32 + glyph.metrics.bearing_x,
                    y,
                );
            }
        }
        if style.stroke_width > 0. {
            outline(&mut raster, style.stroke_width);
        }
        Ok(raster)
    }
}

// Two separable maximum filters implement a square-join outside outline in
// O(pixels), independently of its radius. Both buffers obey blank's 16MP cap.
fn dilate(alpha: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    if radius == 0 {
        return alpha.to_vec();
    }
    let mut horizontal = vec![0; alpha.len()];
    let mut result = vec![0; alpha.len()];
    let filter = |count: usize, read: &dyn Fn(usize) -> u8, write: &mut dyn FnMut(usize, u8)| {
        let mut queue = VecDeque::new();
        let mut end = 0;
        for index in 0..count {
            while end < count && end <= index + radius {
                let value = read(end);
                while queue.back().is_some_and(|&old| read(old) <= value) {
                    queue.pop_back();
                }
                queue.push_back(end);
                end += 1;
            }
            while queue.front().is_some_and(|&old| old + radius < index) {
                queue.pop_front();
            }
            write(index, read(*queue.front().expect("nonempty filter window")));
        }
    };
    for y in 0..height {
        filter(width, &|x| alpha[y * width + x], &mut |x, v| {
            horizontal[y * width + x] = v
        });
    }
    for x in 0..width {
        filter(height, &|y| horizontal[y * width + x], &mut |y, v| {
            result[y * width + x] = v
        });
    }
    result
}
fn outline(raster: &mut Raster, width: f32) {
    let alpha: Vec<_> = raster.rgba.chunks_exact(4).map(|p| p[3]).collect();
    let low = width.floor() as usize;
    let high = width.ceil() as usize;
    let inner = dilate(&alpha, raster.width as usize, raster.height as usize, low);
    let outer = if high == low {
        None
    } else {
        Some(dilate(
            &alpha,
            raster.width as usize,
            raster.height as usize,
            high,
        ))
    };
    for (index, pixel) in raster.rgba.chunks_exact_mut(4).enumerate() {
        let coverage = outer.as_ref().map_or(inner[index], |outer| {
            (f32::from(inner[index]) + f32::from(outer[index] - inner[index]) * width.fract())
                .round() as u8
        });
        if coverage > 0 {
            pixel.copy_from_slice(&[255, 255, 255, coverage]);
        }
    }
}
