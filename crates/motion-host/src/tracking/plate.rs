//! Explicit calibrated projection and crop/rotation/resize mapping for plates.
//! These functions never clamp points into the image or silently discard lens
//! distortion. Callers retain visibility/confidence and source-time metadata.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidExtent,
    InvalidCalibration,
    InvalidCrop,
    NonFinitePoint,
    BehindCamera,
    NonFiniteProjection,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidExtent => "tracking source/output extent is invalid",
            Self::InvalidCalibration => "tracking camera calibration is invalid",
            Self::InvalidCrop => "tracking crop must be finite and inside its source extent",
            Self::NonFinitePoint => "tracking point contains a non-finite coordinate",
            Self::BehindCamera => "tracking point is on or behind the camera plane",
            Self::NonFiniteProjection => "tracking projection exceeded finite numeric range",
        })
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    width: u32,
    height: u32,
}
impl Extent {
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        if width == 0 || height == 0 || width > 32_768 || height > 32_768 {
            return Err(Error::InvalidExtent);
        }
        Ok(Self { width, height })
    }
    pub fn pixels(self) -> [u32; 2] {
        [self.width, self.height]
    }
}

/// Pinhole intrinsics with Brown-Conrady radial (k1,k2,k3) and tangential
/// (p1,p2) distortion in normalized camera coordinates. This is a named model,
/// not a generic vector accepting arbitrary external lens coefficients.
#[derive(Clone, Copy, Debug)]
pub struct Calibration {
    source: Extent,
    focal: [f64; 2],
    principal: [f64; 2],
    radial: [f64; 3],
    tangential: [f64; 2],
}
impl Calibration {
    pub fn new(
        source: Extent,
        focal: [f64; 2],
        principal: [f64; 2],
        radial: [f64; 3],
        tangential: [f64; 2],
    ) -> Result<Self, Error> {
        if focal
            .iter()
            .chain(&principal)
            .chain(&radial)
            .chain(&tangential)
            .any(|x| !x.is_finite())
            || focal.iter().any(|x| *x <= 0.)
        {
            return Err(Error::InvalidCalibration);
        }
        Ok(Self {
            source,
            focal,
            principal,
            radial,
            tangential,
        })
    }
    pub fn source(self) -> Extent {
        self.source
    }
    pub fn focal(self) -> [f64; 2] {
        self.focal
    }
    pub fn principal(self) -> [f64; 2] {
        self.principal
    }
    pub fn radial(self) -> [f64; 3] {
        self.radial
    }
    pub fn tangential(self) -> [f64; 2] {
        self.tangential
    }

    /// Project a camera-space point. BehindCamera is a visibility disposition,
    /// not evidence of a failed import. Points outside the image stay outside.
    pub fn project(self, point: [f64; 3]) -> Result<[f64; 2], Error> {
        if point.iter().any(|x| !x.is_finite()) {
            return Err(Error::NonFinitePoint);
        }
        if point[2] <= 0. {
            return Err(Error::BehindCamera);
        }
        let x = point[0] / point[2];
        let y = point[1] / point[2];
        let r2 = x * x + y * y;
        let [k1, k2, k3] = self.radial;
        let [p1, p2] = self.tangential;
        let radial = 1. + r2 * (k1 + r2 * (k2 + r2 * k3));
        let dx = x * radial + 2. * p1 * x * y + p2 * (r2 + 2. * x * x);
        let dy = y * radial + p1 * (r2 + 2. * y * y) + 2. * p2 * x * y;
        finite([
            self.focal[0] * dx + self.principal[0],
            self.focal[1] * dy + self.principal[1],
        ])
    }
    /// Euclidean residual in source pixels, before plate output transforms.
    pub fn residual(self, point: [f64; 3], observed: [f64; 2]) -> Result<f64, Error> {
        if observed.iter().any(|x| !x.is_finite()) {
            return Err(Error::NonFinitePoint);
        }
        let projected = self.project(point)?;
        let residual = (projected[0] - observed[0]).hypot(projected[1] - observed[1]);
        if !residual.is_finite() {
            return Err(Error::NonFiniteProjection);
        }
        Ok(residual)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    None,
    Clockwise90,
    HalfTurn,
    Clockwise270,
}

/// Crop in original source edge coordinates, then rotate clockwise, then resize.
/// This is independent of camera axes, lens distortion, source PTS and project fps.
#[derive(Clone, Copy, Debug)]
pub struct PlateTransform {
    source: Extent,
    crop: [f64; 4],
    rotation: Rotation,
    output: Extent,
}
impl PlateTransform {
    pub fn new(
        source: Extent,
        crop: [f64; 4],
        rotation: Rotation,
        output: Extent,
    ) -> Result<Self, Error> {
        let [x, y, w, h] = crop;
        if crop.iter().any(|x| !x.is_finite())
            || x < 0.
            || y < 0.
            || w <= 0.
            || h <= 0.
            || x + w > f64::from(source.width)
            || y + h > f64::from(source.height)
        {
            return Err(Error::InvalidCrop);
        }
        Ok(Self {
            source,
            crop,
            rotation,
            output,
        })
    }
    pub fn source(self) -> Extent {
        self.source
    }
    pub fn output(self) -> Extent {
        self.output
    }
    fn rotated_extent(self) -> [f64; 2] {
        let [_, _, w, h] = self.crop;
        match self.rotation {
            Rotation::None | Rotation::HalfTurn => [w, h],
            Rotation::Clockwise90 | Rotation::Clockwise270 => [h, w],
        }
    }
    pub fn apply(self, point: [f64; 2]) -> Result<[f64; 2], Error> {
        if point.iter().any(|x| !x.is_finite()) {
            return Err(Error::NonFinitePoint);
        }
        let [left, top, w, h] = self.crop;
        let [x, y] = [point[0] - left, point[1] - top];
        let [u, v] = match self.rotation {
            Rotation::None => [x, y],
            Rotation::Clockwise90 => [h - y, x],
            Rotation::HalfTurn => [w - x, h - y],
            Rotation::Clockwise270 => [y, w - x],
        };
        let [rw, rh] = self.rotated_extent();
        finite([
            u / rw * f64::from(self.output.width),
            v / rh * f64::from(self.output.height),
        ])
    }
    pub fn inverse(self, point: [f64; 2]) -> Result<[f64; 2], Error> {
        if point.iter().any(|x| !x.is_finite()) {
            return Err(Error::NonFinitePoint);
        }
        let [rw, rh] = self.rotated_extent();
        let [u, v] = [
            point[0] / f64::from(self.output.width) * rw,
            point[1] / f64::from(self.output.height) * rh,
        ];
        let [left, top, w, h] = self.crop;
        let [x, y] = match self.rotation {
            Rotation::None => [u, v],
            Rotation::Clockwise90 => [v, h - u],
            Rotation::HalfTurn => [w - u, h - v],
            Rotation::Clockwise270 => [w - v, u],
        };
        finite([x + left, y + top])
    }
}
fn finite(point: [f64; 2]) -> Result<[f64; 2], Error> {
    if point.iter().all(|x| x.is_finite()) {
        Ok(point)
    } else {
        Err(Error::NonFiniteProjection)
    }
}

#[cfg(test)]
#[path = "plate_tests.rs"]
mod tests;
