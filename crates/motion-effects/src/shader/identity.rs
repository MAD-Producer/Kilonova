//! Identity of actual compiled code, excluding origins and sampled values.
use super::CompiledShader;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug)]
pub struct CompiledCodeIdentity {
    pub digest: [u8; 32],
    /// UTF-8 source/reflection names, excluding binary hash framing.
    pub text_bytes: u64,
    pub loop_work: u64,
    pub samples_per_fragment: Option<u64>,
}

pub fn compiled_code_identity(shader: &CompiledShader) -> CompiledCodeIdentity {
    let mut hash = Sha256::new();
    hash.update(b"motion-execution-code-v1");
    let mut bytes = 0_u64;
    for text in [&shader.wgsl, &shader.glsl.vertex, &shader.glsl.fragment] {
        hash.update((text.len() as u64).to_le_bytes());
        hash.update(text.as_bytes());
        bytes = bytes.saturating_add(text.len() as u64);
    }
    for (tag, bindings) in [(0_u8, &shader.glsl.blocks), (1, &shader.glsl.textures)] {
        hash.update([tag]);
        hash.update((bindings.len() as u64).to_le_bytes());
        for (name, binding) in bindings {
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            for word in binding {
                hash.update(word.to_le_bytes());
            }
            bytes = bytes.saturating_add(name.len() as u64);
        }
    }
    hash.update([u8::from(shader.sprite), u8::from(shader.additive)]);
    hash.update(shader.loop_work.to_le_bytes());
    hash.update([u8::from(shader.samples_per_fragment.is_some())]);
    hash.update(shader.samples_per_fragment.unwrap_or(0).to_le_bytes());
    CompiledCodeIdentity {
        digest: hash.finalize().into(),
        text_bytes: bytes,
        loop_work: shader.loop_work,
        samples_per_fragment: shader.samples_per_fragment,
    }
}
