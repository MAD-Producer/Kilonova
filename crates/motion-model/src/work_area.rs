//! Composition-local, half-open frame bounds shared by preview and export.
use crate::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameScope {
    #[default]
    Composition,
    WorkArea,
}

impl crate::Project {
    pub fn frame_range(&self, scope: FrameScope) -> WorkArea {
        match scope {
            FrameScope::Composition => WorkArea::full(self.frames),
            FrameScope::WorkArea => self.work_area,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkArea {
    pub in_frame: u32,
    /// Exclusive: the last included frame is `out_frame - 1`.
    pub out_frame: u32,
}

impl WorkArea {
    pub fn full(frames: u32) -> Self {
        Self {
            in_frame: 0,
            out_frame: frames,
        }
    }

    pub fn validate(self, frames: u32) -> Result<()> {
        ensure(
            self.in_frame < self.out_frame && self.out_frame <= frames,
            &format!(
                "invalid work area: in_frame={} out_frame={} composition_frames={frames}; expected 0 <= in_frame < out_frame <= composition_frames",
                self.in_frame, self.out_frame
            ),
        )
    }

    /// Integer conversion of absolute edges avoids losing fractional samples
    /// by multiplying a rounded samples-per-frame or rounding the duration.
    pub fn sample_bounds(self, fps: u32, sample_rate: u32) -> Result<std::ops::Range<u64>> {
        ensure(fps > 0 && sample_rate > 0, "invalid work area audio clock")?;
        ensure(self.in_frame < self.out_frame, "empty work area")?;
        let edge = |frame| u64::from(frame) * u64::from(sample_rate) / u64::from(fps);
        Ok(edge(self.in_frame)..edge(self.out_frame))
    }

    /// Preserve a full-range selection when the composition grows. A custom
    /// range follows the settings timing policy, then clamps to at least one
    /// valid frame when duration decreases. This never edits layer contents.
    pub(crate) fn resized(
        self,
        old_frames: u32,
        old_fps: u32,
        frames: u32,
        fps: u32,
        seconds: bool,
    ) -> Result<Self> {
        ensure(
            old_fps > 0 && fps > 0 && frames > 0,
            "invalid work area resize clock or duration",
        )?;
        self.validate(old_frames)?;
        if self == Self::full(old_frames) {
            return Ok(Self::full(frames));
        }
        let edge = |frame| {
            if seconds {
                ((u64::from(frame) * u64::from(fps) + u64::from(old_fps) / 2) / u64::from(old_fps))
                    .min(u64::from(frames)) as u32
            } else {
                frame
            }
        };
        let in_frame = edge(self.in_frame).min(frames - 1);
        let out_frame = edge(self.out_frame).clamp(in_frame + 1, frames);
        Ok(Self {
            in_frame,
            out_frame,
        })
    }
}
