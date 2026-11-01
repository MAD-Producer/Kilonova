//! Checked inverse Brown-Conrady mapping for a separate spatial raster.
//! The forward model is documented at https://docs.opencv.org/4.10.0/d9/d0c/group__calib3d.html
//! No upstream implementation is copied. Admission proves a contraction over
//! the entire requested image, rather than assuming a sampled grid cannot fold.
use super::plate::{Calibration, PlateTransform};
use crate::{ensure, Error, Result};

pub const INVERSE_ITERATIONS: usize = 48;
const MAX_DERIVATIVE: f64 = 0.45;
const MAX_COORDINATE: f64 = 1_000_000.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensWarp {
    calibration: Calibration,
    plate: PlateTransform,
    zoom: f64,
    radius: f64,
    derivative_bound: f64,
    bounds: [f64; 4], // min x/y, max x/y in undistorted composition pixels
}

/// The offscreen producer uses this rectangle in composition pixels and this
/// physical raster size. Guard texels are part of the admitted allocation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensRaster {
    rect: [f64; 4],
    size: [u32; 2],
}
impl LensRaster {
    pub fn rect(self) -> [f64; 4] {
        self.rect
    }
    pub fn size(self) -> [u32; 2] {
        self.size
    }
    /// Pre-multiply the ordinary full-composition pinhole projection by this
    /// matrix. It changes only the raster window, not camera/world/depth units.
    pub fn projection(self, output: [u32; 2]) -> Result<glam::Mat4> {
        ensure(
            output.iter().all(|&v| v > 0 && v <= 32768),
            "lens output extent invalid",
        )?;
        let [w, h] = output.map(f64::from);
        let [x, y, rw, rh] = self.rect;
        Ok(glam::DMat4::from_cols_array(&[
            w / rw,
            0.,
            0.,
            0.,
            0.,
            h / rh,
            0.,
            0.,
            0.,
            0.,
            1.,
            0.,
            (w - 2. * x - rw) / rw,
            (rh - h + 2. * y) / rh,
            0.,
            1.,
        ])
        .as_mat4())
    }
}

impl LensWarp {
    pub fn new(calibration: Calibration, plate: PlateTransform, zoom: f64) -> Result<Self> {
        ensure(
            calibration.source() == plate.source(),
            "lens source/calibration extent mismatch",
        )?;
        ensure(
            zoom.is_finite() && zoom > 0.,
            "lens focal multiplier must be finite and positive",
        )?;
        ensure(
            calibration
                .radial()
                .iter()
                .chain(&calibration.tangential())
                .all(|&v| (v as f32).is_finite()),
            "lens coefficients exceed finite shader range",
        )?;
        let focal = calibration.focal().map(|v| v * zoom);
        ensure(
            focal
                .iter()
                .all(|v| v.is_finite() && (0.001..=1e9).contains(v))
                && calibration
                    .principal()
                    .iter()
                    .all(|v| v.abs() <= MAX_COORDINATE),
            "lens focal/principal exceeds GPU numeric admission range",
        )?;
        let [w, h] = plate.output().pixels().map(f64::from);
        let mut domain = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut d = 0f64;
        for p in [[0., 0.], [w, 0.], [w, h], [0., h]] {
            let p = plate.inverse(p).map_err(invalid)?;
            let q =
                std::array::from_fn::<_, 2, _>(|i| (p[i] - calibration.principal()[i]) / focal[i]);
            ensure(
                q.iter().all(|v| v.is_finite()),
                "lens normalized domain is not finite",
            )?;
            d = d.max(q[0].hypot(q[1]));
            for i in 0..2 {
                domain[i] = domain[i].min(q[i]);
                domain[i + 2] = domain[i + 2].max(q[i]);
            }
        }
        let radius = 2. * d + 1e-12;
        ensure(
            radius.is_finite() && radius <= 10_000.,
            "lens inverse domain exceeds numeric range",
        )?;
        let r2 = radius * radius;
        let [k1, k2, k3] = calibration.radial();
        let [p1, p2] = calibration.tangential();
        // ||J(F-I)||_2 <= sum (2n+1)|kn|R^(2n) + 8(|p1|+|p2|)R.
        // Therefore F=I+g is injective and its inverse is 1/(1-B) Lipschitz.
        // H(q)=d-g(q) maps the radius-R disk to itself since |d|<=R/2
        // and B<0.45. Fixed-point iteration has one bounded, unique limit.
        let bound = 3. * k1.abs() * r2
            + 5. * k2.abs() * r2 * r2
            + 7. * k3.abs() * r2 * r2 * r2
            + 8. * (p1.abs() + p2.abs()) * radius;
        ensure(bound.is_finite() && bound<MAX_DERIVATIVE,
            &format!("lens inverse cannot certify a unique domain derivative_bound={bound} limit={MAX_DERIVATIVE} radius={radius}"))?;
        let mut result = Self {
            calibration,
            plate,
            zoom,
            radius,
            derivative_bound: bound,
            bounds: [0., 0., w, h],
        };
        if !result.is_identity() {
            // A finite grid alone is not a coverage proof. Every point in each
            // cell is within cell_radius of its centre; inverse Lipschitz gives
            // a conservative radius enclosing the complete inverse cell.
            let steps = 4.;
            let dx = (domain[2] - domain[0]) / steps;
            let dy = (domain[3] - domain[1]) / steps;
            let error = 0.5 * dx.hypot(dy) / (1. - bound);
            let mut qbounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for y in 0..4 {
                for x in 0..4 {
                    let q = result.inverse_normalized([
                        domain[0] + (x as f64 + 0.5) * dx,
                        domain[1] + (y as f64 + 0.5) * dy,
                    ])?;
                    for i in 0..2 {
                        qbounds[i] = qbounds[i].min(q[i] - error);
                        qbounds[i + 2] = qbounds[i + 2].max(q[i] + error);
                    }
                }
            }
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for q in [
                [qbounds[0], qbounds[1]],
                [qbounds[2], qbounds[1]],
                [qbounds[2], qbounds[3]],
                [qbounds[0], qbounds[3]],
            ] {
                let p = result.ideal_pixel(q)?;
                for i in 0..2 {
                    bounds[i] = bounds[i].min(p[i]);
                    bounds[i + 2] = bounds[i + 2].max(p[i]);
                }
            }
            ensure(
                bounds
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE),
                "lens overscan bounds exceed numeric range",
            )?;
            result.bounds = bounds;
        }
        Ok(result)
    }
    pub fn calibration(self) -> Calibration {
        self.calibration
    }
    pub fn plate(self) -> PlateTransform {
        self.plate
    }
    pub fn zoom(self) -> f64 {
        self.zoom
    }
    pub fn radius(self) -> f64 {
        self.radius
    }
    pub fn derivative_bound(self) -> f64 {
        self.derivative_bound
    }
    pub fn is_identity(self) -> bool {
        self.calibration.radial() == [0.; 3] && self.calibration.tangential() == [0.; 2]
    }
    pub fn output_size(self) -> [u32; 2] {
        self.plate.output().pixels()
    }
    pub fn raster(self, scale: f64) -> Result<LensRaster> {
        ensure(
            scale.is_finite() && (0.001..=16.).contains(&scale),
            "lens raster scale outside 0.001..16",
        )?;
        if self.is_identity() {
            let [w, h] = self.output_size();
            return checked_raster(
                [0., 0., w as f64, h as f64],
                [(w as f64 * scale).ceil(), (h as f64 * scale).ceil()],
            );
        }
        let lo = [
            (self.bounds[0] * scale).floor() - 2.,
            (self.bounds[1] * scale).floor() - 2.,
        ];
        let hi = [
            (self.bounds[2] * scale).ceil() + 2.,
            (self.bounds[3] * scale).ceil() + 2.,
        ];
        checked_raster(
            [
                lo[0] / scale,
                lo[1] / scale,
                (hi[0] - lo[0]) / scale,
                (hi[1] - lo[1]) / scale,
            ],
            [hi[0] - lo[0], hi[1] - lo[1]],
        )
    }
    pub fn validate_raster(self, raster: LensRaster) -> Result<()> {
        let [x, y, w, h] = raster.rect;
        ensure(
            x <= self.bounds[0]
                && y <= self.bounds[1]
                && x + w >= self.bounds[2]
                && y + h >= self.bounds[3],
            "lens raster does not cover the certified inverse footprint",
        )
    }
    fn ideal_pixel(self, q: [f64; 2]) -> Result<[f64; 2]> {
        let f = self.calibration.focal();
        let c = self.calibration.principal();
        self.plate
            .apply([
                f[0] * self.zoom * q[0] + c[0],
                f[1] * self.zoom * q[1] + c[1],
            ])
            .map_err(invalid)
    }
    fn normalized(self, p: [f64; 2]) -> Result<[f64; 2]> {
        let p = self.plate.inverse(p).map_err(invalid)?;
        let f = self.calibration.focal();
        let c = self.calibration.principal();
        Ok([
            (p[0] - c[0]) / (f[0] * self.zoom),
            (p[1] - c[1]) / (f[1] * self.zoom),
        ])
    }
    /// Undistorted composition pixels to distorted composition pixels. Outside
    /// the certified camera domain is an explicit disposition, never clamping.
    pub fn forward(self, point: [f64; 2]) -> Result<[f64; 2]> {
        let q = self.normalized(point)?;
        ensure(
            q[0].hypot(q[1]) <= self.radius,
            "lens point outside certified camera domain",
        )?;
        let g = self.displacement(q);
        self.ideal_pixel([q[0] + g[0], q[1] + g[1]])
    }
    /// Distorted output pixel to the undistorted spatial producer's pixel.
    pub fn inverse(self, point: [f64; 2]) -> Result<[f64; 2]> {
        let [w, h] = self.output_size();
        ensure(
            point.iter().all(|v| v.is_finite())
                && (0. ..=w as f64).contains(&point[0])
                && (0. ..=h as f64).contains(&point[1]),
            "lens inverse point outside output rectangle",
        )?;
        self.ideal_pixel(self.inverse_normalized(self.normalized(point)?)?)
    }
    /// View-plane manipulation can cross the image rectangle while remaining
    /// inside the same proven contraction disk. This does not extend the raster
    /// footprint or permit inversion outside the certified normalized domain.
    pub fn inverse_in_certified_domain(self, point: [f64; 2]) -> Result<[f64; 2]> {
        ensure(
            point.iter().all(|v| v.is_finite()),
            "lens inverse point is not finite",
        )?;
        let d = self.normalized(point)?;
        ensure(
            d[0].hypot(d[1]) <= self.radius * 0.5,
            "lens inverse point outside certified camera domain",
        )?;
        self.ideal_pixel(self.inverse_normalized(d)?)
    }
    fn inverse_normalized(self, d: [f64; 2]) -> Result<[f64; 2]> {
        let mut q = d;
        for _ in 0..INVERSE_ITERATIONS {
            let g = self.displacement(q);
            q = [d[0] - g[0], d[1] - g[1]];
        }
        let g = self.displacement(q);
        ensure(
            q.iter().all(|v| v.is_finite())
                && q[0].hypot(q[1]) <= self.radius
                && (q[0] + g[0] - d[0]).hypot(q[1] + g[1] - d[1])
                    <= 1e-10 * (1. + d[0].hypot(d[1])),
            "lens inverse did not converge within its certified domain",
        )?;
        Ok(q)
    }
    fn displacement(self, [x, y]: [f64; 2]) -> [f64; 2] {
        let r2 = x * x + y * y;
        let [k1, k2, k3] = self.calibration.radial();
        let [p1, p2] = self.calibration.tangential();
        let r = r2 * (k1 + r2 * (k2 + r2 * k3));
        [
            x * r + 2. * p1 * x * y + p2 * (r2 + 2. * x * x),
            y * r + p1 * (r2 + 2. * y * y) + 2. * p2 * x * y,
        ]
    }
}
fn checked_raster(rect: [f64; 4], size: [f64; 2]) -> Result<LensRaster> {
    ensure(
        size.iter()
            .all(|v| v.is_finite() && (1. ..=32768.).contains(v))
            && size[0] * size[1] <= 16. * 1024. * 1024.,
        "lens overscan raster exceeds side or 16-megapixel capacity",
    )?;
    Ok(LensRaster {
        rect,
        size: size.map(|v| v as u32),
    })
}
fn invalid(cause: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("lens geometry: {cause}"))
}
