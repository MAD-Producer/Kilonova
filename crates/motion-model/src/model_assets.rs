//! Managed immutable static scenes. Geometry stays in a bounded frozen bundle;
//! ordinary layer transforms and their tracks remain editable project data.
use crate::{ensure, Result};
use serde::{Deserialize, Serialize};

/// Frozen glTF/OBJ source plus dependencies, not an original path to reopen.
pub const MAX_MODEL_ASSET_BYTES: u64 = 320 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAsset {
    pub id: u64,
    pub path: String,
    pub bytes: u64,
    /// SHA-256 of the complete frozen bundle, in lowercase hexadecimal.
    pub sha256: String,
}
impl ModelAsset {
    pub fn validate(&self) -> Result<()> {
        crate::storage::validate_relative_path(&self.path)?;
        ensure(
            self.path.starts_with("assets/models/") && self.path.ends_with(".mscn"),
            "model source must be a managed frozen scene",
        )?;
        ensure(self.id != 0, "model asset ID must be nonzero")?;
        ensure(
            (1..=MAX_MODEL_ASSET_BYTES).contains(&self.bytes),
            "invalid frozen model byte count",
        )?;
        ensure(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid model SHA-256",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelClip {
    pub asset: u64,
    pub scene: u32,
    /// Imported right-handed, Y-up metres to composition world pixels.
    pub pixels_per_metre: f32,
    /// Geometry pivot in imported metres, independent of the 2D anchor.
    pub pivot: [f32; 3],
    /// Explicit native MSAA selection; unsupported modes are rejected.
    pub samples: u32,
}
impl ModelClip {
    pub fn validate(&self) -> Result<()> {
        ensure(self.asset != 0, "model source missing")?;
        ensure(
            self.pixels_per_metre.is_finite()
                && (0.001..=1_000_000.).contains(&self.pixels_per_metre),
            "model pixels per metre out of range",
        )?;
        ensure(
            self.pivot
                .iter()
                .all(|n| n.is_finite() && n.abs() <= 10_000_000.),
            "invalid model pivot",
        )?;
        ensure(
            matches!(self.samples, 1 | 4),
            "model samples must be 1 or 4",
        )
    }
}
