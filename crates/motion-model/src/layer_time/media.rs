//! Quantize only at a leaf. Earlier traversal addresses the sample before
//! the mapped boundary: one decoder microsecond or one native PCM sample, never
//! a whole composition frame. Frozen source time has picture but silent audio.
use super::{invalid, SourceTimeSample, TimeTraversal};
use crate::{CompositionClip, Error, Result, VideoClip, MAX_COMPOSITION_FPS};

fn frame_rate(fps: crate::FrameRate) -> Result<()> {
    if !fps.is_between(1, MAX_COMPOSITION_FPS) {
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
    /// Round the exact binary frame to the nearest decoder microsecond, with
    /// ties away from zero. Asset-range checks remain the decoder owner's job.
    pub fn video_time_us(self, clip: &VideoClip, fps: crate::FrameRate) -> Result<i64> {
        self.validate()?;
        frame_rate(fps)?;
        source_offset(clip.source_offset_us)?;
        let time = clip.source_time_us(self.frame, fps)?;
        Ok(if self.traversal == TimeTraversal::Earlier {
            time.checked_sub(1).ok_or(crate::RateError::NumericRange)?
        } else { time })
    }

    /// Continuous native PCM position for the existing resampler. None is a
    /// normal freeze, requiring silence; it must not loop one value as DC.
    /// Negative/out-of-range positions are retained for normal zero padding.
    pub fn pcm_position(
        self,
        fps: crate::FrameRate,
        sample_rate: u32,
        source_offset_us: u64,
    ) -> Result<Option<f64>> {
        self.validate()?;
        frame_rate(fps)?;
        source_offset(source_offset_us)?;
        if !(8_000..=192_000).contains(&sample_rate) {
            return Err(invalid(
                "pcm_sample_rate",
                "native PCM rate must be within 8..192 kHz",
            ));
        }
        if self.traversal == TimeTraversal::Exact {
            return Ok(None);
        }
        let position = self.frame * f64::from(sample_rate) / f64::from(fps)
            + source_offset_us as f64 * f64::from(sample_rate) / 1_000_000.;
        Ok(Some(if self.traversal == TimeTraversal::Earlier {
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
        parent_fps: crate::FrameRate,
        child_fps: crate::FrameRate,
    ) -> Result<Self> {
        self.validate()?;
        frame_rate(parent_fps)?;
        frame_rate(child_fps)?;
        let frame = clip
            .source_frame(self.frame, parent_fps, child_fps)
            .map_err(|source| Error::LayerTime {
                phase: "composition_source",
                source: Box::new(source),
            })?;
        Self::with_traversal(
            frame,
            parent_fps.map_frame(self.rate, child_fps)?,
            self.traversal,
        )
    }
}
