//! Persistent identity of the actual opaque sRGB8 input after preparation.
use super::DepthGeometry;
use crate::{ensure, Result};
use serde::{Deserialize, Serialize};

/// SHA-256 of the versioned RGB contract, dimensions and prepared RGB bytes.
/// This is separate from the live document/revision/PTS installation guard.
/// Paths and caller-supplied asset labels do not prove pixel identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedDepthInput {
    pub rgb_size: [u32; 2],
    pub rgb_sha256: [u8; 32],
}

impl PreparedDepthInput {
    pub fn validate(&self, geometry: &DepthGeometry) -> Result<()> {
        geometry.validate()?;
        let [_, _, width, height] = geometry.crop;
        let expected = if geometry.quarter_turns % 2 == 0 {
            [width, height]
        } else {
            [height, width]
        };
        ensure(
            self.rgb_size == expected && self.rgb_size.iter().all(|size| (1..=4096).contains(size)),
            "prepared depth input differs from oriented source crop",
        )
    }
}
