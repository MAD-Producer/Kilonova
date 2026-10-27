//! Bounded managed-image staging on the inference worker, before the backend.
//! Source pixels must already be sRGB8. Profile conversion, HDR down-conversion
//! and alpha compositing need explicit policies; this path refuses those inputs.
use super::{Cancellation, Error, Input, Key, Result};
use ::image::{DynamicImage, ImageDecoder, ImageReader};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::PathBuf,
    sync::Arc,
};

const MAX_ENCODED: u64 = 64 << 20;
const MAX_DECODED: u64 = 128 << 20;

pub(crate) struct ImageInput {
    pub key: Key,
    pub root: PathBuf,
    pub relative: String,
    pub budget: u64,
}
impl ImageInput {
    pub fn decode(self, cancellation: &Cancellation) -> Result<Input> {
        cancellation.check()?;
        motion_model::storage::validate_relative_path(&self.relative)?;
        let root = self.root.canonicalize()?;
        let path = root.join(&self.relative).canonicalize()?;
        if !path.starts_with(&root) {
            return Err(Error::Invalid("image resolves outside project directory"));
        }
        let file = File::open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_ENCODED {
            return Err(Error::Invalid("encoded depth input exceeds 1 byte..64 MiB"));
        }
        // Read from one handle with a hard bound even if the file grows.
        let mut encoded = Vec::new();
        file.take(MAX_ENCODED + 1).read_to_end(&mut encoded)?;
        if encoded.len() as u64 > MAX_ENCODED {
            return Err(Error::Invalid("encoded depth input grew beyond 64 MiB"));
        }
        cancellation.check()?;
        let [w, h] = self.key.geometry.source_size;
        let [_, _, cw, ch] = self.key.geometry.crop;
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return Err(Error::Invalid(
                "image depth input dimensions exceed 1..4096",
            ));
        }
        // Conservative decoded payload allowance: decoder pixels, RGBA staging
        // and the cropped RGB output. Encoded bytes/codec internals are separate.
        let required = u64::from(w) * u64::from(h) * 8 + u64::from(cw) * u64::from(ch) * 3;
        let budget = self.budget.min(MAX_DECODED);
        if required > budget {
            return Err(Error::Invalid(
                "image depth staging exceeds decoded payload budget",
            ));
        }
        let mut reader = ImageReader::new(Cursor::new(encoded)).with_guessed_format()?;
        let mut limits = ::image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(budget);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        if decoder.dimensions() != (w, h) {
            return Err(Error::Invalid(
                "image dimensions differ from registered source",
            ));
        }
        if decoder.color_type().bits_per_pixel() > 32
            || !matches!(
                decoder.color_type(),
                ::image::ColorType::L8
                    | ::image::ColorType::La8
                    | ::image::ColorType::Rgb8
                    | ::image::ColorType::Rgba8
            )
        {
            return Err(Error::Invalid(
                "depth image requires explicit sRGB8 conversion",
            ));
        }
        if decoder.icc_profile()?.is_some() {
            return Err(Error::Invalid(
                "depth image requires explicit ICC conversion to sRGB8",
            ));
        }
        let rgba = DynamicImage::from_decoder(decoder)?.into_rgba8();
        cancellation.check()?;
        let [x0, y0, cw, ch] = self.key.geometry.crop;
        let [ow, oh] = if self.key.geometry.quarter_turns % 2 == 0 {
            [cw, ch]
        } else {
            [ch, cw]
        };
        let mut rgb = vec![0; (u64::from(ow) * u64::from(oh) * 3) as usize];
        for y in 0..ch {
            cancellation.check()?;
            for x in 0..cw {
                let pixel = rgba.get_pixel(x0 + x, y0 + y).0;
                if pixel[3] != 255 {
                    return Err(Error::Invalid(
                        "depth image crop requires explicit alpha compositing",
                    ));
                }
                let (mut ox, oy) = match self.key.geometry.quarter_turns {
                    0 => (x, y),
                    1 => (ch - 1 - y, x),
                    2 => (cw - 1 - x, ch - 1 - y),
                    3 => (y, cw - 1 - x),
                    _ => return Err(Error::Invalid("unsupported depth rotation")),
                };
                if self.key.geometry.mirror_x {
                    ox = ow - 1 - ox;
                }
                let offset = ((u64::from(oy) * u64::from(ow) + u64::from(ox)) * 3) as usize;
                rgb[offset..offset + 3].copy_from_slice(&pixel[..3]);
            }
        }
        Input::new(self.key, [ow, oh], Arc::from(rgb))
    }
}
