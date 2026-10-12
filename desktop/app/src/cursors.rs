//! Compact interaction cursors, cached once per window density.
use motion_ui::cursor::Cursor as Kind;
use std::collections::HashMap;
use winit::{
    event_loop::ActiveEventLoop,
    window::{Cursor, CursorIcon, CustomCursor},
};

const EDGE: f32 = 17.;
const CUSTOM: [Kind; 8] = [
    Kind::Grab,
    Kind::Grabbing,
    Kind::Move,
    Kind::ResizeEW,
    Kind::ResizeNS,
    Kind::ResizeNESW,
    Kind::ResizeNWSE,
    Kind::Crosshair,
];
#[derive(Default)]
pub struct Cursors {
    images: HashMap<(u8, u16), CustomCursor>,
}
fn density(scale: f32) -> u16 {
    if scale.is_finite() {
        (scale.clamp(0.5, 4.) * 64.).round() as u16
    } else {
        64
    }
}
impl Cursors {
    pub fn prepare(&mut self, event_loop: &ActiveEventLoop, scale: f32) {
        let ratio = density(scale);
        for kind in CUSTOM {
            self.images.entry((kind as u8, ratio)).or_insert_with(|| {
                let (rgba, size) = raster(kind, ratio as f32 / 64.);
                let source = CustomCursor::from_rgba(rgba, size, size, size / 2, size / 2)
                    .expect("bounded cursor bitmap with centered hotspot");
                event_loop.create_custom_cursor(source)
            });
        }
    }
    pub fn get(&self, kind: Kind, scale: f32) -> Cursor {
        self.images
            .get(&(kind as u8, density(scale)))
            .map(|cursor| Cursor::Custom(cursor.clone()))
            .unwrap_or_else(|| Cursor::Icon(system(kind)))
    }
}
fn system(kind: Kind) -> CursorIcon {
    match kind {
        Kind::Default => CursorIcon::Default,
        Kind::Grab => CursorIcon::Grab,
        Kind::Grabbing => CursorIcon::Grabbing,
        Kind::Move => CursorIcon::Move,
        Kind::ResizeEW => CursorIcon::EwResize,
        Kind::ResizeNS => CursorIcon::NsResize,
        Kind::ResizeNESW => CursorIcon::NeswResize,
        Kind::ResizeNWSE => CursorIcon::NwseResize,
        Kind::Text => CursorIcon::Text,
        Kind::Crosshair => CursorIcon::Crosshair,
        Kind::Wait => CursorIcon::Wait,
    }
}
type Point = [f32; 2];
fn paths(kind: Kind) -> Vec<Vec<Point>> {
    let horizontal = vec![
        vec![[2., 8.5], [15., 8.5]],
        vec![[5., 5.5], [2., 8.5], [5., 11.5]],
        vec![[12., 5.5], [15., 8.5], [12., 11.5]],
    ];
    match kind {
        Kind::ResizeEW => horizontal,
        Kind::ResizeNS => horizontal
            .into_iter()
            .map(|path| path.into_iter().map(|[x, y]| [y, x]).collect())
            .collect(),
        Kind::ResizeNESW | Kind::ResizeNWSE => {
            let angle = if kind == Kind::ResizeNWSE {
                std::f32::consts::FRAC_PI_4
            } else {
                -std::f32::consts::FRAC_PI_4
            };
            let (s, c) = angle.sin_cos();
            horizontal
                .into_iter()
                .map(|path| {
                    path.into_iter()
                        .map(|[x, y]| {
                            let (x, y) = (x - 8.5, y - 8.5);
                            [8.5 + x * c - y * s, 8.5 + x * s + y * c]
                        })
                        .collect()
                })
                .collect()
        }
        Kind::Crosshair => vec![vec![[3., 8.5], [14., 8.5]], vec![[8.5, 3.], [8.5, 14.]]],
        Kind::Move => vec![
            vec![[2., 8.5], [15., 8.5]],
            vec![[8.5, 2.], [8.5, 15.]],
            vec![[4.5, 6.], [2., 8.5], [4.5, 11.]],
            vec![[12.5, 6.], [15., 8.5], [12.5, 11.]],
            vec![[6., 4.5], [8.5, 2.], [11., 4.5]],
            vec![[6., 12.5], [8.5, 15.], [11., 12.5]],
        ],
        Kind::Grab => vec![vec![
            [6., 9.],
            [6., 3.],
            [7., 2.],
            [8., 3.],
            [8., 7.],
            [8., 2.],
            [9., 1.5],
            [10., 2.],
            [10., 7.],
            [10., 3.],
            [11., 2.5],
            [12., 3.],
            [12., 8.],
            [12., 5.],
            [13., 4.5],
            [14., 5.5],
            [14., 11.],
            [12., 15.],
            [8., 15.],
            [4., 11.],
            [3., 9.],
            [4., 8.],
            [6., 10.],
        ]],
        Kind::Grabbing => vec![vec![
            [5., 10.],
            [5., 5.],
            [6., 4.],
            [7., 5.],
            [7., 8.],
            [7., 4.],
            [8., 3.],
            [9., 4.],
            [9., 8.],
            [9., 4.],
            [10., 3.],
            [11., 4.],
            [11., 8.],
            [11., 5.],
            [12., 4.],
            [13., 5.],
            [13., 11.],
            [11., 15.],
            [7., 15.],
            [3., 11.],
            [3., 9.],
            [4., 8.],
            [5., 10.],
        ]],
        _ => vec![],
    }
}
fn distance(p: Point, a: Point, b: Point) -> f32 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / (d[0] * d[0] + d[1] * d[1]).max(1e-6))
        .clamp(0., 1.);
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}
fn raster(kind: Kind, scale: f32) -> (Vec<u8>, u16) {
    let size = (EDGE * scale).round().max(1.) as u16;
    let paths = paths(kind);
    let mut rgba = vec![0; usize::from(size) * usize::from(size) * 4];
    for y in 0..size {
        for x in 0..size {
            let p = [(x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale];
            let d = paths
                .iter()
                .flat_map(|path| path.windows(2))
                .map(|pair| distance(p, pair[0], pair[1]))
                .fold(f32::INFINITY, f32::min);
            // A one-pixel light stroke with a narrow dark outline stays visible on
            // both the canvas and chrome; coverage AA scales in physical pixels.
            let light = ((0.5 - d) * scale + 0.5).clamp(0., 1.);
            let alpha = ((1.1 - d) * scale + 0.5).clamp(0., 1.);
            if alpha > 0. {
                let value = ((240. * light + 24. * (alpha - light)) / alpha).round() as u8;
                let offset = (usize::from(y) * usize::from(size) + usize::from(x)) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[
                    value,
                    value,
                    value,
                    (alpha * 255.).round() as u8,
                ]);
            }
        }
    }
    (rgba, size)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thin_cursors_have_valid_centered_hotspots_at_fractional_dpi() {
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            for kind in CUSTOM {
                let (rgba, size) = raster(kind, scale);
                assert_eq!(size, (17. * scale).round() as u16);
                assert!(rgba.chunks_exact(4).any(|p| p[3] > 0));
                assert!(rgba.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
                CustomCursor::from_rgba(rgba, size, size, size / 2, size / 2).unwrap();
            }
        }
    }
}
