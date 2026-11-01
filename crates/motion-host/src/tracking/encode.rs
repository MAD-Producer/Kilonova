//! Versioned tracking JSON output, without project or filesystem mutation.
use super::{
    asset::{self, *},
    decode::MAX_JSON_BYTES,
    plate::Calibration,
};
use serde::{Serialize, Serializer};
use std::{
    fmt,
    io::{self, Write},
};

#[derive(Debug)]
pub enum Error {
    InvalidAsset(asset::Error),
    OutputLimit,
    Serialization(serde_json::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAsset(cause) => write!(f, "tracking serialization rejected: {cause}"),
            Self::OutputLimit => write!(
                f,
                "tracking JSON output exceeds {MAX_JSON_BYTES} byte limit"
            ),
            Self::Serialization(cause) => write!(f, "tracking serialization failed: {cause}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidAsset(cause) => Some(cause),
            Self::Serialization(cause) => Some(cause),
            Self::OutputLimit => None,
        }
    }
}
/// Validate first, then serialize borrowed tracks directly into bounded bytes.
/// A valid typed asset can exceed the JSON transport limit; that is an explicit
/// error, never truncation or silent loss of observations.
pub fn to_vec(asset: &Asset) -> Result<Vec<u8>, Error> {
    asset.validate().map_err(Error::InvalidAsset)?;
    let mut digest = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in asset.source_sha256 {
        digest.push(HEX[(byte >> 4) as usize] as char);
        digest.push(HEX[(byte & 15) as usize] as char);
    }
    let wire = Source {
        version: asset.version,
        source_id: &asset.source_id,
        source_sha256: &digest,
        source_extent: asset.source_extent.pixels(),
        coordinates: "right_down_forward",
        ticks_per_second: asset.ticks_per_second,
        nominal_rate: asset.nominal_rate.ratio(),
        source_pts: &asset.source_pts,
        meters_per_world_unit: asset.meters_per_world_unit,
        tracks: Tracks(&asset.tracks),
    };
    bounded_json(&wire, MAX_JSON_BYTES)
}
struct Output {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(io::Error::other("tracking JSON output limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn bounded_json(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, Error> {
    let mut out = Output {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    match serde_json::to_writer(&mut out, value) {
        Ok(()) => Ok(out.bytes),
        Err(_) if out.exceeded => Err(Error::OutputLimit),
        Err(cause) => Err(Error::Serialization(cause)),
    }
}
#[derive(Serialize)]
struct Source<'a> {
    version: u32,
    source_id: &'a str,
    source_sha256: &'a str,
    source_extent: [u32; 2],
    coordinates: &'static str,
    ticks_per_second: u32,
    nominal_rate: [u32; 2],
    source_pts: &'a [i64],
    meters_per_world_unit: Option<f64>,
    tracks: Tracks<'a>,
}
struct Tracks<'a>(&'a [Track]);
impl Serialize for Tracks<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(|track| TrackRef {
            id: &track.id,
            kind: match track.kind {
                Kind::Point => "point",
                Kind::Plane => "plane",
                Kind::Camera => "camera",
            },
            reference_index: track.reference_index,
            samples: Samples(&track.samples),
        }))
    }
}
#[derive(Serialize)]
struct TrackRef<'a> {
    id: &'a str,
    kind: &'static str,
    reference_index: Option<u32>,
    samples: Samples<'a>,
}
struct Samples<'a>(&'a [Sample]);
impl Serialize for Samples<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(|sample| SampleRef {
            source_index: sample.source_index,
            visibility: match sample.visibility {
                Visibility::Visible => "visible",
                Visibility::Occluded => "occluded",
                Visibility::Lost => "lost",
            },
            confidence: sample.confidence,
            manual_correction: sample.manual_correction,
            observation: sample.observation.map(|observation| match observation {
                Observation::Point(pixels) => Geometry::Point { pixels },
                Observation::Plane(matrix) => Geometry::Plane { matrix },
                Observation::Camera {
                    camera_to_world,
                    calibration,
                } => Geometry::Camera {
                    camera_to_world: CameraPose {
                        position: camera_to_world.position,
                        orientation_xyzw: camera_to_world.orientation_xyzw,
                    },
                    calibration: Lens::from(calibration),
                },
            }),
        }))
    }
}
#[derive(Serialize)]
struct SampleRef {
    source_index: u32,
    visibility: &'static str,
    confidence: f64,
    manual_correction: bool,
    observation: Option<Geometry>,
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Geometry {
    Point {
        pixels: [f64; 2],
    },
    Plane {
        matrix: [[f64; 3]; 3],
    },
    Camera {
        camera_to_world: CameraPose,
        calibration: Lens,
    },
}
#[derive(Serialize)]
struct CameraPose {
    position: [f64; 3],
    orientation_xyzw: [f64; 4],
}
#[derive(Serialize)]
struct Lens {
    source_extent: [u32; 2],
    focal: [f64; 2],
    principal: [f64; 2],
    radial: [f64; 3],
    tangential: [f64; 2],
}
impl From<Calibration> for Lens {
    fn from(c: Calibration) -> Self {
        Self {
            source_extent: c.source().pixels(),
            focal: c.focal(),
            principal: c.principal(),
            radial: c.radial(),
            tangential: c.tangential(),
        }
    }
}
#[cfg(test)]
#[path = "encode_tests.rs"]
mod tests;
