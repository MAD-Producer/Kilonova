//! Owned original sRGB8 pixels paired with an admitted image-depth result.
//! This source is immutable and independent of the disk and GPU image caches.
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};

#[derive(Clone)]
pub struct DepthImage(Arc<Data>);
struct Data {
    size: [u32; 2],
    rgba: Vec<u8>,
    fingerprint: [u8; 32],
}
impl fmt::Debug for DepthImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DepthImage").field("size", &self.0.size)
            .field("bytes", &self.bytes()).finish_non_exhaustive()
    }
}
impl DepthImage {
    /// Original, straight RGBA in sRGB8. Admission performs no implicit profile
    /// conversion. Moving the allocation avoids a second full-image copy.
    pub fn new(size: [u32; 2], rgba: Vec<u8>) -> Result<Self, crate::RenderError> {
        if size.contains(&0) || size.iter().any(|n| *n > 4096)
            || u64::from(size[0]) * u64::from(size[1]) * 4 != rgba.len() as u64
            || rgba.capacity() > 64 << 20 {
            return Err(crate::RenderError::Invalid("owned depth image requires exact 1..4096 RGBA8 pixels within 64 MiB".into()));
        }
        let mut hash = Sha256::new();
        hash.update(b"MotionStudio/depth/original-rgba8-srgb/v1\0");
        for n in size { hash.update(n.to_le_bytes()); }
        hash.update(&rgba);
        Ok(Self(Arc::new(Data { size, rgba, fingerprint: hash.finalize().into() })))
    }
    pub fn size(&self) -> [u32; 2] { self.0.size }
    pub fn rgba(&self) -> &[u8] { &self.0.rgba }
    /// Retained allocation capacity, including spare bytes, not just pixel length.
    pub fn bytes(&self) -> usize { self.0.rgba.capacity() }
    pub(crate) fn fingerprint(&self) -> [u8; 32] { self.0.fingerprint }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_dimensions_and_fingerprints_original_pixels_without_debug_payloads() {
        assert!(DepthImage::new([0, 1], vec![]).is_err());
        assert!(DepthImage::new([1, 1], vec![0; 3]).is_err());
        assert!(DepthImage::new([4097, 1], vec![]).is_err());
        let mut rgba = Vec::with_capacity(32);
        rgba.extend_from_slice(&[10, 20, 30, 255]);
        let a = DepthImage::new([1, 1], rgba).unwrap();
        assert_eq!(a.bytes(), 32);
        let weak = Arc::downgrade(&a.0);
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.0, &b.0));
        assert_eq!(a.fingerprint(), DepthImage::new([1, 1], vec![10,20,30,255]).unwrap().fingerprint());
        assert_ne!(a.fingerprint(), DepthImage::new([1, 1], vec![10,21,30,255]).unwrap().fingerprint());
        assert!(!format!("{a:?}").contains("255"));
        drop(a); drop(b);
        assert!(weak.upgrade().is_none());
    }
}
