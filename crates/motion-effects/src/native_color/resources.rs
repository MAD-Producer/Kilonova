//! Close FileTransform resources over verified bytes, without temporary paths,
//! environment expansion, OCIO's file cache, or a second file-format parser.
use super::*;
use crate::color_lut::{ColorLut, LutInterpolation, LutKind, LutPurpose};

#[repr(C)]
pub(super) struct Resource {
    name: *const c_char,
    kind: u32,
    size: u32,
    min: [f32; 3],
    max: [f32; 3],
    values: *const [f32; 3],
    count: usize,
}

pub(super) struct Resources(Vec<(CString, ColorLut)>);
impl Resources {
    pub(super) fn new(config: &VerifiedColorConfig) -> Result<Self> {
        let mut result = Vec::new();
        let mut retained = 0usize;
        for name in config.manifest().resources.keys() {
            let reject = |reason| Error::Resource {
                name: name.clone(),
                reason,
            };
            if !name.ends_with(".cube") {
                return Err(reject(
                    "only strict single-table .cube resources are admitted",
                ));
            }
            let bytes = config
                .resource_bytes(name)
                .ok_or_else(|| reject("verified resource bytes are missing"))?;
            let text =
                std::str::from_utf8(bytes).map_err(|_| reject("Cube resource is not UTF-8"))?;
            // Interpolation here only validates the numeric data. The owning
            // FileTransform explicitly selects the native interpolation later.
            // Purpose derives from that processor phase, never from the file.
            let lut = ColorLut::from_cube(text, LutInterpolation::Linear, LutPurpose::Look)
                .map_err(|source| Error::ResourceLut {
                    name: name.clone(),
                    source,
                })?;
            let descriptor = lut.descriptor();
            if descriptor.kind == LutKind::ThreeD && descriptor.size > 64 {
                return Err(reject("native cube edge exceeds 64"));
            }
            retained = retained
                .checked_add(lut.values().len() * std::mem::size_of::<[f32; 3]>())
                .ok_or_else(|| reject("numeric resource byte count overflow"))?;
            if retained > crate::color_config::MAX_BUNDLE_BYTES {
                return Err(reject("numeric resources exceed bundle byte capacity"));
            }
            let name_c = CString::new(name.as_bytes())
                .map_err(|_| reject("NUL in verified resource name"))?;
            result.push((name_c, lut));
        }
        Ok(Self(result))
    }

    pub(super) fn native(&self) -> Vec<Resource> {
        self.0
            .iter()
            .map(|(name, lut)| {
                let descriptor = lut.descriptor();
                Resource {
                    name: name.as_ptr(),
                    kind: match descriptor.kind {
                        LutKind::OneD => 1,
                        LutKind::ThreeD => 3,
                    },
                    size: descriptor.size,
                    min: descriptor.domain_min,
                    max: descriptor.domain_max,
                    values: lut.values().as_ptr(),
                    count: lut.values().len(),
                }
            })
            .collect()
    }
}
