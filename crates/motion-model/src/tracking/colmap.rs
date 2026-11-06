//! Bounded COLMAP text conversion. This pure boundary performs no IO or project
//! edits and retains both original documents for the caller's import boundary.
//! Camera-local axes are the documented right/down/forward convention; world
//! basis, scale gauge, frame identity, PTS and confidence are explicit inputs.
use super::{asset, plate, pose};
use glam::{DQuat, DVec3};
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    sync::Arc,
};

pub const PROFILE: &str = "colmap-cameras-images-text-v1";
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
pub const MAX_CAMERAS: usize = 4096;
pub const MAX_IMAGES: usize = 16_384;
pub const MAX_FRAMES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Cameras,
    Images,
}
#[derive(Debug)]
pub enum Cause {
    Invalid(&'static str),
    Calibration(plate::Error),
    Pose(asset::Error),
    Conversion(pose::Error),
    Asset(asset::Error),
}
#[derive(Debug)]
pub struct Error {
    pub input: Option<Input>,
    pub line: Option<usize>,
    pub frame: Option<u32>,
    pub cause: Cause,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tracking COLMAP input={:?} line={:?} frame={:?}: ",
            self.input, self.line, self.frame
        )?;
        match &self.cause {
            Cause::Invalid(reason) => f.write_str(reason),
            Cause::Calibration(cause) => cause.fmt(f),
            Cause::Pose(cause) | Cause::Asset(cause) => cause.fmt(f),
            Cause::Conversion(cause) => cause.fmt(f),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.cause {
            Cause::Calibration(cause) => Some(cause),
            Cause::Pose(cause) | Cause::Asset(cause) => Some(cause),
            Cause::Conversion(cause) => Some(cause),
            Cause::Invalid(_) => None,
        }
    }
}
fn error(cause: Cause) -> Error {
    Error {
        input: None,
        line: None,
        frame: None,
        cause,
    }
}
fn invalid(reason: &'static str) -> Error {
    error(Cause::Invalid(reason))
}
fn location(mut e: Error, input: Input, line: usize) -> Error {
    e.input = Some(input);
    e.line = Some(line);
    e
}
fn require(condition: bool, reason: &'static str) -> Result<(), Error> {
    if condition {
        Ok(())
    } else {
        Err(invalid(reason))
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub id: String,
    pub sha256: [u8; 32],
    pub extent: plate::Extent,
    pub ticks_per_second: u32,
    pub nominal_rate: asset::Rate,
    pub pts: Vec<i64>,
}
#[derive(Clone, Debug)]
pub struct Frame {
    /// Must equal its position in the complete source mapping; no sorting,
    /// interpolation or filename/frame-rate inference repairs an input.
    pub source_index: u32,
    pub image_name: String,
    /// Supplied by the caller: COLMAP text does not contain a confidence score.
    pub confidence: f64,
}
#[derive(Clone, Debug)]
pub struct Options {
    pub track_id: String,
    pub world_basis: pose::Basis,
    /// A declared scale gauge, not a metric accuracy claim about a reconstruction.
    pub source_meters_per_unit: f64,
    pub target_meters_per_unit: f64,
    pub target_origin: [f64; 3],
}
#[derive(Clone)]
pub struct Originals {
    cameras: Arc<[u8]>,
    images: Arc<[u8]>,
}
impl Originals {
    pub fn cameras(&self) -> &[u8] {
        &self.cameras
    }
    pub fn images(&self) -> &[u8] {
        &self.images
    }
}
impl fmt::Debug for Originals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Originals")
            .field("camera_bytes", &self.cameras.len())
            .field("image_bytes", &self.images.len())
            .finish()
    }
}
#[derive(Debug)]
pub struct Converted {
    pub asset: asset::Asset,
    pub originals: Originals,
    pub options: Options,
    /// Other registered photographs can calibrate a scene without belonging to
    /// this explicit video-frame mapping. They remain in the original documents.
    pub unmapped_images: usize,
}
struct ImagePose {
    line: usize,
    camera: u64,
    camera_to_world: asset::Pose,
}

/// Conversion only. Encoded-source identity must still be verified by the
/// existing source connector. The caller must retain `originals` and `options`;
/// this function does not publish a managed vendor-source attachment.
pub fn convert(
    cameras: &[u8],
    images: &[u8],
    source: Source,
    frames: &[Frame],
    options: Options,
) -> Result<Converted, Error> {
    require(
        cameras
            .len()
            .checked_add(images.len())
            .is_some_and(|n| n <= MAX_TEXT_BYTES),
        "combined text byte limit exceeded",
    )?;
    require(
        !frames.is_empty() && frames.len() <= MAX_FRAMES && frames.len() == source.pts.len(),
        "source mapping count differs or exceeds limit",
    )?;
    let conversion = pose::Conversion::new(
        options.world_basis,
        pose::Basis::RIGHT_DOWN_FORWARD,
        options.source_meters_per_unit,
        options.target_meters_per_unit,
        options.target_origin,
    )
    .map_err(|e| error(Cause::Conversion(e)))?;
    let mut names = HashSet::new();
    for (index, frame) in frames.iter().enumerate() {
        let result = (|| {
            require(
                frame.source_index as usize == index,
                "source frame mapping is not ordered and complete",
            )?;
            require(valid_name(&frame.image_name), "invalid frame image name")?;
            require(
                names.insert(frame.image_name.as_str()),
                "duplicate mapped image name",
            )?;
            require(
                frame.confidence.is_finite() && (0. ..=1.).contains(&frame.confidence),
                "invalid supplied confidence",
            )
        })();
        result.map_err(|mut e| {
            e.frame = Some(frame.source_index);
            e
        })?;
    }
    let mut asset = asset::Asset {
        version: asset::VERSION,
        source_id: source.id,
        source_sha256: source.sha256,
        source_extent: source.extent,
        coordinates: asset::Coordinates::RightDownForward,
        ticks_per_second: source.ticks_per_second,
        nominal_rate: source.nominal_rate,
        source_pts: source.pts,
        meters_per_world_unit: Some(options.target_meters_per_unit),
        tracks: vec![asset::Track {
            id: options.track_id.clone(),
            kind: asset::Kind::Camera,
            reference_index: None,
            samples: frames
                .iter()
                .map(|f| asset::Sample {
                    source_index: f.source_index,
                    visibility: asset::Visibility::Lost,
                    confidence: 0.,
                    observation: None,
                    manual_correction: false,
                })
                .collect(),
        }],
    };
    asset.validate().map_err(|e| error(Cause::Asset(e)))?;
    let camera_table = parse_cameras(cameras)?;
    let images_by_name = parse_images(images, &camera_table)?;
    let mut mapped = 0;
    for (frame, sample) in frames.iter().zip(&mut asset.tracks[0].samples) {
        let Some(image) = images_by_name.get(frame.image_name.as_str()) else {
            continue;
        };
        let calibration = camera_table[&image.camera];
        let result = (|| {
            require(
                calibration.source() == asset.source_extent,
                "registered camera extent differs from source",
            )?;
            let pose = conversion
                .pose(image.camera_to_world)
                .map_err(|e| error(Cause::Conversion(e)))?;
            Ok(asset::Observation::Camera {
                camera_to_world: pose,
                calibration,
            })
        })();
        let observation = result.map_err(|e| {
            let mut e = location(e, Input::Images, image.line);
            e.frame = Some(frame.source_index);
            e
        })?;
        sample.visibility = asset::Visibility::Visible;
        sample.confidence = frame.confidence;
        sample.observation = Some(observation);
        mapped += 1;
    }
    asset.validate().map_err(|e| error(Cause::Asset(e)))?;
    Ok(Converted {
        asset,
        originals: Originals {
            cameras: Arc::from(cameras),
            images: Arc::from(images),
        },
        options,
        unmapped_images: images_by_name.len() - mapped,
    })
}

fn text(bytes: &[u8], input: Input) -> Result<&str, Error> {
    std::str::from_utf8(bytes).map_err(|_| location(invalid("text is not UTF-8"), input, 1))
}
fn words(line: &str, maximum: usize) -> Result<Vec<&str>, Error> {
    require(
        line.len() <= MAX_LINE_BYTES,
        "text line byte limit exceeded",
    )?;
    let fields: Vec<_> = line.split_whitespace().take(maximum + 1).collect();
    require(fields.len() <= maximum, "unexpected text field count")?;
    Ok(fields)
}
fn number(value: &str) -> Result<f64, Error> {
    let n: f64 = value
        .parse()
        .map_err(|_| invalid("invalid numeric field"))?;
    require(n.is_finite(), "non-finite numeric field")?;
    Ok(n)
}
fn id(value: &str) -> Result<u64, Error> {
    value
        .parse()
        .map_err(|_| invalid("invalid unsigned identifier"))
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 1024
        && !name.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn parse_cameras(bytes: &[u8]) -> Result<BTreeMap<u64, plate::Calibration>, Error> {
    let mut result = BTreeMap::new();
    for (index, line) in text(bytes, Input::Cameras)?.lines().enumerate() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let parsed = (|| {
            let f = words(line, 12)?;
            require(f.len() >= 4, "camera fields are incomplete")?;
            require(result.len() < MAX_CAMERAS, "camera count limit exceeded")?;
            let key = id(f[0])?;
            require(!result.contains_key(&key), "duplicate camera identifier")?;
            let expected = match f[1] {
                "SIMPLE_PINHOLE" => 3,
                "PINHOLE" => 4,
                "SIMPLE_RADIAL" => 4,
                "RADIAL" => 5,
                "OPENCV" => 8,
                _ => return Err(invalid("unsupported camera model")),
            };
            require(f.len() == 4 + expected, "camera parameter count differs")?;
            let width = f[2].parse().map_err(|_| invalid("invalid camera width"))?;
            let height = f[3].parse().map_err(|_| invalid("invalid camera height"))?;
            let extent =
                plate::Extent::new(width, height).map_err(|e| error(Cause::Calibration(e)))?;
            let p: Vec<_> = f[4..].iter().map(|v| number(v)).collect::<Result<_, _>>()?;
            let (focal, principal, radial, tangential) = match f[1] {
                "SIMPLE_PINHOLE" => ([p[0]; 2], [p[1], p[2]], [0.; 3], [0.; 2]),
                "SIMPLE_RADIAL" => ([p[0]; 2], [p[1], p[2]], [p[3], 0., 0.], [0.; 2]),
                "RADIAL" => ([p[0]; 2], [p[1], p[2]], [p[3], p[4], 0.], [0.; 2]),
                "PINHOLE" => ([p[0], p[1]], [p[2], p[3]], [0.; 3], [0.; 2]),
                _ => ([p[0], p[1]], [p[2], p[3]], [p[4], p[5], 0.], [p[6], p[7]]),
            };
            let calibration = plate::Calibration::new(extent, focal, principal, radial, tangential)
                .map_err(|e| error(Cause::Calibration(e)))?;
            result.insert(key, calibration);
            Ok(())
        })();
        parsed.map_err(|e| location(e, Input::Cameras, index + 1))?;
    }
    require(!result.is_empty(), "camera table is empty")?;
    Ok(result)
}
fn parse_images(
    bytes: &[u8],
    cameras: &BTreeMap<u64, plate::Calibration>,
) -> Result<BTreeMap<String, ImagePose>, Error> {
    let mut result = BTreeMap::new();
    let mut identifiers = HashSet::new();
    let mut lines = text(bytes, Input::Images)?.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let parsed = (|| {
            let f = words(line, 10)?;
            require(f.len() == 10, "image pose fields are incomplete")?;
            require(result.len() < MAX_IMAGES, "image count limit exceeded")?;
            require(identifiers.insert(id(f[0])?), "duplicate image identifier")?;
            require(valid_name(f[9]), "invalid registered image name")?;
            require(
                !result.contains_key(f[9]),
                "duplicate registered image name",
            )?;
            let camera = id(f[8])?;
            require(
                cameras.contains_key(&camera),
                "image references an unknown camera",
            )?;
            let q = [number(f[2])?, number(f[3])?, number(f[4])?, number(f[1])?];
            let t = [number(f[5])?, number(f[6])?, number(f[7])?];
            asset::Pose {
                position: t,
                orientation_xyzw: q,
            }
            .validate()
            .map_err(|e| error(Cause::Pose(e)))?;
            let inverse = DQuat::from_array(q).normalize().conjugate();
            let camera_to_world = asset::Pose {
                position: (-(inverse * DVec3::from_array(t))).to_array(),
                orientation_xyzw: inverse.to_array(),
            };
            camera_to_world
                .validate()
                .map_err(|e| error(Cause::Pose(e)))?;
            let Some((points_index, points)) = lines.next() else {
                return Err(invalid("image keypoint line is missing"));
            };
            validate_keypoints(points).map_err(|e| location(e, Input::Images, points_index + 1))?;
            result.insert(
                f[9].to_owned(),
                ImagePose {
                    line: index + 1,
                    camera,
                    camera_to_world,
                },
            );
            Ok(())
        })();
        parsed.map_err(|e| {
            if e.input.is_some() {
                e
            } else {
                location(e, Input::Images, index + 1)
            }
        })?;
    }
    Ok(result)
}
fn validate_keypoints(line: &str) -> Result<(), Error> {
    require(
        line.len() <= MAX_LINE_BYTES,
        "keypoint line byte limit exceeded",
    )?;
    let mut fields = line.split_whitespace();
    while let Some(x) = fields.next() {
        number(x)?;
        number(
            fields
                .next()
                .ok_or_else(|| invalid("incomplete keypoint triple"))?,
        )?;
        let point = fields
            .next()
            .ok_or_else(|| invalid("incomplete keypoint triple"))?;
        if point != "-1" {
            id(point)?;
        }
    }
    Ok(())
}
