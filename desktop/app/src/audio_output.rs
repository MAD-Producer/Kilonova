//! Real device playback. The callback only touches a bounded atomic SPSC queue;
//! project evaluation, PCM disk reads and device-rate conversion run elsewhere.
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, SampleFormat, SizedSample,
};
use motion_core::Project;
use motion_media::AudioMixer;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const QUEUE_FRAMES: usize = 8192;
const MIX_FRAMES: usize = 4096;
struct Ring {
    samples: Vec<[AtomicU32; 2]>,
    read: AtomicUsize,
    write: AtomicUsize,
}
impl Ring {
    fn new() -> Self {
        Self {
            samples: (0..QUEUE_FRAMES)
                .map(|_| [AtomicU32::new(0), AtomicU32::new(0)])
                .collect(),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
        }
    }
    fn push(&self, frame: [f32; 2]) -> bool {
        let write = self.write.load(Ordering::Relaxed);
        if write.wrapping_sub(self.read.load(Ordering::Acquire)) == QUEUE_FRAMES {
            return false;
        }
        let slot = &self.samples[write % QUEUE_FRAMES];
        slot[0].store(frame[0].to_bits(), Ordering::Relaxed);
        slot[1].store(frame[1].to_bits(), Ordering::Relaxed);
        self.write.store(write.wrapping_add(1), Ordering::Release);
        true
    }
    fn pop(&self) -> Option<[f32; 2]> {
        let read = self.read.load(Ordering::Relaxed);
        if read == self.write.load(Ordering::Acquire) {
            return None;
        }
        let slot = &self.samples[read % QUEUE_FRAMES];
        let result = [
            f32::from_bits(slot[0].load(Ordering::Relaxed)),
            f32::from_bits(slot[1].load(Ordering::Relaxed)),
        ];
        self.read.store(read.wrapping_add(1), Ordering::Release);
        Some(result)
    }
}
struct Shared {
    ring: Ring,
    stop: AtomicBool,
    done: AtomicBool,
    error: Mutex<Option<String>>,
    epoch: Instant,
    sequence: AtomicU64,
    anchor_frames: AtomicU64,
    anchor_ns: AtomicU64,
    submitted_frames: AtomicU64,
    observed_frames: AtomicU64,
    underruns: AtomicU64,
}
pub struct AudioOutput {
    stream: Option<cpal::Stream>,
    producer: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
    rate: u32,
    start_sample: u64,
    total_samples: u64,
}

impl AudioOutput {
    pub fn start(project: Project, root: &Path, start_frame: f64) -> Result<Self, String> {
        if !start_frame.is_finite() || start_frame < 0.0 || start_frame >= f64::from(project.frames)
        {
            return Err("audio start outside composition".into());
        }
        let start_sample = (start_frame * 48_000.0 / f64::from(project.fps)).floor() as u64;
        let mixer = AudioMixer::new(project, root)?;
        let total_samples = mixer.total_frames();
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device is available")?;
        let supported = device
            .supported_output_configs()
            .map_err(|e| format!("query audio output: {e}"))?
            .filter(|c| {
                c.channels() == 2
                    && c.min_sample_rate().0 <= 48_000
                    && c.max_sample_rate().0 >= 48_000
                    && matches!(
                        c.sample_format(),
                        SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16
                    )
            })
            .max_by_key(|c| match c.sample_format() {
                SampleFormat::F32 => 2,
                SampleFormat::I16 => 1,
                _ => 0,
            })
            .map(|c| c.with_sample_rate(cpal::SampleRate(48_000)))
            .map(Ok)
            .unwrap_or_else(|| {
                device
                    .default_output_config()
                    .map_err(|e| format!("default audio output: {e}"))
            })?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let rate = config.sample_rate.0;
        if rate == 0 || config.channels == 0 {
            return Err("audio device reported an invalid configuration".into());
        }
        let shared = Arc::new(Shared {
            ring: Ring::new(),
            stop: AtomicBool::new(false),
            done: AtomicBool::new(false),
            error: Mutex::new(None),
            epoch: Instant::now(),
            sequence: AtomicU64::new(0),
            anchor_frames: AtomicU64::new(0),
            anchor_ns: AtomicU64::new(0),
            submitted_frames: AtomicU64::new(0),
            observed_frames: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
        });
        let mut source = Producer::new(mixer, start_sample, rate);
        // Prime before starting the device. This cannot execute on its callback.
        for _ in 0..MIX_FRAMES {
            match source.next()? {
                Some(frame) => {
                    shared.ring.push(frame);
                }
                None => {
                    shared.done.store(true, Ordering::Release);
                    break;
                }
            }
        }
        let stream = match sample_format {
            SampleFormat::F32 => build::<f32>(&device, &config, shared.clone()),
            SampleFormat::F64 => build::<f64>(&device, &config, shared.clone()),
            SampleFormat::I8 => build::<i8>(&device, &config, shared.clone()),
            SampleFormat::I16 => build::<i16>(&device, &config, shared.clone()),
            SampleFormat::I32 => build::<i32>(&device, &config, shared.clone()),
            SampleFormat::I64 => build::<i64>(&device, &config, shared.clone()),
            SampleFormat::U8 => build::<u8>(&device, &config, shared.clone()),
            SampleFormat::U16 => build::<u16>(&device, &config, shared.clone()),
            SampleFormat::U32 => build::<u32>(&device, &config, shared.clone()),
            SampleFormat::U64 => build::<u64>(&device, &config, shared.clone()),
            other => Err(format!("unsupported audio output format: {other}")),
        }?;
        let worker = shared.clone();
        let producer = std::thread::Builder::new()
            .name("motion-audio-mix".into())
            .spawn(move || {
                while !worker.stop.load(Ordering::Acquire) {
                    let frame = match source.next() {
                        Ok(Some(frame)) => frame,
                        Ok(None) => {
                            worker.done.store(true, Ordering::Release);
                            return;
                        }
                        Err(error) => {
                            *worker.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
                            worker.done.store(true, Ordering::Release);
                            return;
                        }
                    };
                    while !worker.ring.push(frame) {
                        if worker.stop.load(Ordering::Acquire) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let mut output = Self {
            stream: Some(stream),
            producer: Some(producer),
            shared,
            rate,
            start_sample,
            total_samples,
        };
        if let Err(error) = output.stream.as_ref().unwrap().play() {
            output.stop();
            return Err(format!("start audio output: {error}"));
        }
        Ok(output)
    }

    /// Use this for the video playhead. It follows PCM submitted to the device,
    /// compensates output latency and freezes when the producer underruns.
    pub fn frame(&self, fps: u32) -> f64 {
        let (anchor, at, submitted) = loop {
            let version = self.shared.sequence.load(Ordering::Acquire);
            if version % 2 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let values = (
                self.shared.anchor_frames.load(Ordering::Relaxed),
                self.shared.anchor_ns.load(Ordering::Relaxed),
                self.shared.submitted_frames.load(Ordering::Relaxed),
            );
            if self.shared.sequence.load(Ordering::Acquire) == version {
                break values;
            }
        };
        let now = self
            .shared
            .epoch
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let delta = (u128::from(now.abs_diff(at)) * u128::from(self.rate) / 1_000_000_000) as u64;
        let extrapolated = if now >= at {
            anchor.saturating_add(delta)
        } else {
            anchor.saturating_sub(delta)
        }
        .min(submitted);
        let device_frames = self
            .shared
            .observed_frames
            .fetch_max(extrapolated, Ordering::AcqRel)
            .max(extrapolated);
        let sample =
            self.start_sample as f64 + device_frames as f64 * 48_000.0 / f64::from(self.rate);
        sample.min(self.total_samples as f64) * f64::from(fps) / 48_000.0
    }
    pub fn error(&self) -> Option<String> {
        self.shared
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.stream.take();
        if let Some(worker) = self.producer.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.stop();
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, String> {
    let channels = usize::from(config.channels);
    let error_state = shared.clone();
    let mut consumed = 0u64;
    device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let before = consumed;
                let mut underrun = false;
                for target in data.chunks_mut(channels) {
                    let pcm = if underrun || shared.stop.load(Ordering::Relaxed) {
                        None
                    } else {
                        shared.ring.pop()
                    };
                    if let Some(pcm) = pcm {
                        consumed += 1;
                        for (channel, sample) in target.iter_mut().enumerate() {
                            let value = if channels == 1 {
                                (pcm[0] + pcm[1]) * 0.5
                            } else {
                                match channel {
                                    0 => pcm[0],
                                    1 => pcm[1],
                                    _ => 0.0,
                                }
                            };
                            *sample = T::from_sample(value);
                        }
                    } else {
                        // Keep each callback's source samples contiguous. If the
                        // producer refills midway through this loop, consume the new
                        // samples on the next callback with its actual playback PTS.
                        underrun = true;
                        target.fill(T::EQUILIBRIUM);
                    }
                }
                if consumed > before {
                    let timestamp = info.timestamp();
                    let latency = timestamp
                        .playback
                        .duration_since(&timestamp.callback)
                        .unwrap_or_default();
                    shared.sequence.fetch_add(1, Ordering::AcqRel);
                    shared.anchor_frames.store(before, Ordering::Relaxed);
                    shared.anchor_ns.store(
                        (shared.epoch.elapsed() + latency)
                            .as_nanos()
                            .min(u128::from(u64::MAX)) as u64,
                        Ordering::Relaxed,
                    );
                    shared.submitted_frames.store(consumed, Ordering::Relaxed);
                    shared.sequence.fetch_add(1, Ordering::Release);
                }
                if consumed - before < (data.len() / channels) as u64
                    && !shared.done.load(Ordering::Acquire)
                    && !shared.stop.load(Ordering::Relaxed)
                {
                    shared.underruns.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |error| {
                *error_state.error.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(format!("audio output: {error}"));
            },
            None,
        )
        .map_err(|e| format!("open audio output: {e}"))
}

struct Producer {
    mixer: AudioMixer,
    buffer: Vec<f32>,
    buffer_start: u64,
    buffer_count: usize,
    source_start: u64,
    output_frame: u64,
    output_rate: u32,
}
impl Producer {
    fn new(mixer: AudioMixer, source_start: u64, output_rate: u32) -> Self {
        Self {
            mixer,
            buffer: vec![0.0; MIX_FRAMES * 2],
            buffer_start: u64::MAX,
            buffer_count: 0,
            source_start,
            output_frame: 0,
            output_rate,
        }
    }
    fn sample(&mut self, sample: u64) -> Result<[f32; 2], String> {
        if sample >= self.mixer.total_frames() {
            return Ok([0.0; 2]);
        }
        if self.buffer_start == u64::MAX
            || sample < self.buffer_start
            || sample >= self.buffer_start + self.buffer_count as u64
        {
            self.buffer_start = sample;
            self.buffer_count = self.mixer.mix(sample, &mut self.buffer)?;
        }
        let at = (sample - self.buffer_start) as usize * 2;
        Ok([self.buffer[at], self.buffer[at + 1]])
    }
    fn next(&mut self) -> Result<Option<[f32; 2]>, String> {
        let numerator = u128::from(self.output_frame) * 48_000;
        let index = self.source_start + (numerator / u128::from(self.output_rate)) as u64;
        if index >= self.mixer.total_frames() {
            return Ok(None);
        }
        let a = self.sample(index)?;
        let fraction = (numerator % u128::from(self.output_rate)) as f32 / self.output_rate as f32;
        let b = if fraction > 0.0 {
            self.sample(index + 1)?
        } else {
            a
        };
        self.output_frame += 1;
        Ok(Some([
            a[0] + (b[0] - a[0]) * fraction,
            a[1] + (b[1] - a[1]) * fraction,
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ring_preserves_stereo_order_across_wrap_and_full_queue() {
        let ring = Ring::new();
        for i in 0..QUEUE_FRAMES {
            assert!(ring.push([i as f32, -(i as f32)]));
        }
        assert!(!ring.push([1.0, 1.0]));
        for i in 0..QUEUE_FRAMES {
            assert_eq!(ring.pop(), Some([i as f32, -(i as f32)]));
        }
        assert!(ring.pop().is_none());
        assert!(ring.push([0.25, -0.5]));
        assert_eq!(ring.pop(), Some([0.25, -0.5]));
    }

    #[test]
    fn producer_and_callback_queue_preserve_samples_during_concurrent_wrap() {
        let ring = Arc::new(Ring::new());
        let writer = ring.clone();
        let count = QUEUE_FRAMES * 4;
        let worker = std::thread::spawn(move || {
            for i in 0..count {
                while !writer.push([i as f32, -(i as f32)]) {
                    std::thread::yield_now();
                }
            }
        });
        for i in 0..count {
            let value = loop {
                if let Some(value) = ring.pop() {
                    break value;
                }
                std::thread::yield_now();
            };
            assert_eq!(value, [i as f32, -(i as f32)]);
        }
        worker.join().unwrap();
        assert!(ring.pop().is_none());
    }

    #[test]
    #[ignore = "requires an actual native audio output device"]
    fn real_output_device_clock_advances_and_reaches_composition_end() {
        let root = tempfile::tempdir().unwrap();
        let project = Project::new(64, 64, 30, 15).unwrap();
        let mut output = AudioOutput::start(project, root.path(), 0.0).unwrap();
        std::thread::sleep(Duration::from_millis(180));
        let first = output.frame(30);
        assert!(first > 1.0 && first < 7.0, "real device clock={first}");
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            output.frame(30) >= 14.9,
            "device clock must reach the last sample"
        );
        assert!(
            output.error().is_none(),
            "audio device error={:?}",
            output.error()
        );
        output.stop();
    }
}
