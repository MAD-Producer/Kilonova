//! Owned original SDR video samples, staged on the inference worker. The request
//! coordinate is not a frame PTS: the managed index and decoder must agree before
//! an exact PTS can enter the inference input or depth result.
use super::{image::rgb_from_rgba, Cancellation, Error, Input, Key, Result, VideoKey};
use crate::{video_frame::VideoPixels, Platform};
use motion_core::VideoAsset;
use motion_model::depth::{DepthSource, DepthTime};
use std::{borrow::Cow, path::PathBuf, sync::Arc};

pub(crate) struct VideoInput {
    pub key: VideoKey,
    pub root: PathBuf,
    pub asset: VideoAsset,
    pub platform: Arc<dyn Platform>,
    pub budget: u64,
}
impl VideoInput {
    pub fn decode(self, cancellation: &Cancellation) -> Result<Input> {
        cancellation.check()?;
        self.asset.validate()?;
        self.key.validate()?;
        if self.key.asset != self.asset.id
            || self.key.geometry.source_size
                != [self.asset.display_width, self.asset.display_height]
            || self.key.target_us < self.asset.video_start_us
            || self.key.target_us >= self.asset.video_end_us
        {
            return Err(Error::Invalid(
                "video depth request differs from registered source",
            ));
        }
        let [width, height] = self.key.geometry.source_size;
        let [_, _, cw, ch] = self.key.geometry.crop;
        let budget = self.budget.min(128 << 20);
        let index_bytes = 16 + u64::from(self.asset.frame_count) * 8;
        let pixels = u64::from(width) * u64::from(height);
        let rgb_bytes = u64::from(cw) * u64::from(ch) * 3;
        if width > 4096 || height > 4096 || pixels * 12 + rgb_bytes + index_bytes > budget {
            return Err(Error::Invalid(
                "video depth staging exceeds decoded payload budget",
            ));
        }
        let root = self.root.canonicalize()?;
        let path = root.join(&self.asset.path).canonicalize()?;
        if !path.starts_with(&root) {
            return Err(Error::Invalid("video resolves outside project directory"));
        }
        let metadata = path.metadata()?;
        if !metadata.is_file() || metadata.len() != self.asset.bytes {
            return Err(Error::Invalid("video size differs from registered source"));
        }
        let pts = motion_media::load_video_index(&root, &self.asset)?;
        cancellation.check()?;
        let index = pts
            .partition_point(|p| *p <= self.key.target_us)
            .checked_sub(1)
            .ok_or(Error::Invalid(
                "video index has no sample at requested time",
            ))?;
        let expected_pts = pts[index];
        let expected_end = pts
            .get(index + 1)
            .copied()
            .unwrap_or(self.asset.video_end_us);
        // The decoder is created, used and dropped on this same worker. No
        // !Send codec or GPU handle enters Session or crosses a thread boundary.
        let color = (self.asset.color_standard, self.asset.color_range);
        let coded_geometry = (self.asset.width, self.asset.height, self.asset.rotation);
        let mut decoder = self.platform.open_decoder(&path, self.asset, pts)?;
        cancellation.check()?;
        let decoded = decoder.frame(self.key.target_us, &|| {
            cancellation
                .check()
                .map_err(|_| motion_media::Error::from("depth video request canceled"))
        });
        cancellation.check()?;
        let frame = decoded?;
        drop(decoder);
        if frame.pts != expected_pts
            || frame.end != expected_end
            || (frame.width, frame.height) != (width, height)
        {
            return Err(Error::Invalid(
                "decoded depth sample differs from managed video PTS index or dimensions",
            ));
        }
        if frame.bytes() as u64 + pixels * 4 + rgb_bytes + index_bytes > budget {
            return Err(Error::Invalid(
                "decoded video depth payload exceeds budget before RGB conversion",
            ));
        }
        let rgba = match &frame.pixels {
            VideoPixels::Yuv(yuv) => {
                if yuv.display_size() != (width, height)
                    || (yuv.standard, yuv.range) != color
                    || (yuv.width, yuv.height, yuv.rotation) != coded_geometry
                {
                    return Err(Error::Invalid(
                        "decoded depth YUV geometry or color differs from registered video",
                    ));
                }
                Cow::Owned(yuv.to_rgba()?)
            }
            VideoPixels::Rgba(rgba) => Cow::Borrowed(rgba.as_slice()),
        };
        cancellation.check()?;
        let key = Key {
            source: DepthSource {
                asset: self.key.asset,
                revision: self.key.revision,
                pts: DepthTime {
                    ticks: i64::try_from(frame.pts)
                        .map_err(|_| Error::Invalid("video source PTS overflow"))?,
                    timescale: 1_000_000,
                },
            },
            geometry: self.key.geometry,
            model: self.key.model,
        };
        rgb_from_rgba(key, &rgba, cancellation)
    }
}
