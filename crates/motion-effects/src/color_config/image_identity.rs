//! Canonical color endpoints for prepared images. Role aliases do not create a
//! different image space, and unrelated view choices do not invalidate working
//! images. A display/view endpoint remains distinct from a named color space.
use super::*;

/// Opaque, configuration-bound identity. Construct only after verifying the
/// complete configuration bytes and resolving an explicit non-data endpoint.
/// This identifies a producer's declared values; it does not inspect pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColorSpaceIdentity {
    config: [u8; 32],
    key: [u8; 32],
}
impl ColorSpaceIdentity {
    pub fn config_key(self) -> [u8; 32] {
        self.config
    }
    pub fn key(self) -> [u8; 32] {
        self.key
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Endpoint<'a> {
    Space { name: &'a str },
    Display { display: &'a str, view: &'a str },
}
impl VerifiedColorConfig {
    fn image_identity(&self, endpoint: Endpoint<'_>) -> Result<ColorSpaceIdentity> {
        let mut hash = Sha256::new();
        hash.update(b"motion-color-image-space-v1\0");
        hash.update(self.key());
        hash.update(serde_json::to_vec(&endpoint)?);
        Ok(ColorSpaceIdentity {
            config: self.key(),
            key: hash.finalize().into(),
        })
    }

    pub fn space_identity(&self, reference: &SpaceReference) -> Result<ColorSpaceIdentity> {
        let name = self.manifest.declared_color_space(reference)?;
        self.image_identity(Endpoint::Space { name })
    }

    pub fn display_identity(&self, value: &DisplayView) -> Result<ColorSpaceIdentity> {
        self.manifest.display_view(value)?;
        self.image_identity(Endpoint::Display {
            display: &value.display,
            view: &value.view,
        })
    }
}
