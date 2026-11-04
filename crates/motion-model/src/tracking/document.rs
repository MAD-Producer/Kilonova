//! Original tracking document admission. The optional `format` discriminator
//! selects an explicit vendor envelope; ordinary canonical JSON stays unchanged.
//! Vendor source text, mapping and conversion settings stay inside the managed
//! byte-exact document, independently of normalized runtime poses.
use super::{asset, colmap, decode, encode, plate, pose};
use crate::TrackingFormat;
use serde::{Deserialize, Serialize};
use std::{fmt, io::Read};

#[derive(Debug)]
pub enum Error {
    Canonical(decode::Error),
    Colmap(colmap::Error),
    Encode(encode::Error),
    Invalid(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonical(cause) => cause.fmt(f),
            Self::Colmap(cause) => cause.fmt(f),
            Self::Encode(cause) => cause.fmt(f),
            Self::Invalid(reason) => write!(f, "tracking original document: {reason}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Canonical(cause) => Some(cause),
            Self::Colmap(cause) => Some(cause),
            Self::Encode(cause) => Some(cause),
            Self::Invalid(_) => None,
        }
    }
}
pub struct Decoded {
    pub format: TrackingFormat,
    pub asset: asset::Asset,
}
/// Retains the frame-name map as well as byte-exact text and conversion options.
pub struct ColmapImport {
    pub converted: colmap::Converted,
    pub frames: Vec<colmap::Frame>,
}

fn bounded(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > decode::MAX_JSON_BYTES {
        Err(Error::Canonical(decode::Error::InputLimit))
    } else {
        Ok(())
    }
}
fn json(e: serde_json::Error) -> Error {
    Error::Canonical(decode::json_error(e))
}

/// Does not infer time, geometry, axes or units from a document filename.
pub fn from_slice(bytes: &[u8]) -> Result<Decoded, Error> {
    bounded(bytes)?;
    #[derive(Deserialize)]
    struct Header {
        format: Option<String>,
    }
    // Unknown content is skipped without constructing an unbounded JSON Value.
    let header: Header = serde_json::from_slice(bytes).map_err(json)?;
    match header.format.as_deref() {
        None => Ok(Decoded {
            format: TrackingFormat::MotionTrackingJsonV1,
            asset: decode::from_slice(bytes).map_err(Error::Canonical)?,
        }),
        Some(colmap::PROFILE) => Ok(Decoded {
            format: TrackingFormat::ColmapCameraTextV1,
            asset: colmap_from_slice(bytes)?.converted.asset,
        }),
        Some(_) => Err(Error::Invalid("unsupported format discriminator")),
    }
}
pub fn from_reader(reader: impl Read) -> Result<Decoded, Error> {
    let mut bytes = Vec::new();
    reader
        .take((decode::MAX_JSON_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Canonical(decode::Error::Read(e)))?;
    from_slice(&bytes)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    format: String,
    cameras: String,
    images: String,
    source: Source,
    frames: decode::List<Frame, { colmap::MAX_FRAMES }>,
    options: Options,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    sha256: [u8; 32],
    extent: [u32; 2],
    ticks_per_second: u32,
    nominal_rate: [u32; 2],
    pts: decode::List<i64, { colmap::MAX_FRAMES }>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    source_index: u32,
    image_name: String,
    confidence: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    track_id: String,
    world_basis: [i8; 3],
    source_meters_per_unit: f64,
    target_meters_per_unit: f64,
    target_origin: [f64; 3],
}

pub fn colmap_from_slice(bytes: &[u8]) -> Result<ColmapImport, Error> {
    bounded(bytes)?;
    let wire: Wire = serde_json::from_slice(bytes).map_err(json)?;
    if wire.format != colmap::PROFILE {
        return Err(Error::Invalid("unsupported format discriminator"));
    }
    let extent = plate::Extent::new(wire.source.extent[0], wire.source.extent[1]).map_err(|e| {
        Error::Colmap(colmap::Error {
            input: None,
            line: None,
            frame: None,
            cause: colmap::Cause::Calibration(e),
        })
    })?;
    let rate = asset::Rate::new(wire.source.nominal_rate[0], wire.source.nominal_rate[1]).map_err(
        |e| {
            Error::Colmap(colmap::Error {
                input: None,
                line: None,
                frame: None,
                cause: colmap::Cause::Asset(e),
            })
        },
    )?;
    let basis = pose::Basis::new(wire.options.world_basis).map_err(|e| {
        Error::Colmap(colmap::Error {
            input: None,
            line: None,
            frame: None,
            cause: colmap::Cause::Conversion(e),
        })
    })?;
    let frames: Vec<_> = wire
        .frames
        .0
        .into_iter()
        .map(|f| colmap::Frame {
            source_index: f.source_index,
            image_name: f.image_name,
            confidence: f.confidence,
        })
        .collect();
    let converted = colmap::convert(
        wire.cameras.as_bytes(),
        wire.images.as_bytes(),
        colmap::Source {
            id: wire.source.id,
            sha256: wire.source.sha256,
            extent,
            ticks_per_second: wire.source.ticks_per_second,
            nominal_rate: rate,
            pts: wire.source.pts.0,
        },
        &frames,
        colmap::Options {
            track_id: wire.options.track_id,
            world_basis: basis,
            source_meters_per_unit: wire.options.source_meters_per_unit,
            target_meters_per_unit: wire.options.target_meters_per_unit,
            target_origin: wire.options.target_origin,
        },
    )
    .map_err(Error::Colmap)?;
    Ok(ColmapImport { converted, frames })
}

/// Validate the complete conversion before writing a bounded vendor envelope.
/// Original UTF-8 text is JSON-escaped, never normalized or reserialized as poses.
pub fn colmap_to_vec(
    cameras: &[u8],
    images: &[u8],
    source: colmap::Source,
    frames: &[colmap::Frame],
    options: colmap::Options,
) -> Result<Vec<u8>, Error> {
    colmap::convert(cameras, images, source.clone(), frames, options.clone())
        .map_err(Error::Colmap)?;
    let text = |bytes: &[u8]| {
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::Invalid("original text is not UTF-8"))
    };
    let wire = Wire {
        format: colmap::PROFILE.into(),
        cameras: text(cameras)?,
        images: text(images)?,
        source: Source {
            id: source.id,
            sha256: source.sha256,
            extent: source.extent.pixels(),
            ticks_per_second: source.ticks_per_second,
            nominal_rate: source.nominal_rate.ratio(),
            pts: decode::List(source.pts),
        },
        frames: decode::List(
            frames
                .iter()
                .map(|f| Frame {
                    source_index: f.source_index,
                    image_name: f.image_name.clone(),
                    confidence: f.confidence,
                })
                .collect(),
        ),
        options: Options {
            track_id: options.track_id,
            world_basis: options.world_basis.rows(),
            source_meters_per_unit: options.source_meters_per_unit,
            target_meters_per_unit: options.target_meters_per_unit,
            target_origin: options.target_origin,
        },
    };
    encode::bounded_json(&wire, decode::MAX_JSON_BYTES).map_err(Error::Encode)
}
