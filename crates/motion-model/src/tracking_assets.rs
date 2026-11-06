//! Immutable original tracking documents, separate from editable pose bindings.
//! These are internal attachments, not new project-browser UI items.
use crate::{
    ensure,
    media_assets::{AssetKind, AssetRef},
    Result,
};
use serde::{Deserialize, Serialize};

pub mod replacement;

pub const MAX_TRACKING_ASSET_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_TRACKING_ASSETS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackingFormat {
    MotionTrackingJsonV1,
    ColmapCameraTextV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingAsset {
    pub id: u64,
    pub path: String,
    pub bytes: u64,
    /// Complete original-document bytes, not a repaired/reformatted JSON value.
    pub sha256: String,
    pub format: TrackingFormat,
    /// Explicit image/video identity; text inside a solve cannot select a source.
    pub source: AssetRef,
    /// Original encoded source bytes, independent of the solve document digest.
    pub source_sha256: String,
    /// Immutable normalized runtime data, verified against the managed original
    /// at storage boundaries. Project/history clones share its allocation.
    pub data: crate::tracking::shared::SharedData,
}
impl TrackingAsset {
    pub fn validate(&self) -> Result<()> {
        crate::storage::validate_relative_path(&self.path)?;
        ensure(
            self.path.starts_with("assets/tracking/") && self.path.ends_with(".mstrack"),
            "tracking source must be a managed original document",
        )?;
        ensure(
            self.id != 0 && self.source.id != 0,
            "tracking IDs must be nonzero",
        )?;
        ensure(
            matches!(self.source.kind, AssetKind::Image | AssetKind::Video),
            "tracking source must be image or video",
        )?;
        ensure(
            (1..=MAX_TRACKING_ASSET_BYTES).contains(&self.bytes),
            "invalid tracking document byte count",
        )?;
        for digest in [&self.sha256, &self.source_sha256] {
            ensure(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid tracking SHA-256",
            )?;
        }
        Ok(())
    }
    /// Shape validation alone is not source admission. Enforce this identity
    /// before a runtime binding or a managed resource is consumed.
    pub fn validate_source_digest(&self) -> Result<()> {
        let digest: String = self
            .data
            .source_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        ensure(
            digest == self.source_sha256,
            "tracking runtime source digest differs",
        )
    }
}
