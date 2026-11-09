//! Bounded, explicitly opt-in research batches; never a product cache key.
use crate::admission::{Result, Tensor};
use serde::Deserialize;

pub const MAX_INPUTS: usize = 32;
pub const MAX_MANIFEST_BYTES: u64 = 128 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub version: u32,
    pub program: String,
    pub program_sha256: String,
    pub requested_backend: String,
    pub inputs: Vec<Tensor>,
}
impl BatchRequest {
    pub fn validate(&self, expected_program: &str) -> Result<()> {
        if self.version != 1 || self.program_sha256 != expected_program {
            return Err("unqualified batch version/program identity".into());
        }
        if self.inputs.is_empty() || self.inputs.len() > MAX_INPUTS {
            return Err("research batch requires 1..=32 sequential inputs".into());
        }
        if self.program.is_empty() || self.program.len() > 4096 {
            return Err("batch program path is empty or exceeds 4096 bytes".into());
        }
        if !matches!(self.requested_backend.as_str(), "vulkan" | "dx12" | "metal") {
            return Err("explicit supported batch backend required; no fallback".into());
        }
        for (index, input) in self.inputs.iter().enumerate() {
            if input.name != "rgb" || input.shape != [1, 3, 518, 518] {
                return Err(format!("batch input {index} name/shape differs"));
            }
            input
                .bytes()
                .map_err(|cause| format!("batch input {index}: {cause}"))?;
        }
        Ok(())
    }
}
