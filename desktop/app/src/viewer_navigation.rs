//! Ephemeral canvas navigation. These coordinates never enter the project or undo history.
use motion_ui::input::Rect;

const MIN_ZOOM: f64 = 0.01;
const MAX_ZOOM: f64 = 64.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Mapping {
    pub image: Rect,
    pub scale: f64,
    origin: [f64; 2],
}
impl Mapping {
    /// The inverse used by viewer overlays, rulers, picking and snapping.
    pub fn screen(self, point: [f64; 2]) -> [f32; 2] {
        std::array::from_fn(|i| (self.origin[i] + point[i] * self.scale) as f32)
    }
    pub fn local(self, point: [f32; 2]) -> [f64; 2] {
        std::array::from_fn(|i| (point[i] as f64 - self.origin[i]) / self.scale)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Navigation {
    zoom: Option<f64>,
    center: Option<[f64; 2]>,
}
impl Navigation {
    pub fn is_fit(&self) -> bool {
        self.zoom.is_none()
    }
    pub fn fit(&mut self) {
        *self = Self::default();
    }
    pub fn mapping(&self, viewport: Rect, canvas: [f64; 2]) -> Option<Mapping> {
        if !canvas.iter().all(|v| v.is_finite() && *v > 0.)
            || !viewport
                .min
                .iter()
                .chain(&viewport.max)
                .all(|v| v.is_finite())
            || viewport.width() <= 0.
            || viewport.height() <= 0.
        {
            return None;
        }
        let scale = self.zoom.unwrap_or_else(|| {
            (viewport.width() as f64 / canvas[0]).min(viewport.height() as f64 / canvas[1])
        });
        let center = self.center.unwrap_or(canvas.map(|v| v * 0.5));
        let size = canvas.map(|v| (v * scale) as f32);
        let origin = std::array::from_fn::<_, 2, _>(|i| {
            (viewport.min[i] as f64 + viewport.max[i] as f64) * 0.5 - center[i] * scale
        });
        if !origin
            .iter()
            .all(|v| v.is_finite() && (*v as f32).is_finite())
            || !size.iter().all(|v| v.is_finite())
        {
            return None;
        }
        let image = Rect::new(origin[0] as f32, origin[1] as f32, size[0], size[1]);
        // Finite inputs can still collapse the f32 rectangle after a huge pan.
        // Reject the candidate before publishing an unusable view to the GPU.
        if !image.max.iter().all(|v| v.is_finite())
            || image.width() <= 0. || image.height() <= 0.
        { return None; }
        Some(Mapping {
            image,
            scale,
            origin,
        })
    }
    /// Returns false for invalid input without changing the last usable view.
    pub fn zoom_at(
        &mut self,
        viewport: Rect,
        canvas: [f64; 2],
        anchor: [f32; 2],
        zoom: f64,
    ) -> bool {
        if !zoom.is_finite() || zoom <= 0. || !anchor.iter().all(|v| v.is_finite()) {
            return false;
        }
        let Some(before) = self.mapping(viewport, canvas) else {
            return false;
        };
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let point = before.local(anchor);
        let center = std::array::from_fn(|i| {
            point[i]
                - (anchor[i] as f64 - (viewport.min[i] as f64 + viewport.max[i] as f64) * 0.5)
                    / zoom
        });
        let candidate = Self {
            zoom: Some(zoom),
            center: Some(center),
        };
        if candidate.mapping(viewport, canvas).is_none() {
            return false;
        }
        *self = candidate;
        true
    }
    pub fn pan(&mut self, viewport: Rect, canvas: [f64; 2], delta: [f32; 2]) -> bool {
        if !delta.iter().all(|v| v.is_finite()) {
            return false;
        }
        let Some(mapping) = self.mapping(viewport, canvas) else {
            return false;
        };
        let center = self.center.unwrap_or(canvas.map(|v| v * 0.5));
        let candidate = Self {
            center: Some(std::array::from_fn(|i| {
                center[i] - delta[i] as f64 / mapping.scale
            })),
            zoom: Some(mapping.scale),
        };
        if candidate.mapping(viewport, canvas).is_none() {
            return false;
        }
        *self = candidate;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_keeps_the_composition_point_under_the_pointer() {
        for viewport in [
            Rect::new(40., 80., 800., 500.),
            Rect::new(10., 30., 230., 700.),
        ] {
            let canvas = [1920., 1080.];
            let anchor = [viewport.min[0] + 47., viewport.min[1] + 130.];
            let mut nav = Navigation::default();
            let before = nav.mapping(viewport, canvas).unwrap().local(anchor);
            for zoom in [0.1, 1., 4., 64., 0.01] {
                assert!(nav.zoom_at(viewport, canvas, anchor, zoom));
                let after = nav.mapping(viewport, canvas).unwrap().local(anchor);
                for i in 0..2 {
                    assert!((before[i] - after[i]).abs() < 0.02, "{before:?} {after:?}");
                }
            }
        }
    }
    #[test]
    fn pan_resize_and_fit_are_independent_of_project_data() {
        let canvas = [640., 480.];
        let a = Rect::new(0., 0., 640., 480.);
        let b = Rect::new(0., 0., 320., 240.);
        let mut nav = Navigation::default();
        assert_eq!(nav.mapping(b, canvas).unwrap().scale, 0.5);
        assert!(nav.pan(a, canvas, [20., -30.]));
        assert_eq!(nav.mapping(a, canvas).unwrap().image.min, [20., -30.]);
        assert_eq!(nav.mapping(b, canvas).unwrap().scale, 1.);
        nav.pan(a, canvas, [-20., 30.]);
        assert_eq!(nav.mapping(a, canvas).unwrap().image.min, [0., 0.]);
        nav.fit();
        assert!(nav.is_fit());
        assert_eq!(nav.mapping(b, canvas).unwrap().scale, 0.5);
    }
    #[test]
    fn invalid_navigation_is_atomic_and_empty_views_are_normal() {
        let mut nav = Navigation::default();
        let initial = nav;
        let view = Rect::new(0., 0., 100., 100.);
        assert!(!nav.zoom_at(view, [100., 100.], [20., 20.], f64::NAN));
        assert!(!nav.pan(view, [100., 100.], [f32::INFINITY, 0.]));
        assert!(!nav.pan(view, [100., 100.], [f32::MAX, 0.]));
        assert!(!nav.pan(Rect::default(), [100., 100.], [1., 1.]));
        assert!(nav.mapping(view, [0., 100.]).is_none());
        assert_eq!(nav, initial);
    }
}
