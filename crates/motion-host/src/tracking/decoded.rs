//! Actual decoded source admission. Native video adapters feed original signed
//! frame PTS and its rational clock, never rounded microseconds or inferred FPS.
use super::asset::Asset;
use std::{
    fs::File,
    io::{BufReader, Seek, SeekFrom},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("tracking decoded source asset: {0}")]
    Asset(#[source] super::asset::Error),
    #[error("tracking decoded source IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("tracking decoded image: {0}")]
    Image(#[from] image::ImageError),
    #[error("tracking decoded source phase={phase} cause={cause}")]
    Media {
        phase: &'static str,
        #[source]
        cause: motion_media::Error,
    },
    #[error("tracking decoded source: {0}")]
    Invalid(&'static str),
    #[error("tracking decoded dimensions frame={index:?} expected={expected:?} actual={actual:?}")]
    Dimensions {
        index: Option<usize>,
        expected: [u32; 2],
        actual: [u32; 2],
    },
    #[error("tracking decoded frame count expected={expected} actual={actual}")]
    Count { expected: usize, actual: usize },
    #[error("tracking decoded PTS frame={index} expected={expected}/{ticks_per_second} actual={actual}*{numerator}/{denominator}")]
    Timestamp {
        index: usize,
        expected: i64,
        ticks_per_second: u32,
        actual: i64,
        numerator: i32,
        denominator: i32,
    },
}

/// Decode from the same open file that was hashed. No transformed proxy or
/// eight-bit conversion is substituted for a high-precision original.
pub fn image(file: &mut File, solve: &Asset) -> Result<(), Error> {
    file.seek(SeekFrom::Start(0))?;
    let mut reader = image::ImageReader::new(BufReader::new(file)).with_guessed_format()?;
    let [width, height] = solve.source_extent.pixels();
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    let decoded = reader.decode()?;
    if [decoded.width(), decoded.height()] != [width, height] {
        return Err(Error::Dimensions {
            index: None,
            expected: [width, height],
            actual: [decoded.width(), decoded.height()],
        });
    }
    Ok(())
}

/// Streaming comparison of every decoded presentation frame. Memory is O(1)
/// beyond the already bounded original solve. Missing/extra/duplicate frames or
/// differing timestamps are errors; no sorting, deduplication or repair occurs.
pub struct VideoTimeline<'a> {
    solve: &'a Asset,
    numerator: i32,
    denominator: i32,
    index: usize,
    failed: bool,
}
impl<'a> VideoTimeline<'a> {
    pub fn new(solve: &'a Asset, numerator: i32, denominator: i32) -> Result<Self, Error> {
        solve.validate().map_err(Error::Asset)?;
        if numerator <= 0 || denominator <= 0 {
            return Err(Error::Invalid("native video clock must be positive"));
        }
        Ok(Self {
            solve,
            numerator,
            denominator,
            index: 0,
            failed: false,
        })
    }
    pub fn observe(&mut self, pts: i64, width: u32, height: u32) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid(
                "a previous decoded frame failed verification",
            ));
        }
        let result = self.observe_inner(pts, width, height);
        self.failed = result.is_err();
        result
    }
    fn observe_inner(&mut self, pts: i64, width: u32, height: u32) -> Result<(), Error> {
        let expected = *self.solve.source_pts.get(self.index).ok_or(Error::Count {
            expected: self.solve.source_pts.len(),
            actual: self.index + 1,
        })?;
        let extent = self.solve.source_extent.pixels();
        if [width, height] != extent {
            return Err(Error::Dimensions {
                index: Some(self.index),
                expected: extent,
                actual: [width, height],
            });
        }
        let left = i128::from(expected).checked_mul(i128::from(self.denominator));
        let right = i128::from(pts)
            .checked_mul(i128::from(self.numerator))
            .and_then(|n| n.checked_mul(i128::from(self.solve.ticks_per_second)));
        if left.is_none() || left != right {
            return Err(Error::Timestamp {
                index: self.index,
                expected,
                ticks_per_second: self.solve.ticks_per_second,
                actual: pts,
                numerator: self.numerator,
                denominator: self.denominator,
            });
        }
        self.index += 1;
        Ok(())
    }
    pub fn finish(self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid(
                "a previous decoded frame failed verification",
            ));
        }
        if self.index != self.solve.source_pts.len() {
            return Err(Error::Count {
                expected: self.solve.source_pts.len(),
                actual: self.index,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "decoded_tests.rs"]
mod tests;
