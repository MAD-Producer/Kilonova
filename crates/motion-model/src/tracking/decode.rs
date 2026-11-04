//! Strict bounded external tracking JSON, without project/filesystem mutation.
use super::{
    asset::*,
    plate::{self, Calibration, Extent},
};
use serde::{
    de::{self, IgnoredAny, SeqAccess, Visitor},
    Deserialize,
};
use std::{fmt, io::Read, marker::PhantomData};

pub const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;
#[derive(Debug)]
pub enum Error {
    InputLimit,
    Read(std::io::Error),
    /// Deliberately excludes unknown user keys/values and source text. Location
    /// and structural cause are retained without a credential-bearing payload.
    Json {
        reason: &'static str,
        line: usize,
        column: usize,
    },
    Field(&'static str),
    Geometry {
        track: Option<usize>,
        sample: Option<usize>,
        cause: plate::Error,
    },
    Asset(super::asset::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit => write!(f, "tracking JSON exceeds {MAX_JSON_BYTES} byte limit"),
            Self::Read(e) => write!(f, "tracking input read failed: {e}"),
            Self::Json {
                reason,
                line,
                column,
            } => write!(f, "tracking JSON {reason} line={line} column={column}"),
            Self::Field(field) => write!(f, "tracking metadata invalid field={field}"),
            Self::Geometry {
                track,
                sample,
                cause,
            } => write!(
                f,
                "tracking calibration track={track:?} sample={sample:?}: {cause}"
            ),
            Self::Asset(cause) => cause.fmt(f),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(e) => Some(e),
            Self::Geometry { cause, .. } => Some(cause),
            Self::Asset(e) => Some(e),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for Asset {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireAsset::deserialize(deserializer)?
            .into_asset()
            .map_err(de::Error::custom)
    }
}
pub fn from_slice(bytes: &[u8]) -> Result<Asset, Error> {
    if bytes.len() > MAX_JSON_BYTES {
        return Err(Error::InputLimit);
    }
    let wire: WireAsset = serde_json::from_slice(bytes).map_err(json_error)?;
    wire.into_asset()
}
pub fn from_reader(reader: impl Read) -> Result<Asset, Error> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_JSON_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    from_slice(&bytes)
}
fn json_error(e: serde_json::Error) -> Error {
    let reason = match e.classify() {
        serde_json::error::Category::Io => "input failure",
        serde_json::error::Category::Eof => "incomplete input",
        serde_json::error::Category::Syntax => "invalid syntax",
        serde_json::error::Category::Data => {
            // Only select fixed structural labels. Never retain the parser's
            // potentially large, user-controlled unknown field/enum text.
            let message = e.to_string();
            if message.starts_with("missing field") {
                "missing required field"
            } else if message.starts_with("duplicate field") {
                "duplicate field"
            } else if message.starts_with("unknown field") {
                "unknown field"
            } else if message.starts_with("unknown variant") {
                "unsupported enum value"
            } else if message.starts_with("tracking sequence limit") {
                "sequence limit exceeded"
            } else if message.starts_with("tracking string limit") {
                "identifier length exceeded"
            } else {
                "incorrect value type or shape"
            }
        }
    };
    Error::Json {
        reason,
        line: e.line(),
        column: e.column(),
    }
}

struct List<T, const LIMIT: usize>(Vec<T>);
/// Consume the array boundary explicitly: serde_json's tuple decoder otherwise
/// reports an extra numeric component as a syntax error rather than a shape
/// error. Allocation depends only on the schema's tiny fixed length.
fn fixed<'de, D, T, const N: usize>(deserializer: D) -> Result<[T; N], D::Error>
where
    D: de::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Components<T, const N: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Components<T, N> {
        type Value = [T; N];
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "exactly {N} tracking components")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut values = Vec::with_capacity(N);
            for index in 0..N {
                values.push(
                    seq.next_element()?
                        .ok_or_else(|| de::Error::invalid_length(index, &self))?,
                );
            }
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(de::Error::invalid_length(N + 1, &self));
            }
            values
                .try_into()
                .map_err(|_| de::Error::custom("tracking component count"))
        }
    }
    deserializer.deserialize_seq(Components::<T, N>(PhantomData))
}
impl<'de, T: Deserialize<'de>, const LIMIT: usize> Deserialize<'de> for List<T, LIMIT> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Entries<T, const LIMIT: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const LIMIT: usize> Visitor<'de> for Entries<T, LIMIT> {
            type Value = List<T, LIMIT>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "at most {LIMIT} tracking entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                loop {
                    if values.len() == LIMIT {
                        if seq.next_element::<IgnoredAny>()?.is_some() {
                            return Err(de::Error::custom("tracking sequence limit"));
                        }
                        break;
                    }
                    match seq.next_element()? {
                        Some(value) => values.push(value),
                        None => break,
                    }
                }
                Ok(List(values))
            }
        }
        deserializer.deserialize_seq(Entries::<T, LIMIT>(PhantomData))
    }
}
struct Text<const LIMIT: usize>(String);
impl<'de, const LIMIT: usize> Deserialize<'de> for Text<LIMIT> {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Chars<const LIMIT: usize>;
        impl<'de, const LIMIT: usize> Visitor<'de> for Chars<LIMIT> {
            type Value = Text<LIMIT>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "at most {LIMIT} UTF-8 bytes")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() > LIMIT {
                    return Err(E::custom("tracking string limit"));
                }
                Ok(Text(value.into()))
            }
        }
        deserializer.deserialize_str(Chars::<LIMIT>)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireAsset {
    version: u32,
    source_id: Text<128>,
    source_sha256: Text<64>,
    #[serde(deserialize_with = "fixed")]
    source_extent: [u32; 2],
    coordinates: WireCoordinates,
    ticks_per_second: u32,
    #[serde(deserialize_with = "fixed")]
    nominal_rate: [u32; 2],
    source_pts: List<i64, MAX_SOURCE_TIMESTAMPS>,
    meters_per_world_unit: Option<f64>,
    tracks: List<WireTrack, MAX_TRACKS>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireCoordinates {
    RightDownForward,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireKind {
    Point,
    Plane,
    Camera,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireVisibility {
    Visible,
    Occluded,
    Lost,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTrack {
    id: Text<128>,
    kind: WireKind,
    reference_index: Option<u32>,
    samples: List<WireSample, MAX_OBSERVATIONS>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSample {
    source_index: u32,
    visibility: WireVisibility,
    confidence: f64,
    observation: Option<WireObservation>,
    manual_correction: bool,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum WireObservation {
    Point {
        #[serde(deserialize_with = "fixed")]
        pixels: [f64; 2],
    },
    Plane {
        matrix: [[f64; 3]; 3],
    },
    Camera {
        camera_to_world: WirePose,
        calibration: WireCalibration,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePose {
    #[serde(deserialize_with = "fixed")]
    position: [f64; 3],
    #[serde(deserialize_with = "fixed")]
    orientation_xyzw: [f64; 4],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCalibration {
    #[serde(deserialize_with = "fixed")]
    source_extent: [u32; 2],
    #[serde(deserialize_with = "fixed")]
    focal: [f64; 2],
    #[serde(deserialize_with = "fixed")]
    principal: [f64; 2],
    #[serde(deserialize_with = "fixed")]
    radial: [f64; 3],
    #[serde(deserialize_with = "fixed")]
    tangential: [f64; 2],
}
impl WireAsset {
    fn into_asset(self) -> Result<Asset, Error> {
        let extent = |size: [u32; 2]| Extent::new(size[0], size[1]);
        let source_extent = extent(self.source_extent).map_err(|cause| Error::Geometry {
            track: None,
            sample: None,
            cause,
        })?;
        let mut tracks = Vec::with_capacity(self.tracks.0.len());
        let mut total = 0usize;
        for (index, wire) in self.tracks.0.into_iter().enumerate() {
            total = total
                .checked_add(wire.samples.0.len())
                .ok_or(Error::Field("total_observations"))?;
            if total > MAX_OBSERVATIONS {
                return Err(Error::Field("total_observations"));
            }
            let mut samples = Vec::with_capacity(wire.samples.0.len());
            for (number, sample) in wire.samples.0.into_iter().enumerate() {
                let geometry_error = |cause| Error::Geometry {
                    track: Some(index),
                    sample: Some(number),
                    cause,
                };
                let observation = match sample.observation {
                    None => None,
                    Some(WireObservation::Point { pixels }) => Some(Observation::Point(pixels)),
                    Some(WireObservation::Plane { matrix }) => Some(Observation::Plane(matrix)),
                    Some(WireObservation::Camera {
                        camera_to_world,
                        calibration: c,
                    }) => {
                        let calibration = Calibration::new(
                            extent(c.source_extent).map_err(geometry_error)?,
                            c.focal,
                            c.principal,
                            c.radial,
                            c.tangential,
                        )
                        .map_err(geometry_error)?;
                        Some(Observation::Camera {
                            camera_to_world: Pose {
                                position: camera_to_world.position,
                                orientation_xyzw: camera_to_world.orientation_xyzw,
                            },
                            calibration,
                        })
                    }
                };
                samples.push(Sample {
                    source_index: sample.source_index,
                    visibility: match sample.visibility {
                        WireVisibility::Visible => Visibility::Visible,
                        WireVisibility::Occluded => Visibility::Occluded,
                        WireVisibility::Lost => Visibility::Lost,
                    },
                    confidence: sample.confidence,
                    observation,
                    manual_correction: sample.manual_correction,
                });
            }
            tracks.push(Track {
                id: wire.id.0,
                kind: match wire.kind {
                    WireKind::Point => Kind::Point,
                    WireKind::Plane => Kind::Plane,
                    WireKind::Camera => Kind::Camera,
                },
                reference_index: wire.reference_index,
                samples,
            });
        }
        let hash = self.source_sha256.0.as_bytes();
        if hash.len() != 64 {
            return Err(Error::Field("source_sha256"));
        }
        let mut source_sha256 = [0; 32];
        let hex = |b| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        };
        for (i, pair) in hash.chunks_exact(2).enumerate() {
            source_sha256[i] = hex(pair[0])
                .zip(hex(pair[1]))
                .map(|(a, b)| a * 16 + b)
                .ok_or(Error::Field("source_sha256"))?;
        }
        let asset = Asset {
            version: self.version,
            source_id: self.source_id.0,
            source_sha256,
            source_extent,
            coordinates: match self.coordinates {
                WireCoordinates::RightDownForward => Coordinates::RightDownForward,
            },
            ticks_per_second: self.ticks_per_second,
            nominal_rate: Rate::new(self.nominal_rate[0], self.nominal_rate[1])
                .map_err(Error::Asset)?,
            source_pts: self.source_pts.0,
            meters_per_world_unit: self.meters_per_world_unit,
            tracks,
        };
        asset.validate().map_err(Error::Asset)?;
        Ok(asset)
    }
}
#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
