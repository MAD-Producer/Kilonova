//! Original stored samples for an explicitly selected color processor. Unlike
//! preview decoding, this path never applies sRGB, premultiplies, resizes, or
//! reads/writes an RGBA8 proxy. PNG integer samples retain their full precision.
use super::{check_cancel, check_decode_budget, Error, Source};
use motion_effects::image_contract::ChannelPrecision;
use std::{fs::File, io::BufReader, sync::atomic::AtomicBool};

#[derive(Debug)]
pub struct ColorPixels {
    pub width: u32,
    pub height: u32,
    pub source_precision: ChannelPrecision,
    /// Stored, normalized components. Space and alpha interpretation belong to
    /// the explicit project input contract, not the filename or this decoder.
    pub rgba: Vec<[f32; 4]>,
}

/// Full original resolution only. `budget` bounds owned output payload; PNG
/// codec workspace is separately capped at 16 MiB. JPEG additionally admits
/// its full decoded image before entering the codec. No external runtime.
/// Call inside an observed non-Present preparation operation.
pub fn decode_color_f32(
    source: &Source,
    budget: u64,
    cancel: &AtomicBool,
) -> Result<ColorPixels, Error> {
    check_cancel(Some(cancel))?;
    let format = source.validate()?;
    let cost = u64::from(source.width)
        .checked_mul(u64::from(source.height))
        .and_then(|n| n.checked_mul(16))
        .ok_or_else(|| Error::from("Float32 image capacity overflow"))?;
    check_decode_budget("color_f32_output", cost, budget)?;
    let length = usize::try_from(cost / 16)
        .map_err(|_| Error::from("Float32 image exceeds addressable length"))?;
    match format {
        image::ImageFormat::Png => png(source, length, cancel),
        image::ImageFormat::Jpeg => jpeg(source, length, cost, budget, cancel),
        _ => Err("original color decoding supports PNG and JPEG only".into()),
    }
}

fn output(length: usize) -> Result<Vec<[f32; 4]>, Error> {
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(length)?;
    pixels.resize(length, [0.; 4]);
    Ok(pixels)
}

fn png(source: &Source, length: usize, cancel: &AtomicBool) -> Result<ColorPixels, Error> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(&source.path)?));
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    decoder.set_ignore_text_chunk(true);
    // EXPAND resolves indexed/tRNS/low-bit inputs without STRIP_16.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    if (reader.info().width, reader.info().height) != (source.width, source.height) {
        return Err("PNG metadata changed during color decoding".into());
    }
    if reader.info().animation_control.is_some() {
        return Err("animated PNG requires a temporal source, not a static color image".into());
    }
    let (color, depth) = reader.output_color_type();
    let (sample_bytes, divisor, source_precision) = match depth {
        png::BitDepth::Eight => (1, 255., ChannelPrecision::Unorm8),
        png::BitDepth::Sixteen => (2, 65535., ChannelPrecision::Unorm16),
        _ => return Err("PNG color expansion did not produce 8/16-bit samples".into()),
    };
    if color == png::ColorType::Indexed {
        return Err("PNG color expansion did not resolve the palette".into());
    }
    let pixel_bytes = color.samples() * sample_bytes;
    let passes: &[(u32, u32, usize, usize)] = if reader.info().interlaced {
        &[
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
    } else {
        &[(0, 0, 1, 1)]
    };
    let mut rgba = output(length)?;
    for &(x0, y0, dx, dy) in passes {
        if x0 >= source.width {
            continue;
        }
        for y in (y0..source.height).step_by(dy) {
            check_cancel(Some(cancel))?;
            let row = reader.next_row()?.ok_or("truncated PNG color image")?;
            let xs = (x0..source.width).step_by(dx);
            if row.data().len() != xs.clone().count() * pixel_bytes {
                return Err("invalid PNG color row length".into());
            }
            for (x, pixel) in xs.zip(row.data().chunks_exact(pixel_bytes)) {
                let sample = |i: usize| {
                    if sample_bytes == 1 {
                        f32::from(pixel[i]) / divisor
                    } else {
                        f32::from(u16::from_be_bytes([pixel[i * 2], pixel[i * 2 + 1]])) / divisor
                    }
                };
                let value = match color {
                    png::ColorType::Rgba => [sample(0), sample(1), sample(2), sample(3)],
                    png::ColorType::Rgb => [sample(0), sample(1), sample(2), 1.],
                    png::ColorType::GrayscaleAlpha => [sample(0), sample(0), sample(0), sample(1)],
                    png::ColorType::Grayscale => [sample(0), sample(0), sample(0), 1.],
                    png::ColorType::Indexed => {
                        return Err("unresolved PNG color palette".into());
                    }
                };
                rgba[(u64::from(y) * u64::from(source.width) + u64::from(x)) as usize] = value;
            }
        }
    }
    reader.finish()?;
    check_cancel(Some(cancel))?;
    Ok(ColorPixels {
        width: source.width,
        height: source.height,
        source_precision,
        rgba,
    })
}

fn jpeg(
    source: &Source,
    length: usize,
    cost: u64,
    budget: u64,
    cancel: &AtomicBool,
) -> Result<ColorPixels, Error> {
    // The supported JPEG decoder returns at most RGBA8. Its owned decoded
    // payload must coexist with Float32 output, independently of codec workspace.
    let decoded = u64::from(source.width) * u64::from(source.height) * 4;
    check_decode_budget("color_jpeg_workspace", cost + decoded, budget)?;
    let mut reader = image::ImageReader::open(&source.path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(budget - cost);
    reader.limits(limits);
    let image = reader.decode()?;
    use image::GenericImageView;
    if image.dimensions() != (source.width, source.height)
        || !matches!(
            image,
            image::DynamicImage::ImageLuma8(_)
                | image::DynamicImage::ImageLumaA8(_)
                | image::DynamicImage::ImageRgb8(_)
                | image::DynamicImage::ImageRgba8(_)
        )
    {
        return Err("JPEG color dimensions or decoded sample precision changed".into());
    }
    check_cancel(Some(cancel))?;
    let mut rgba = output(length)?;
    for y in 0..source.height {
        check_cancel(Some(cancel))?;
        for x in 0..source.width {
            rgba[(u64::from(y) * u64::from(source.width) + u64::from(x)) as usize] =
                image.get_pixel(x, y).0.map(|v| f32::from(v) / 255.);
        }
    }
    check_cancel(Some(cancel))?;
    Ok(ColorPixels {
        width: source.width,
        height: source.height,
        source_precision: ChannelPrecision::Unorm8,
        rgba,
    })
}

#[cfg(test)]
mod tests;
