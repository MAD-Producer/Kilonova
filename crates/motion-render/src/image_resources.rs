//! Bounded image decoding. Dimensions describe the source, never the proxy.
use motion_model::Asset;
mod error;
pub use error::Error;
use crate::Scene;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};

pub const MAX_ENCODED_BYTES: u64 = 64 * 1024 * 1024;
/// Safe default for standalone callers; Session paths use their explicit policy.
pub const MAX_DECODED_BYTES: u64 = crate::resource_policy::DEFAULT_DECODED_IMAGE_BUDGET;
pub const MAX_PREVIEW_EDGE: u32 = 2048;
pub const IDLE_TEXTURE_BYTES: u64 = 32 * 1024 * 1024;
pub const PREFETCH_SECONDS: f64 = 0.5;
pub const MAX_PREFETCH_IMAGES: usize = 2;

/// Inspect clip dependencies without sampling expressions or animation. Times
/// are mapped through nested clips in seconds, including mixed frame rates.
/// Invalid rates/mappings propagate as errors. Negative or non-finite query
/// positions retain their normal empty-lookahead behavior.
pub fn upcoming_assets(project: &motion_model::Project, frame: f64) -> motion_model::Result<Vec<u64>> {
    fn visit(
        document: &motion_model::Project,
        layers: &[motion_model::Layer],
        composition: &str,
        fps: u32,
        frames: u32,
        begin: f64,
        end: f64,
        delay: f64,
        depth: usize,
        found: &mut std::collections::BTreeMap<u64, f64>,
    ) -> motion_model::Result<()> {
        if depth >= motion_model::composition::MAX_COMPOSITION_DEPTH {
            return Ok(());
        }
        for layer in layers.iter().filter(|l| l.visible) {
            let clip = layer.interval(frames);
            let start = begin.max(clip.in_frame).max(0.);
            let finish = end.min(clip.out_frame).min(f64::from(frames));
            if start >= finish {
                continue;
            }
            let wait = delay + (start - begin) / f64::from(fps);
            match &layer.content {
                motion_model::Content::Image { asset }
                | motion_model::Content::Text {
                    raster_asset: asset,
                    ..
                } => {
                    found
                        .entry(*asset)
                        .and_modify(|t| *t = t.min(wait))
                        .or_insert(wait);
                }
                motion_model::Content::Composition { clip } => {
                    if let Some(child) = document
                        .compositions
                        .iter()
                        .find(|c| c.id == clip.composition)
                    {
                        visit(
                            document,
                            &child.layers,
                            &child.id,
                            child.fps,
                            child.frames,
                            crate::scene::composition_source_frame(clip, layer.local_frame(start), fps,
                                child.fps, composition, layer.id, "image_prefetch")?,
                            crate::scene::composition_source_frame(clip, layer.local_frame(finish), fps,
                                child.fps, composition, layer.id, "image_prefetch")?,
                            wait,
                            depth + 1,
                            found,
                        )?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    if !frame.is_finite() || frame < 0. {
        return Ok(vec![]);
    }
    if project.fps == 0 {
        return motion_model::composition::fail(
            &project.composition_id,
            "invalid_frame_rate",
            "Composition frame rate must be positive",
            serde_json::json!({"fps":project.fps,"phase":"image_prefetch"}),
        );
    }
    let mut found = std::collections::BTreeMap::new();
    visit(
        project,
        &project.layers,
        &project.composition_id,
        project.fps,
        project.frames,
        frame,
        frame + f64::from(project.fps) * PREFETCH_SECONDS,
        0.,
        0,
        &mut found,
    )?;
    let mut ordered: Vec<_> = found.into_iter().collect();
    ordered.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    Ok(ordered.into_iter().map(|(id, _)| id).collect())
}

fn check_cancel(cancel: Option<&AtomicBool>) -> Result<(), Error> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Full,
    Preview(u32),
}
impl Resolution {
    pub fn dimensions(self, width: u32, height: u32) -> (u32, u32) {
        let edge = match self {
            Self::Full => return (width, height),
            Self::Preview(edge) => edge.clamp(1, MAX_PREVIEW_EDGE),
        };
        let longest = width.max(height);
        if longest <= edge {
            (width, height)
        } else {
            // Integer ceil avoids a zero short side and never exceeds edge.
            (
                ((u64::from(width) * u64::from(edge)).div_ceil(u64::from(longest))) as u32,
                ((u64::from(height) * u64::from(edge)).div_ceil(u64::from(longest))) as u32,
            )
        }
    }
}

/// Collect sampled dependencies recursively, including alpha occluders. Scene
/// sampling already excludes inactive clips; offscreen geometry is kept because
/// effects can move it into the viewport or use it for occlusion.
pub fn scene_assets(scene: &Scene) -> BTreeSet<u64> {
    fn collect(scene: &Scene, ids: &mut BTreeSet<u64>) {
        ids.extend(scene.layers.iter().filter_map(|l| l.asset));
        ids.extend(scene.effects.iter().filter(|e| e.enabled && scene.layers.iter().any(|l|l.id==e.layer))
            .filter_map(|e| match e.image_input { Some(motion_model::EffectImageInput::Asset {asset}) => Some(asset), _ => None }));
        ids.extend(scene.effects.iter()
            .filter(|effect| effect.enabled && scene.layers.iter().any(|layer| layer.id == effect.layer))
            .filter_map(|effect| effect.scene.as_ref().and_then(|settings| settings.sprite_asset)));
        for child in &scene.nested {
            collect(&child.scene, ids);
        }
    }
    let mut ids = BTreeSet::new();
    collect(scene, &mut ids);
    ids
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub id: u64,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub root: PathBuf,
}
impl Source {
    pub fn new(root: &Path, asset: &Asset) -> Result<Self, Error> {
        motion_model::storage::validate_relative_path(&asset.path)?;
        let base = root.canonicalize()?;
        let path = root
            .join(&asset.path)
            .canonicalize()?;
        if !path.starts_with(&base) {
            return Err("image resolves outside project directory".into());
        }
        Ok(Self {
            id: asset.id,
            path,
            width: asset.width,
            height: asset.height,
            root: base,
        })
    }
    pub fn validate(&self) -> Result<image::ImageFormat, Error> {
        let (width, height, format, _) = inspect(&self.path)?;
        if (width, height) != (self.width, self.height) {
            return Err(format!("image {} metadata mismatch", self.id).into());
        }
        Ok(format)
    }
}

pub fn inspect(path: &Path) -> Result<(u32, u32, image::ImageFormat, u64), Error> {
    let meta = path.metadata()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_ENCODED_BYTES {
        return Err("encoded image must be 1 byte..64 MiB".into());
    }
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format().ok_or("unsupported image format")?;
    let (w, h) = reader.into_dimensions()?;
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return Err("image dimensions must be 1..16384".into());
    }
    Ok((w, h, format, meta.len()))
}

pub struct Pixels {
    pub width: u32,
    pub height: u32,
    /// sRGB encoded, premultiplied in linear space, matching the renderer.
    pub rgba: Vec<u8>,
    pub cached: bool,
}

pub fn decode(source: &Source, resolution: Resolution) -> Result<Pixels, Error> {
    decode_with_budget(source, resolution, MAX_DECODED_BYTES)
}
pub fn decode_with_budget(source: &Source, resolution: Resolution, budget: u64) -> Result<Pixels, Error> {
    decode_cancellable(source, resolution, budget, None)
}

/// Validate the logical RGBA8 result capacity, independently of codec workspace.
pub fn decoded_image_bytes(width: u32, height: u32, budget: u64) -> Result<u64, Error> {
    if width == 0 || height == 0 {
        return Err("image dimensions must be positive".into());
    }
    let cost = u64::from(width).checked_mul(u64::from(height))
        .and_then(|cost| cost.checked_mul(4))
        .ok_or_else(|| Error::from("image pixel capacity overflow"))?;
    check_decode_budget("output", cost, budget)?;
    Ok(cost)
}

fn validate_decode_budget(budget: u64) -> Result<(), Error> {
    if !(4..=crate::resource_policy::MAX_DECODED_IMAGE_BUDGET).contains(&budget) {
        return Err("decoded image budget must be 4 bytes..2 GiB".into());
    }
    Ok(())
}
fn check_decode_budget(phase: &'static str, required: u64, budget: u64) -> Result<(), Error> {
    validate_decode_budget(budget)?;
    if required > budget {
        return Err(Error::Budget {
            rejection: crate::ResourceRejection::Budget {
                scope: crate::ResourceBudgetScope::DecodedImages,
                required_bytes: required,
                budget_bytes: budget,
            },
            message: format!("CPU image pixels exceed configured decoded image budget phase={phase} required_bytes={required} budget_bytes={budget}"),
        });
    }
    Ok(())
}
fn decode_cancellable(
    source: &Source,
    resolution: Resolution,
    budget: u64,
    cancel: Option<&AtomicBool>,
) -> Result<Pixels, Error> {
    check_cancel(cancel)?;
    validate_decode_budget(budget)?;
    source.validate()?;
    let (w, h) = resolution.dimensions(source.width, source.height);
    let cost = decoded_image_bytes(w, h, budget)?;
    let cache = if matches!(resolution, Resolution::Preview(_))
        && (w, h) != (source.width, source.height)
    {
        proxy_path(source, w, h, cancel)
    } else {
        None
    };
    if let Some(path) = &cache {
        if let Some(rgba) = read_proxy(path, w, h) {
            return Ok(Pixels {
                width: w,
                height: h,
                rgba,
                cached: true,
            });
        }
    }
    check_cancel(cancel)?;
    let length = usize::try_from(cost).map_err(|_| Error::from("image output exceeds addressable length"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length)?;
    rgba.resize(length, 0);
    let mut pixels = Pixels {
        width: w,
        height: h,
        rgba,
        cached: false,
    };
    decode_into_cancellable(source, resolution, &mut pixels.rgba, budget, cancel)?;
    check_cancel(cancel)?;
    if let Some(path) = &cache {
        write_proxy(path, &pixels);
    }
    Ok(pixels)
}

// Disposable cache, outside assets/ and project packages. Keys include a full
// source SHA-256 and algorithm version; corrupt cache data is never trusted.
pub const PROXY_CACHE_BYTES: u64 = 64 * 1024 * 1024;
// Canonical proxy directories select a fixed shard. All reads, writes and
// eviction within one directory share its lock. Hash collisions conservatively
// serialize unrelated directories; no path keys or per-project locks are retained.
const PROXY_CACHE_SHARDS: usize = 64;
static CACHE_LOCKS: [std::sync::Mutex<()>; PROXY_CACHE_SHARDS] =
    [const { std::sync::Mutex::new(()) }; PROXY_CACHE_SHARDS];

fn proxy_cache_shard(path: &Path) -> usize {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.parent().hash(&mut hash);
    (hash.finish() % PROXY_CACHE_SHARDS as u64) as usize
}

fn proxy_cache_mutex(path: &Path) -> &'static std::sync::Mutex<()> {
    &CACHE_LOCKS[proxy_cache_shard(path)]
}
static PROXY_WRITES: crate::observability::FailureCounter = crate::observability::FailureCounter::new();
static PROXY_READS: crate::observability::FailureCounter = crate::observability::FailureCounter::new();
static PROXY_DIRECTORY: crate::observability::FailureCounter = crate::observability::FailureCounter::new();
static PROXY_CLEANUP_WARNED: AtomicBool = AtomicBool::new(false);

mod proxy_observation;
use proxy_observation::Events as ProxyEvents;

fn proxy_cache_lock(path: &Path, events: &mut ProxyEvents) -> std::sync::MutexGuard<'static, ()> {
    let cache = proxy_cache_mutex(path);
    cache.lock().unwrap_or_else(|poisoned| {
        // This mutex protects filesystem operations, not shared in-memory data.
        // Proxy reads still validate length/header/hash before accepting bytes.
        let guard = poisoned.into_inner();
        cache.clear_poison();
        events.recovered(proxy_cache_shard(path));
        guard
    })
}

struct ProxyTemporary<'a>(PathBuf, &'a mut ProxyEvents);
impl Drop for ProxyTemporary<'_> {
    fn drop(&mut self) {
        match fs::remove_file(&self.0) {
            Ok(()) => { PROXY_CLEANUP_WARNED.store(false, Ordering::Relaxed); }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                PROXY_CLEANUP_WARNED.store(false, Ordering::Relaxed);
            }
            Err(error) => {
                if !PROXY_CLEANUP_WARNED.swap(true, Ordering::Relaxed) {
                    self.1.cleanup_failed(&self.0, &error);
                }
            }
        }
    }
}
static CACHE_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn proxy_path(source: &Source, w: u32, h: u32, cancel: Option<&AtomicBool>) -> Option<PathBuf> {
    use sha2::{Digest, Sha256};
    source.validate().ok()?;
    let mut hash = Sha256::new();
    hash.update(b"motion-image-proxy-linear-box-v1");
    hash.update(w.to_le_bytes());
    hash.update(h.to_le_bytes());
    let mut file = File::open(&source.path).ok()?;
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0;
    loop {
        check_cancel(cancel).ok()?;
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > MAX_ENCODED_BYTES {
            return None;
        }
        hash.update(&buffer[..n]);
    }
    let directory = source.root.join(".cache/image-proxies-v1");
    let resolved = fs::create_dir_all(&directory).and_then(|()| directory.canonicalize());
    let directory = match resolved {
        Ok(directory) if directory.starts_with(&source.root) => directory,
        Ok(_) => {
            PROXY_DIRECTORY.failure("image_proxy.directory", format_args!(
                "source_id={} path={} error=cache escapes project root", source.id, directory.display()));
            return None;
        }
        Err(error) => {
            PROXY_DIRECTORY.failure("image_proxy.directory", format_args!(
                "source_id={} path={} error={error}", source.id, directory.display()));
            return None;
        }
    };
    PROXY_DIRECTORY.success();
    Some(directory.join(format!("{:x}.rgba", hash.finalize())))
}
fn read_proxy(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    // Serialize IO within a directory shard; release it before logging.
    let mut events = ProxyEvents::default();
    let result = {
        let _guard = proxy_cache_lock(path, &mut events);
        read_proxy_inner(path, w, h)
    };
    match result {
        Ok(rgba) => {
            // NotFound is an expected cache miss, not a failed cache read.
            PROXY_READS.success();
            rgba
        }
        Err(error) => {
            PROXY_READS.failure("image_proxy.read", format_args!(
                "path={} width={w} height={h} error={error}", path.display()));
            None
        }
    }
}
fn read_proxy_inner(path: &Path, w: u32, h: u32) -> std::io::Result<Option<Vec<u8>>> {
    use sha2::{Digest, Sha256};
    use std::io::{Error, ErrorKind};
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let cost = u64::from(w) * u64::from(h) * 4;
    let actual = file.metadata()?.len();
    if actual != cost + 44 {
        return Err(Error::new(ErrorKind::InvalidData,
            format!("proxy length mismatch: expected={} actual={actual}", cost + 44)));
    }
    let mut header = [0u8; 12];
    file.read_exact(&mut header)?;
    if &header[..4] != b"MSIP"
        || header[4..8] != w.to_le_bytes()
        || header[8..12] != h.to_le_bytes()
    {
        return Err(Error::new(ErrorKind::InvalidData, "proxy header or dimensions mismatch"));
    }
    let mut rgba = vec![0; cost as usize];
    file.read_exact(&mut rgba)?;
    let mut digest = [0u8; 32];
    file.read_exact(&mut digest)?;
    if Sha256::digest(&rgba).as_slice() != digest {
        return Err(Error::new(ErrorKind::InvalidData, "proxy checksum mismatch"));
    }
    Ok(Some(rgba))
}
fn write_proxy(path: &Path, pixels: &Pixels) {
    let mut events = ProxyEvents::default();
    let result = {
        let _guard = proxy_cache_lock(path, &mut events);
        write_proxy_inner(path, pixels, &mut events)
    };
    match result {
        Ok(()) => PROXY_WRITES.success(),
        Err(error) => {
            let context = proxy_observation::context(path, &error);
            PROXY_WRITES.failure("image_proxy.write", format_args!("{context}"));
        }
    }
}
fn write_proxy_inner(path: &Path, pixels: &Pixels, events: &mut ProxyEvents) -> std::io::Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::{Error, ErrorKind};
    let directory = path.parent().ok_or_else(|| Error::new(ErrorKind::InvalidInput, "proxy directory missing"))?;
    let cost = pixels.rgba.len() as u64 + 44;
    let mut old = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue; };
        if name.len() != 69 || !name.ends_with(".rgba")
            || !name[..64].bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let metadata = entry.metadata()?;
        if !metadata.is_file() { continue; }
        old.push((metadata.modified()?, entry.path(), metadata.len()));
    }
    old.sort_by_key(|entry| entry.0);
    // The old destination remains valid until the final atomic replacement.
    let mut total = old.iter().filter(|entry| entry.1 != path).try_fold(0u64,
        |sum, entry| sum.checked_add(entry.2).ok_or_else(|| Error::other("proxy cache size overflow")))?;
    for (_, previous, bytes) in old {
        if total.saturating_add(cost) <= PROXY_CACHE_BYTES { break; }
        if previous == path { continue; }
        fs::remove_file(&previous)
            .map_err(|error| Error::new(error.kind(), format!("evict proxy {}: {error}", previous.display())))?;
        total = total.saturating_sub(bytes);
    }
    if total.saturating_add(cost) > PROXY_CACHE_BYTES {
        return Err(Error::other("image proxy disk budget exhausted"));
    }
    let serial = CACHE_SERIAL.fetch_add(1, Ordering::Relaxed);
    let temp = directory.join(format!("proxy-{}-{serial}.tmp", std::process::id()));
    // Install cleanup only after exclusive creation succeeded. Bind the file
    // after the guard so it closes first during an early return or unwind.
    let (temporary, mut file) = {
        let file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        (ProxyTemporary(temp, events), file)
    };
    file.write_all(b"MSIP")?;
    file.write_all(&pixels.width.to_le_bytes())?;
    file.write_all(&pixels.height.to_le_bytes())?;
    file.write_all(&pixels.rgba)?;
    file.write_all(&Sha256::digest(&pixels.rgba))?;
    file.flush()?;
    drop(file);
    fs::rename(&temporary.0, path)?;
    Ok(())
}

/// The PNG path writes directly into the caller's allocation, including full
/// resolution export; no full-size intermediate RGBA/Java byte[] is required.
pub fn decode_into(
    source: &Source,
    resolution: Resolution,
    output: &mut [u8],
) -> Result<(), Error> {
    decode_into_with_budget(source, resolution, output, MAX_DECODED_BYTES)
}
pub fn decode_into_with_budget(
    source: &Source,
    resolution: Resolution,
    output: &mut [u8],
    budget: u64,
) -> Result<(), Error> {
    decode_into_cancellable(source, resolution, output, budget, None)
}
fn decode_into_cancellable(
    source: &Source,
    resolution: Resolution,
    output: &mut [u8],
    budget: u64,
    cancel: Option<&AtomicBool>,
) -> Result<(), Error> {
    check_cancel(cancel)?;
    let format = source.validate()?;
    let (w, h) = resolution.dimensions(source.width, source.height);
    let cost = u64::from(w) * u64::from(h) * 4;
    if output.len() as u64 != cost {
        return Err("invalid image output length".into());
    }
    check_decode_budget("output", cost, budget)?;
    if format == image::ImageFormat::Png {
        return png_into(source, w, h, output, cancel);
    }
    // Existing JPEG resources remain supported. Its decoder needs a full image,
    // so validate that cost before allocating even when requesting a proxy.
    check_decode_budget("non_png_source", u64::from(source.width) * u64::from(source.height) * 4, budget)?;
    let mut reader = image::ImageReader::open(&source.path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(budget);
    reader.limits(limits);
    let image = reader.decode()?;
    check_cancel(cancel)?;
    use image::GenericImageView;
    let lut = linear_lut();
    let mut row = vec![[0.; 4]; w as usize];
    let mut rows = 0;
    let mut y_out = 0;
    for y in 0..source.height {
        check_cancel(cancel)?;
        for x in 0..source.width {
            accumulate(
                &mut row[(u64::from(x) * u64::from(w) / u64::from(source.width)) as usize],
                image.get_pixel(x, y).0,
                &lut,
            );
        }
        rows += 1;
        if y + 1 == source.height
            || (u64::from(y + 1) * u64::from(h) / u64::from(source.height)) > u64::from(y_out)
        {
            finish_row(
                &mut row,
                rows,
                source.width,
                w,
                &mut output[(y_out * w * 4) as usize..((y_out + 1) * w * 4) as usize],
            );
            rows = 0;
            y_out += 1;
        }
    }
    Ok(())
}

fn linear_lut() -> [f32; 256] {
    std::array::from_fn(|i| {
        let v = i as f32 / 255.;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
}
fn encode(v: f32) -> u8 {
    let v = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (v.clamp(0., 1.) * 255.).round() as u8
}
fn accumulate(sum: &mut [f32; 4], pixel: [u8; 4], lut: &[f32; 256]) {
    let a = pixel[3] as f32 / 255.;
    for c in 0..3 {
        sum[c] += lut[pixel[c] as usize] * a;
    }
    sum[3] += a;
}
fn finish_row(row: &mut [[f32; 4]], rows: u32, sw: u32, w: u32, output: &mut [u8]) {
    for (x, (sum, pixel)) in row.iter_mut().zip(output.chunks_exact_mut(4)).enumerate() {
        let left = (x as u64 * u64::from(sw)).div_ceil(u64::from(w));
        let right = ((x as u64 + 1) * u64::from(sw)).div_ceil(u64::from(w));
        let n = ((right - left) * u64::from(rows)) as f32;
        for c in 0..3 {
            pixel[c] = encode(sum[c] / n);
        }
        pixel[3] = (sum[3] / n * 255.).round() as u8;
        *sum = [0.; 4];
    }
}
fn png_into(
    source: &Source,
    w: u32,
    h: u32,
    output: &mut [u8],
    cancel: Option<&AtomicBool>,
) -> Result<(), Error> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(&source.path)?));
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    decoder.set_ignore_text_chunk(true);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    if reader.info().width != source.width || reader.info().height != source.height {
        return Err("PNG metadata changed during decoding".into());
    }
    let color = reader.output_color_type().0;
    let channels = color.samples();
    let lut = linear_lut();
    let rgba = |pixel: &[u8]| match color {
        png::ColorType::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
        png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
        png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
        png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
        _ => unreachable!("EXPAND removes indexed output"),
    };
    if reader.info().interlaced {
        // Adam7 rows visit each source pixel exactly once. Proxy accumulators
        // are bounded by the proxy dimensions, never by the original image.
        let full = (w, h) == (source.width, source.height);
        let mut sums = if full {
            Vec::new()
        } else {
            vec![[0.; 4]; (w * h) as usize]
        };
        for (x0, y0, dx, dy) in [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ] {
            if x0 >= source.width {
                continue;
            }
            for y in (y0..source.height).step_by(dy) {
                check_cancel(cancel)?;
                let row = reader
                    .next_row()?
                    .ok_or("truncated Adam7 image")?;
                let xs = (x0..source.width).step_by(dx);
                if row.data().len() != xs.clone().count() * channels {
                    return Err("invalid Adam7 row".into());
                }
                for (x, p) in xs.zip(row.data().chunks_exact(channels)) {
                    let xo = u64::from(x) * u64::from(w) / u64::from(source.width);
                    let yo = u64::from(y) * u64::from(h) / u64::from(source.height);
                    let offset = (yo * u64::from(w) + xo) as usize;
                    if full {
                        let mut sum = [0.; 4];
                        accumulate(&mut sum, rgba(p), &lut);
                        for c in 0..3 {
                            output[offset * 4 + c] = encode(sum[c]);
                        }
                        output[offset * 4 + 3] = rgba(p)[3];
                    } else {
                        accumulate(&mut sums[offset], rgba(p), &lut);
                    }
                }
            }
        }
        if !full {
            for y in 0..h {
                check_cancel(cancel)?;
                let rows = ((u64::from(y + 1) * u64::from(source.height)).div_ceil(u64::from(h))
                    - (u64::from(y) * u64::from(source.height)).div_ceil(u64::from(h)))
                    as u32;
                finish_row(
                    &mut sums[(y * w) as usize..((y + 1) * w) as usize],
                    rows,
                    source.width,
                    w,
                    &mut output[(y * w * 4) as usize..((y + 1) * w * 4) as usize],
                );
            }
        }
    } else {
        if (w, h) == (source.width, source.height) {
            for y in 0..source.height {
                check_cancel(cancel)?;
                let row = reader
                    .next_row()?
                    .ok_or("truncated PNG image")?;
                if row.data().len() != source.width as usize * channels {
                    return Err("invalid PNG row".into());
                }
                let target = &mut output[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
                for (input, pixel) in row
                    .data()
                    .chunks_exact(channels)
                    .zip(target.chunks_exact_mut(4))
                {
                    let p = rgba(input);
                    if p[3] == 255 {
                        pixel.copy_from_slice(&p);
                    } else {
                        let a = p[3] as f32 / 255.;
                        for c in 0..3 {
                            pixel[c] = encode(lut[p[c] as usize] * a);
                        }
                        pixel[3] = p[3];
                    }
                }
            }
            reader.finish()?;
            return Ok(());
        }
        let mut sums = vec![[0.; 4]; w as usize];
        let mut rows = 0;
        let mut yo = 0;
        for y in 0..source.height {
            check_cancel(cancel)?;
            let row = reader
                .next_row()?
                .ok_or("truncated PNG image")?;
            if row.data().len() != source.width as usize * channels {
                return Err("invalid PNG row".into());
            }
            for (x, p) in row.data().chunks_exact(channels).enumerate() {
                accumulate(
                    &mut sums[(x as u64 * u64::from(w) / u64::from(source.width)) as usize],
                    rgba(p),
                    &lut,
                );
            }
            rows += 1;
            if y + 1 == source.height
                || (u64::from(y + 1) * u64::from(h) / u64::from(source.height)) > u64::from(yo)
            {
                finish_row(
                    &mut sums,
                    rows,
                    source.width,
                    w,
                    &mut output[(yo * w * 4) as usize..((yo + 1) * w * 4) as usize],
                );
                rows = 0;
                yo += 1;
            }
        }
    }
    reader.finish()?;
    Ok(())
}

/// One outstanding decode per renderer, with at most one result allocation.
/// Decoding never holds a session/GPU lock. Stale results are discarded by the
/// owner using the exact source and requested dimensions.
#[derive(Default)]
pub struct DecodeTask {
    pending: Option<(
        Source,
        Resolution,
        u64,
        bool,
        Arc<AtomicBool>,
        Receiver<Result<Pixels, Error>>,
    )>,
}
impl DecodeTask {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn cancel_unwanted(&mut self, wanted: &BTreeSet<u64>, resolution: Resolution) {
        self.cancel_unwanted_inner(wanted, resolution, None);
    }
    pub fn cancel_unwanted_with_budget(&mut self, wanted: &BTreeSet<u64>, resolution: Resolution, budget: u64) {
        self.cancel_unwanted_inner(wanted, resolution, Some(budget));
    }
    fn cancel_unwanted_inner(&mut self, wanted: &BTreeSet<u64>, resolution: Resolution, budget: Option<u64>) {
        if let Some((source, mode, pending_budget, _, cancel, _)) = &self.pending {
            if *mode != resolution || !wanted.contains(&source.id) || budget.is_some_and(|budget| budget != *pending_budget) {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }
    pub fn cancel(&mut self) {
        if let Some((_, _, _, _, cancel, _)) = &self.pending {
            cancel.store(true, Ordering::Relaxed);
        }
    }
    pub fn start(
        &mut self,
        source: Source,
        resolution: Resolution,
        speculative: bool,
    ) -> Result<(), Error> {
        self.start_with_budget(source, resolution, speculative, MAX_DECODED_BYTES)
    }
    pub fn start_with_budget(
        &mut self,
        source: Source,
        resolution: Resolution,
        speculative: bool,
        budget: u64,
    ) -> Result<(), Error> {
        validate_decode_budget(budget)?;
        if self.busy() {
            return Err("an image decode is already pending".into());
        }
        let (send, receive) = mpsc::sync_channel(1);
        let input = source.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        std::thread::Builder::new()
            .name("motion-image-decode".into())
            .spawn(move || {
                let began = std::time::Instant::now();
                log::info!("start image decode source_id={} resolution={:?} decoded_budget_bytes={} speculative={}", input.id, resolution, budget, speculative);
                let result = std::panic::catch_unwind(|| {
                    decode_cancellable(&input, resolution, budget, Some(&worker_cancel))
                })
                .unwrap_or_else(|payload| {
                    let reason = payload.downcast_ref::<String>().map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown native error");
                    Err(Error::WorkerPanic(reason.chars().take(2048).collect()))
                });
                log::debug!("done image decode source_id={} resolution={:?} elapsed_ms={}", input.id, resolution, began.elapsed().as_millis());
                let _ = send.send(result);
            })?;
        self.pending = Some((source, resolution, budget, speculative, cancel, receive));
        Ok(())
    }
    pub fn poll(&mut self) -> Option<(Source, Resolution, bool, Result<Pixels, Error>)> {
        let (_, _, _, _, _, receive) = self.pending.as_ref()?;
        let result = match receive.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err(Error::WorkerDisconnected),
        };
        let (source, resolution, _, speculative, cancel, _) = self.pending.take().unwrap();
        Some((
            source,
            resolution,
            speculative,
            if cancel.load(Ordering::Relaxed) {
                Err(Error::Cancelled)
            } else {
                result
            },
        ))
    }
}
impl Drop for DecodeTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod proxy_failure_tests {
    use super::*;

    fn isolate_temporary_serial() -> Vec<std::sync::MutexGuard<'static, ()>> {
        // This fixture precreates the exact next process-wide temporary name.
        // Other directories can now write concurrently, so exclude all writers
        // only while injecting that collision. Never wait holding partial locks.
        let began = std::time::Instant::now();
        loop {
            let attempt: std::result::Result<Vec<_>, _> =
                CACHE_LOCKS.iter().map(std::sync::Mutex::try_lock).collect();
            match attempt {
                Ok(guards) => return guards,
                Err(error) => drop(error),
            }
            assert!(began.elapsed() < std::time::Duration::from_secs(5));
            std::thread::yield_now();
        }
    }

    fn pixels() -> Pixels {
        Pixels {width:2,height:1,rgba:vec![80;8],cached:false}
    }

    #[test]
    fn poisoned_cache_lock_recovers_and_still_roundtrips_verified_pixels() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("{:064x}.rgba",1));
        let caught = std::panic::catch_unwind(|| {
            let mut events = ProxyEvents::default();
            let _guard = proxy_cache_lock(&path, &mut events);
            panic!("injected proxy writer panic");
        });
        assert!(caught.is_err());
        let pixels = pixels();
        write_proxy(&path,&pixels);
        assert!(!proxy_cache_mutex(&path).is_poisoned());
        assert_eq!(read_proxy(&path,2,1).unwrap(),pixels.rgba);
    }

    #[test]
    fn exclusive_temp_collision_preserves_foreign_file_and_previous_cache() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("{:064x}.rgba",2));
        let mut events = ProxyEvents::default();
        let _guards = isolate_temporary_serial();
        fs::write(&path,b"previous cache").unwrap();
        let serial = CACHE_SERIAL.load(Ordering::Relaxed);
        let foreign = root.path().join(format!("proxy-{}-{serial}.tmp", std::process::id()));
        fs::write(&foreign,b"another writer").unwrap();
        assert_eq!(write_proxy_inner(&path,&pixels(),&mut events).unwrap_err().kind(),std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(foreign).unwrap(),b"another writer");
        assert_eq!(fs::read(path).unwrap(),b"previous cache");
    }

    #[test]
    fn rename_failure_preserves_destination_and_cleans_own_temp_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(format!("{:064x}.rgba",3));
        let mut events = ProxyEvents::default();
        let _guard = proxy_cache_lock(&path, &mut events);
        fs::create_dir(&path).unwrap();
        assert!(write_proxy_inner(&path,&pixels(),&mut events).is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(),1);
    }
}

#[cfg(test)]
mod proxy_read_observation_tests;

#[cfg(test)]
mod proxy_lock_observation_tests;

#[cfg(test)]
mod proxy_directory_tests;
