//! Quantize only at a leaf. A negative source rate addresses the sample before
//! the mapped boundary: one decoder microsecond or one native PCM sample, never
//! a whole composition frame. Frozen source time has picture but silent audio.
use super::{invalid, SourceTimeSample};
use crate::{CompositionClip, Error, Result, VideoClip, MAX_COMPOSITION_FPS};

fn frame_rate(fps: u32) -> Result<()> {
    if fps == 0 || fps > MAX_COMPOSITION_FPS {
        return Err(invalid(
            "media_frame_rate",
            "source frame rate is outside the composition contract",
        ));
    }
    Ok(())
}
fn source_offset(offset: u64) -> Result<()> {
    if offset > 3_600_000_000 {
        return Err(invalid(
            "media_offset",
            "source offset exceeds the one hour media contract",
        ));
    }
    Ok(())
}

impl SourceTimeSample {
    /// Existing forward timestamp rounding stays exactly the same. Asset-range
    /// checks and hold-first-picture behavior remain the decoder owner's job.
    pub fn video_time_us(self, clip: &VideoClip, fps: u32) -> Result<i64> {
        Self::new(self.frame, self.rate)?;
        frame_rate(fps)?;
        source_offset(clip.source_offset_us)?;
        let time = clip.source_time_us(self.frame, fps);
        Ok(if self.rate < 0. { time - 1 } else { time })
    }

    /// Continuous native PCM position for the existing resampler. None is a
    /// normal freeze, requiring silence; it must not loop one value as DC.
    /// Negative/out-of-range positions are retained for normal zero padding.
    pub fn pcm_position(
        self,
        fps: u32,
        sample_rate: u32,
        source_offset_us: u64,
    ) -> Result<Option<f64>> {
        Self::new(self.frame, self.rate)?;
        frame_rate(fps)?;
        source_offset(source_offset_us)?;
        if !(8_000..=192_000).contains(&sample_rate) {
            return Err(invalid(
                "pcm_sample_rate",
                "native PCM rate must be within 8..192 kHz",
            ));
        }
        if self.rate == 0. {
            return Ok(None);
        }
        let position = self.frame * f64::from(sample_rate) / f64::from(fps)
            + source_offset_us as f64 * f64::from(sample_rate) / 1_000_000.;
        Ok(Some(if self.rate < 0. {
            position - 1.
        } else {
            position
        }))
    }

    /// Keep source-start and the actual existing cross-composition frame-rate
    /// operation order, while propagating its derivative without quantization.
    pub fn through_composition(
        self,
        clip: &CompositionClip,
        parent_fps: u32,
        child_fps: u32,
    ) -> Result<Self> {
        Self::new(self.frame, self.rate)?;
        frame_rate(parent_fps)?;
        frame_rate(child_fps)?;
        let frame = clip
            .source_frame(self.frame, parent_fps, child_fps)
            .map_err(|source| Error::LayerTime {
                phase: "composition_source",
                source: Box::new(source),
            })?;
        Self::new(
            frame,
            self.rate * f64::from(child_fps) / f64::from(parent_fps),
        )
    }
}
