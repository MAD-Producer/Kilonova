//! Original solve attachments for core project/history/storage operations.
//! UI routing, file publication and pose-track binding are separate boundaries.
use super::{asset::Asset, decode, source};
use motion_core::{
    media_assets::{self, AssetRef, MediaAsset},
    Project, TrackingAsset, TrackingFormat,
};
use std::{fmt, io::Read, path::Path};

#[derive(Debug)]
pub enum Error {
    Project(motion_core::Error),
    Decode(decode::Error),
    Runtime(super::encode::Error),
    Source(source::Error),
    Binding(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(cause) => write!(f, "tracking project attachment: {cause}"),
            Self::Decode(cause) => write!(f, "tracking original document: {cause}"),
            Self::Runtime(cause) => write!(f, "tracking runtime admission: {cause}"),
            Self::Source(cause) => write!(f, "tracking bound source: {cause}"),
            Self::Binding(cause) => write!(f, "tracking binding: {cause}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Project(cause) => Some(cause),
            Self::Decode(cause) => Some(cause),
            Self::Runtime(cause) => Some(cause),
            Self::Source(cause) => Some(cause),
            Self::Binding(_) => None,
        }
    }
}

fn check_source(
    project: &Project,
    source: AssetRef,
    solve: &Asset,
    bytes: u64,
) -> Result<(), Error> {
    let media =
        media_assets::lookup(project, source).ok_or(Error::Binding("managed source is missing"))?;
    let size = match media {
        MediaAsset::Image(image) => {
            if solve.source_pts.len() != 1 {
                return Err(Error::Binding("one image requires one source timestamp"));
            }
            [image.width, image.height]
        }
        MediaAsset::Video(video) => {
            if bytes != video.bytes {
                return Err(Error::Binding("managed video byte count differs"));
            }
            if solve.source_pts.len() != video.frame_count as usize {
                return Err(Error::Binding(
                    "tracking source timestamp count differs from managed video",
                ));
            }
            // Solve calibration is in original source pixels, before display rotation.
            [video.width, video.height]
        }
        MediaAsset::Audio(_) => return Err(Error::Binding("audio is not a tracking plate")),
    };
    if size != solve.source_extent.pixels() {
        return Err(Error::Binding("original source extent differs"));
    }
    Ok(())
}

/// Owns byte-exact imported JSON including whitespace and original number spelling.
/// Conversion helpers never replace these bytes with a normalized/repaired solve.
pub struct Attachment {
    descriptor: TrackingAsset,
    original: Vec<u8>,
}
impl Attachment {
    /// Caller must obtain a fresh verification from the selected original source.
    /// This is not a promise that a previously verified filesystem path is unchanged.
    pub fn prepare(
        project: &Project,
        id: u64,
        source: AssetRef,
        original: &[u8],
        proof: source::VerifiedSource,
    ) -> Result<Self, Error> {
        project.validate().map_err(Error::Project)?;
        let solve = decode::from_slice(original).map_err(Error::Decode)?;
        if proof.sha256() != solve.source_sha256 {
            return Err(Error::Binding(
                "verification belongs to a different solve/source",
            ));
        }
        check_source(project, source, &solve, proof.byte_len())?;
        let sha256 = motion_core::storage::resource_sha256(original);
        let descriptor = TrackingAsset {
            id,
            path: format!("assets/tracking/{id}-{sha256}.mstrack"),
            bytes: original.len() as u64,
            sha256,
            format: TrackingFormat::MotionTrackingJsonV1,
            source,
            source_sha256: solve
                .source_sha256
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            data: motion_core::tracking::shared::SharedData::new(solve).map_err(Error::Runtime)?,
        };
        let mut candidate = project.clone();
        candidate.tracking_assets.push(descriptor.clone());
        candidate.validate().map_err(Error::Project)?;
        Ok(Self {
            descriptor,
            original: original.to_vec(),
        })
    }
    pub fn descriptor(&self) -> &TrackingAsset {
        &self.descriptor
    }
    pub fn original(&self) -> &[u8] {
        &self.original
    }
    pub fn solve(&self) -> &Asset {
        &self.descriptor.data
    }
    pub fn registration(&self) -> motion_core::Command {
        motion_core::Command::RegisterTrackingAsset {
            asset: self.descriptor.clone(),
        }
    }
}

/// Read a managed original document and reverify the currently selected source.
/// The caller publishes operation/request/in-flight state before any file/reader IO
/// and handles returned causes after releasing business locks. This helper neither
/// selects a path from imported text nor changes the project/history on failure.
pub fn load_original(
    root: &Path,
    project: &Project,
    id: u64,
    source_reader: &mut impl Read,
    max_source_bytes: u64,
    cancelled: impl FnMut() -> bool,
) -> Result<Asset, Error> {
    project.validate().map_err(Error::Project)?;
    let descriptor = project
        .tracking_assets
        .iter()
        .find(|a| a.id == id)
        .ok_or(Error::Binding("tracking attachment is missing"))?;
    if descriptor.validate_source_digest().is_err() {
        return Err(Error::Binding(
            "document and descriptor source digests differ",
        ));
    }
    let original =
        motion_core::storage::read_tracking_resource(root, descriptor).map_err(Error::Project)?;
    let solve = decode::from_slice(&original).map_err(Error::Decode)?;
    let declared: String = solve
        .source_sha256
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if descriptor.source_sha256 != declared {
        return Err(Error::Binding(
            "document and descriptor source digests differ",
        ));
    }
    let proof = source::verify(&solve, source_reader, max_source_bytes, cancelled)
        .map_err(Error::Source)?;
    check_source(project, descriptor.source, &solve, proof.byte_len())?;
    Ok(solve)
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
