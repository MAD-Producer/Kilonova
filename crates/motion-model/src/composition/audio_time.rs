//! Immutable per-voice clocks. Only paths containing an authored time mapping
//! use these; ordinary audio retains its existing rational sample arithmetic.
use super::{AudioVoice, CompositionClip};
use crate::{
    layer_time::{LayerClock, SourceTimeSample, TimeRemap},
    AudioLevels, ClipRange, Error, Layer, Result,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
struct Step {
    composition: String,
    object: u64,
    fps: u32,
    frames: u32,
    interval: ClipRange,
    offset: i32,
    clock: LayerClock,
    remap: Option<Arc<TimeRemap>>,
    levels: Option<AudioLevels>,
    child: Option<(CompositionClip, u32)>,
}

#[derive(Clone, Debug)]
pub struct AudioTimePath {
    origin_sample: f64,
    fps: u32,
    steps: Vec<Arc<Step>>,
}
pub(super) struct Sample {
    source: SourceTimeSample,
    fps: u32,
    pub db: [f32; 2],
    pub balance: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct AudioMappedSample {
    pub position: f64,
    pub gain: [f32; 2],
}

impl AudioTimePath {
    pub(super) fn animated_levels(&self) -> bool {
        self.steps
            .iter()
            .any(|step| step.levels.as_ref().is_some_and(AudioLevels::animated))
    }
    pub(super) fn new(origin_sample: f64, fps: u32) -> Self {
        Self {
            origin_sample,
            fps,
            steps: Vec::new(),
        }
    }
    pub(super) fn with_layer(
        mut self,
        layer: &Layer,
        composition: &str,
        fps: u32,
        frames: u32,
        child: Option<(CompositionClip, u32)>,
    ) -> Self {
        let mapping = layer.time_mapping.as_ref();
        self.steps.push(Arc::new(Step {
            composition: composition.chars().take(64).collect(),
            object: layer.id,
            fps,
            frames,
            interval: layer.interval(frames),
            offset: layer.timeline.map_or(0, |t| t.offset_frame),
            clock: mapping.map_or_else(LayerClock::default, |m| m.clock),
            remap: mapping
                .and_then(|m| m.remap.as_ref())
                .map(|r| Arc::new(r.clone())),
            levels: layer.audio_levels.clone(),
            child,
        }));
        self
    }
    pub(super) fn sample(&self, sample: u64) -> Result<Option<Sample>> {
        let mut cursor = SourceTimeSample::new(
            (sample as f64 - self.origin_sample) * f64::from(self.fps) / 48_000.,
            f64::from(self.fps) / 48_000.,
        )?;
        let mut result = Sample {
            source: cursor,
            fps: self.fps,
            db: [0.; 2],
            balance: [1.; 2],
        };
        for step in &self.steps {
            if !cursor.in_interval(0., f64::from(step.frames))
                || !cursor.in_interval(step.interval.in_frame, step.interval.out_frame)
            {
                return Ok(None);
            }
            let next = (|| {
                if let Some(levels) = &step.levels {
                    let local = step.clock.local_frame(cursor.frame, step.offset)?;
                    let values = levels.sample(local)?;
                    let balance = levels.balance_gain();
                    for channel in 0..2 {
                        result.db[channel] += values[channel];
                        result.balance[channel] *= balance[channel];
                    }
                }
                let source = cursor.through(step.clock, step.offset, step.remap.as_deref())?;
                match &step.child {
                    Some((clip, child_fps)) => {
                        source.through_composition(clip, step.fps, *child_fps)
                    }
                    None => Ok(source),
                }
            })()
            .map_err(|source| Error::AudioTimeline {
                composition: step.composition.clone(),
                object: step.object,
                phase: "mapped_sample",
                source: Box::new(source),
            })?;
            cursor = next;
            result.fps = step.child.as_ref().map_or(step.fps, |(_, fps)| *fps);
        }
        result.source = cursor;
        Ok(Some(result))
    }
}

impl AudioVoice {
    pub fn mapped_sample_at(
        &self,
        output_sample: u64,
        sample_rate: u32,
    ) -> Result<Option<AudioMappedSample>> {
        let path = self
            .time_mapping
            .as_ref()
            .ok_or_else(|| Error::Invalid("audio voice has no mapped source clock".into()))?;
        if output_sample < self.begin_sample || output_sample >= self.end_sample {
            return Ok(None);
        }
        let Some(mapped) = path.sample(output_sample)? else {
            return Ok(None);
        };
        let Some(position) =
            mapped
                .source
                .pcm_position(mapped.fps, sample_rate, self.source_offset_us)?
        else {
            return Ok(None);
        };
        Ok(Some(AudioMappedSample {
            position,
            gain: self.gain_with(output_sample, mapped.db, mapped.balance)?,
        }))
    }
}
