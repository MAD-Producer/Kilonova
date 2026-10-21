//! Pinhole projection: horizontal film gate, vertical FOV, square composition pixels.
//! Focal length and pixel zoom are derived views of the same animated FOV.
use crate::{ensure, Result};

pub const MIN_FILM_WIDTH_MM: f64 = 0.1;
pub const MAX_FILM_WIDTH_MM: f64 = 1000.;
pub fn default_film_width() -> f32 {
    36.
}
pub fn default_gate(value: &f32) -> bool {
    *value == default_film_width()
}
pub fn validate_film_width(value: f64) -> Result<()> {
    ensure(
        value.is_finite() && (MIN_FILM_WIDTH_MM..=MAX_FILM_WIDTH_MM).contains(&value),
        "film width must be 0.1..1000 mm",
    )
}

#[derive(Clone, Copy, Debug)]
pub struct Lens {
    pub film_width_mm: f64,
    pub focal_length_mm: f64,
    pub zoom_px: f64,
    pub vertical_fov_degrees: f64,
}
impl Lens {
    pub fn from_fov(
        film_width_mm: f64,
        vertical_fov_degrees: f64,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        validate_film_width(film_width_mm)?;
        ensure(
            width > 0 && height > 0,
            "camera lens requires nonzero composition dimensions",
        )?;
        ensure(
            vertical_fov_degrees.is_finite() && (10. ..=120.).contains(&vertical_fov_degrees),
            "field of view must be 10..120 degrees",
        )?;
        let zoom_px = f64::from(height) / (2. * (vertical_fov_degrees.to_radians() * 0.5).tan());
        Ok(Self {
            film_width_mm,
            vertical_fov_degrees,
            zoom_px,
            focal_length_mm: zoom_px * film_width_mm / f64::from(width),
        })
    }
    pub fn from_focal_length(
        film_width_mm: f64,
        focal_length_mm: f64,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        validate_film_width(film_width_mm)?;
        ensure(
            width > 0 && height > 0,
            "camera lens requires nonzero composition dimensions",
        )?;
        ensure(
            focal_length_mm.is_finite() && focal_length_mm > 0.,
            "focal length must be finite and positive",
        )?;
        let zoom = focal_length_mm * f64::from(width) / film_width_mm;
        Self::from_zoom(film_width_mm, zoom, width, height)
    }
    pub fn from_zoom(film_width_mm: f64, zoom_px: f64, width: u32, height: u32) -> Result<Self> {
        ensure(
            zoom_px.is_finite() && zoom_px > 0.,
            "camera zoom must be finite and positive",
        )?;
        let fov = (2. * (f64::from(height) / (2. * zoom_px)).atan()).to_degrees();
        // The renderer stores FOV as f32. A focal value rounded by a numeric
        // editor may fall just beyond an endpoint while representing exactly
        // that endpoint at the projection's storage precision. Preserve f64
        // precision elsewhere; never admit a distinct out-of-range f32 FOV.
        let fov = if (10_f32..=120.).contains(&(fov as f32)) {
            fov.clamp(10., 120.)
        } else {
            fov
        };
        Self::from_fov(film_width_mm, fov, width, height)
    }
}
