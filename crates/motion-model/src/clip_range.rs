//! Exact layer clip intervals. Integer timeline bounds cover the exact range.
use crate::{ensure, Layer, LayerTimeline, Result, MAX_FRAMES};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipRange {
    pub in_frame: f64,
    pub out_frame: f64,
}
impl ClipRange {
    pub fn validate(self) -> Result<()> {
        ensure(
            self.in_frame.is_finite()
                && self.out_frame.is_finite()
                && self.in_frame < self.out_frame
                && self.in_frame >= -f64::from(MAX_FRAMES)
                && self.out_frame <= f64::from(MAX_FRAMES),
            "invalid precise layer clip interval",
        )
    }
    pub fn fractional(self) -> bool {
        self.in_frame.fract() != 0. || self.out_frame.fract() != 0.
    }
    pub fn translated(self, delta: i64) -> Self {
        Self {
            in_frame: self.in_frame + delta as f64,
            out_frame: self.out_frame + delta as f64,
        }
    }
    pub fn covers(self, timeline: LayerTimeline) -> bool {
        self.in_frame.floor() == f64::from(timeline.in_frame)
            && self.out_frame.ceil() == f64::from(timeline.out_frame)
    }
}
impl Layer {
    pub fn interval(&self, frames: u32) -> ClipRange {
        self.subframe_range.unwrap_or_else(|| {
            let t = self.clip(frames);
            ClipRange {
                in_frame: f64::from(t.in_frame),
                out_frame: f64::from(t.out_frame),
            }
        })
    }
    pub fn set_interval(&mut self, frames: u32, range: ClipRange) -> Result<()> {
        range.validate()?;
        let mut t = self.clip(frames);
        t.in_frame = range.in_frame.floor() as i32;
        t.out_frame = range.out_frame.ceil() as i32;
        t.validate(frames)?;
        self.timeline = Some(t);
        self.subframe_range = Some(range);
        Ok(())
    }
}
