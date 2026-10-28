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
const CALLBACK_TIMEOUT_NS: u64 = 2_000_000_000;
const CLOCK_READ_ATTEMPTS: usize = 32;
static NEXT_OBSERVATION: AtomicU64 = AtomicU64::new(1);
mod meter;
pub(crate) use meter::Levels;
#[cfg(test)]
pub(crate) fn meter_probe(pcm: &[[f32; 2]]) -> Levels {
    let shared = Shared::new();
    shared.ready.store(true, Ordering::Release);
    for &frame in pcm { assert!(shared.ring.push(frame)); }
    let mut output = vec![0f32; pcm.len() * 2];
    render_callback(&shared, &mut output, 2, &mut 0, 0, 0);
    shared.meter.take()
}
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
// Realtime callbacks publish only atomics. The watchdog drains these two bits;
// multiple episodes between polls coalesce, without losing escalation evidence.
#[derive(Default)]
struct CallbackEvidence { count: AtomicUsize, pending: AtomicUsize }
impl CallbackEvidence {
    fn failed(&self) {
        let previous = self.count.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| Some(n.saturating_add(1))).unwrap_or_else(|n| n);
        let bit = match previous { 0 => 1, 59 => 2, _ => 0 };
        if bit != 0 { self.pending.fetch_or(bit, Ordering::Release); }
    }
    fn success(&self) { self.count.store(0, Ordering::Relaxed); }
    fn drain(&self, site: &'static str, playback: u64, reason: &'static str, fallback: &'static str) {
        let flags = self.pending.swap(0, Ordering::AcqRel);
        if flags & 1 != 0 { log::warn!("audio degradation site={site} count=1 playback={playback} reason={reason} fallback={fallback}"); }
        if flags & 2 != 0 { log::error!("persistent audio degradation site={site} count=60 playback={playback} reason={reason} fallback={fallback}"); }
    }
}
struct Shared {
    meter: meter::Meter,
    observation: u64,
    latency_evidence: CallbackEvidence,
    error_poison_evidence: CallbackEvidence,
    ring: Ring,
    stop: AtomicBool,
    done: AtomicBool,
    ready: AtomicBool,
    started: AtomicBool,
    error: Mutex<Option<String>>,
    epoch: Instant,
    #[cfg(test)]
    test_now_ns: Option<AtomicU64>,
    sequence: AtomicU64,
    anchor_frames: AtomicU64,
    anchor_ns: AtomicU64,
    submitted_frames: AtomicU64,
    observed_frames: AtomicU64,
    underruns: AtomicU64,
    last_callback_ns: AtomicU64,
}
impl Shared {
    fn new() -> Self {
        Self {
            meter: meter::Meter::default(),
            observation: NEXT_OBSERVATION.fetch_add(1, Ordering::Relaxed),
            latency_evidence: CallbackEvidence::default(),
            error_poison_evidence: CallbackEvidence::default(),
            ring: Ring::new(),
            stop: AtomicBool::new(false),
            done: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            started: AtomicBool::new(false),
            error: Mutex::new(None),
            epoch: Instant::now(),
            #[cfg(test)]
            test_now_ns: None,
            sequence: AtomicU64::new(0),
            anchor_frames: AtomicU64::new(0),
            anchor_ns: AtomicU64::new(0),
            submitted_frames: AtomicU64::new(0),
            observed_frames: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            last_callback_ns: AtomicU64::new(0),
        }
    }
    fn callback_latency(&self, latency: Option<Duration>) -> u64 {
        match latency {
            Some(duration) => { self.latency_evidence.success(); duration.as_nanos().min(u128::from(u64::MAX)) as u64 }
            None => { self.latency_evidence.failed(); 0 }
        }
    }
    fn drain_callback_evidence(&self) {
        self.latency_evidence.drain("audio.callback.timestamp_order", self.observation, "playback_before_callback", "zero_latency");
        self.error_poison_evidence.drain("audio.error.publish", self.observation, "mutex_poisoned", "into_inner");
    }
    fn now_ns(&self) -> u64 {
        #[cfg(test)]
        if let Some(now) = &self.test_now_ns {
            return now.load(Ordering::Relaxed);
        }
        self.epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
    }
    fn fail(&self, message: String) {
        let mut error = self.error.lock().unwrap_or_else(|e| { self.error_poison_evidence.failed(); e.into_inner() });
        if error.is_none() {
            *error = Some(message);
        }
        self.stop.store(true, Ordering::Release);
    }
    fn check_callback_at(&self, now: u64) {
        if self.started.load(Ordering::Acquire)
            && !self.stop.load(Ordering::Acquire)
            && now.saturating_sub(self.last_callback_ns.load(Ordering::Acquire))
                >= CALLBACK_TIMEOUT_NS
        {
            self.fail("audio output stopped responding (no device callback for 2 seconds)".into());
        }
    }
    fn begin_playback_at(&self, now: u64) {
        self.last_callback_ns.store(now, Ordering::Release);
        self.started.store(true, Ordering::Release);
    }
    fn publish_clock(&self, before: u64, consumed: u64, playback_ns: u64) {
        // Publish submitted position even for silence. Only source samples
        // establish a new time anchor: reanchoring an empty buffer would keep
        // pushing the final sample into the future by the output latency.
        self.sequence.fetch_add(1, Ordering::AcqRel);
        if consumed > before {
            self.anchor_frames.store(before, Ordering::Relaxed);
            self.anchor_ns.store(playback_ns, Ordering::Relaxed);
        }
        self.submitted_frames.store(consumed, Ordering::Relaxed);
        self.sequence.fetch_add(1, Ordering::Release);
    }
    fn device_frames_at(&self, now: u64, rate: u32) -> u64 {
        if self.stop.load(Ordering::Acquire) {
            return self.observed_frames.load(Ordering::Acquire);
        }
        for attempt in 0..CLOCK_READ_ATTEMPTS {
            let version = self.sequence.load(Ordering::Acquire);
            if version % 2 == 0 {
                let anchor = self.anchor_frames.load(Ordering::Relaxed);
                let at = self.anchor_ns.load(Ordering::Relaxed);
                let submitted = self.submitted_frames.load(Ordering::Relaxed);
                if self.sequence.load(Ordering::Acquire) == version {
                    let delta = (u128::from(now.abs_diff(at)) * u128::from(rate) / 1_000_000_000)
                        .min(u128::from(u64::MAX)) as u64;
                    let extrapolated = if now >= at {
                        anchor.saturating_add(delta)
                    } else {
                        anchor.saturating_sub(delta)
                    }
                    .min(submitted);
                    // fetch_max returns the OLD value, which can exceed a newly
                    // latency-adjusted anchor after an underrun. Keep both.
                    return self
                        .observed_frames
                        .fetch_max(extrapolated, Ordering::AcqRel)
                        .max(extrapolated);
                }
            }
            if attempt % 8 == 7 {
                std::thread::yield_now();
            } else {
                std::hint::spin_loop();
            }
        }
        // A preempted callback must not keep the UI in an unbounded seqlock spin.
        self.observed_frames.load(Ordering::Acquire)
    }
}
pub struct AudioOutput {
    stream: Option<cpal::Stream>,
    producer: Option<JoinHandle<()>>,
    monitor: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
    rate: u32,
    start_sample: u64,
    total_samples: u64,
    range_start: u64,
    looping: bool,
}

impl AudioOutput {
    #[cfg(test)]
    pub fn start(project: Project, root: &Path, start_frame: f64) -> Result<Self, String> {
        let range = motion_core::WorkArea::full(project.frames);
        Self::start_in_range(project, root, start_frame, range, false)
    }

    pub fn start_in_range(
        project: Project, root: &Path, start_frame: f64,
        range: motion_core::WorkArea, looping: bool,
    ) -> Result<Self, String> {
        let began = Instant::now();
        let fps = project.fps;
        log::info!("audio preview start phase=start fps={fps} in_frame={} out_frame={} looping={looping}", range.in_frame, range.out_frame);
        let result = Self::start_inner(project, root, start_frame, range, looping);
        if let Err(error) = &result {
            log::error!("audio preview start failed phase=start elapsed_ms={} fps={fps} in_frame={} out_frame={} looping={looping} cause={error}", began.elapsed().as_millis(), range.in_frame, range.out_frame);
        } else {
            log::debug!("audio preview ready elapsed_ms={} fps={fps} in_frame={} out_frame={} looping={looping}", began.elapsed().as_millis(), range.in_frame, range.out_frame);
        }
        result
    }

    fn start_inner(project: Project, root: &Path, start_frame: f64, range: motion_core::WorkArea, looping: bool) -> Result<Self, String> {
        project.validate().map_err(|e| e.to_string())?;
        range.validate(project.frames).map_err(|error| error.to_string())?;
        if !start_frame.is_finite() || start_frame < f64::from(range.in_frame) || start_frame >= f64::from(range.out_frame)
        {
            return Err("audio start outside composition".into());
        }
        let start_sample = (start_frame * 48_000.0 / f64::from(project.fps)).floor() as u64;
        let samples = range.sample_bounds(project.fps, 48_000).map_err(|error| error.to_string())?;
        let range_start = samples.start;
        let total_samples = samples.end;
        let root = root.to_path_buf();
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
        let shared = Arc::new(Shared::new());
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
        let mut source = None;
        let mut project = Some(project);
        let producer = spawn_producer(shared.clone(), move || {
            // Initialization also opens PCM files; do all of it on this worker.
            if source.is_none() {
                source = Some(Producer::in_range(
                    AudioMixer::new(project.take().unwrap(), &root)?,
                    start_sample,
                    rate,
                    range_start..total_samples,
                    looping,
                )?);
            }
            source.as_mut().unwrap().next()
        })?;
        let mut output = Self {
            stream: Some(stream),
            producer: Some(producer),
            monitor: None,
            shared,
            rate,
            start_sample,
            total_samples,
            range_start,
            looping,
        };
        output.shared.begin_playback_at(output.shared.now_ns());
        if let Err(error) = output.stream.as_ref().unwrap().play() {
            output.stop();
            return Err(format!("start audio output: {error}"));
        }
        output.monitor = Some(spawn_monitor(output.shared.clone())?);
        Ok(output)
    }

    /// Use this for the video playhead. It follows PCM submitted to the device,
    /// compensates output latency and freezes when the producer underruns.
    pub fn frame(&self, fps: u32) -> f64 {
        self.shared.check_callback_at(self.shared.now_ns());
        let device_frames = self
            .shared
            .device_frames_at(self.shared.now_ns(), self.rate);
        let sample =
            self.start_sample as f64 + device_frames as f64 * 48_000.0 / f64::from(self.rate);
        let sample = if self.looping {
            self.range_start as f64 + (sample - self.range_start as f64)
                .rem_euclid((self.total_samples - self.range_start) as f64)
        } else { sample.min(self.total_samples as f64) };
        sample * f64::from(fps) / 48_000.0
    }
    pub(crate) fn take_levels(&self) -> Levels {
        self.shared.meter.take()
    }
    pub fn error(&self) -> Option<String> {
        self.shared.check_callback_at(self.shared.now_ns());
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(&self.shared.error, &FAILURES, "audio.error.read", |value| value.clone())
    }
    pub fn finished(&self, frame: f64, fps: u32) -> bool {
        !self.looping && frame >= (self.total_samples.saturating_sub(1)) as f64 * f64::from(fps) / 48_000.
    }
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.stream.take();
        // A mixer can still be inside a blocking disk read. Its Arc and owned
        // file cursors outlive this output; cancellation is checked on return.
        // Detach unfinished workers instead of joining them on the UI thread.
        for worker in [self.producer.take(), self.monitor.take()]
            .into_iter()
            .flatten()
        {
            worker.thread().unpark();
            if worker.is_finished() {
                if let Err(payload) = worker.join() {
                    log::error!("audio worker join failed: {}", motion_media::panic_reason(payload));
                }
            }
        }
    }
}
impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_producer(
    shared: Arc<Shared>,
    mut next: impl FnMut() -> Result<Option<[f32; 2]>, String> + Send + 'static,
) -> Result<JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("motion-audio-mix".into())
        .spawn(move || {
            let mut observation = producer_observation::Observation::new(shared.observation);
            let mut primed = 0;
            while !shared.stop.load(Ordering::Acquire) {
                let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| next()))
                {
                    Ok(result) => result,
                    Err(payload) => {
                        let error = format!("audio mixer worker panicked: {}",motion_media::panic_reason(payload));
                        shared.fail(error.clone());
                        observation.failed("panic", &error);
                        return;
                    }
                };
                let frame = match result {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        shared.done.store(true, Ordering::Release);
                        shared.ready.store(true, Ordering::Release);
                        observation.stopped("eof");
                        return;
                    }
                    Err(error) => {
                        let error = format!("audio mix: {error}");
                        shared.fail(error.clone());
                        observation.failed("mix_error", &error);
                        return;
                    }
                };
                loop {
                    // Check cancellation after disk IO, including when the
                    // queue still has room. A stopped output must not refill.
                    if shared.stop.load(Ordering::Acquire) {
                        observation.stopped("stop_after_read");
                        return;
                    }
                    if shared.ring.push(frame) {
                        break;
                    }
                    shared.check_callback_at(shared.now_ns());
                    std::thread::park_timeout(Duration::from_millis(10));
                }
                primed += 1;
                if primed == MIX_FRAMES {
                    shared.ready.store(true, Ordering::Release);
                }
            }
            observation.stopped("stop");
        })
        .map_err(|error| {
            log::error!("start audio mixer failed: {error}");
            format!("start audio mixer: {error}")
        })
}

mod producer_observation;

fn spawn_monitor(shared: Arc<Shared>) -> Result<JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("motion-audio-watchdog".into())
        .spawn(move || {
            let mut reported = 0;
            while !shared.stop.load(Ordering::Acquire) {
                shared.check_callback_at(shared.now_ns());
                shared.drain_callback_evidence();
                let underruns = shared.underruns.load(Ordering::Relaxed);
                if underruns != reported {
                    // Diagnostic IO stays off the realtime callback and UI.
                    log::warn!("[motion-audio] underruns={underruns} playback={}", shared.observation);
                    reported = underruns;
                }
                std::thread::park_timeout(Duration::from_millis(250));
            }
            // Device callbacks only publish state. Observe the latched failure
            // here, outside the realtime callback and after releasing its mutex.
            let error = shared.error.lock().unwrap_or_else(|e| { shared.error_poison_evidence.failed(); e.into_inner() }).clone();
            shared.drain_callback_evidence();
            if let Some(error) = error {
                log::error!("[motion-audio] output stopped: {error} playback={}", shared.observation);
            }
            log::debug!("audio watchdog stopped playback={}", shared.observation);
        })
        .map_err(|error| {
            log::error!("start audio watchdog failed: {error}");
            format!("start audio watchdog: {error}")
        })
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
                let now = shared.now_ns();
                let timestamp = info.timestamp();
                let latency = shared.callback_latency(timestamp.playback.duration_since(&timestamp.callback));
                render_callback(&shared, data, channels, &mut consumed, now, latency);
            },
            move |error| {
                error_state.fail(format!("audio output: {error}"));
            },
            None,
        )
        .map_err(|e| format!("open audio output: {e}"))
}

// Inject callback time/latency and buffers in tests without opening hardware.
fn render_callback<T: SizedSample + FromSample<f32>>(
    shared: &Shared,
    data: &mut [T],
    channels: usize,
    consumed: &mut u64,
    now: u64,
    latency: u64,
) {
    shared.last_callback_ns.store(now, Ordering::Release);
    let before = *consumed;
    let ready = shared.ready.load(Ordering::Acquire);
    let mut underrun = false;
    let mut peaks = [0f32; 2];
    for target in data.chunks_mut(channels) {
        let pcm = if underrun || !ready || shared.stop.load(Ordering::Relaxed) {
            None
        } else {
            shared.ring.pop()
        };
        if let Some(pcm) = pcm {
            *consumed += 1;
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
                if channel < 2 { peaks[channel] = peaks[channel].max(value.abs()); }
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
    if channels == 1 { peaks[1] = peaks[0]; }
    shared.meter.publish(peaks);
    shared.publish_clock(before, *consumed, now.saturating_add(latency));
    if *consumed - before < (data.len() / channels) as u64
        && ready
        && !shared.done.load(Ordering::Acquire)
        && !shared.stop.load(Ordering::Relaxed)
    {
        shared.underruns.fetch_add(1, Ordering::Relaxed);
    }
}

struct Producer {
    mixer: AudioMixer,
    buffer: Vec<f32>,
    buffer_start: u64,
    buffer_count: usize,
    source_start: u64,
    output_frame: u64,
    output_rate: u32,
    range: std::ops::Range<u64>,
    looping: bool,
}
impl Producer {
    fn new(mixer: AudioMixer, source_start: u64, output_rate: u32) -> Self {
        let end = mixer.total_frames();
        Self {
            mixer,
            buffer: vec![0.0; MIX_FRAMES * 2],
            buffer_start: u64::MAX,
            buffer_count: 0,
            source_start,
            output_frame: 0,
            output_rate,
            range: 0..end,
            looping: false,
        }
    }
    fn in_range(mixer: AudioMixer, source_start: u64, output_rate: u32, range: std::ops::Range<u64>, looping: bool) -> Result<Self, String> {
        if range.start >= range.end || range.end > mixer.total_frames() || !range.contains(&source_start) || output_rate == 0 {
            return Err("audio playback range or output rate is invalid".into());
        }
        let mut producer = Self::new(mixer, source_start, output_rate);
        producer.range = range;
        producer.looping = looping;
        Ok(producer)
    }
    fn sample(&mut self, sample: u64) -> Result<[f32; 2], String> {
        let sample = match self.source_index(sample) { Some(sample) => sample, None => return Ok([0.; 2]) };
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
        if self.source_index(index).is_none() {
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
    fn source_index(&self, index: u64) -> Option<u64> {
        if index < self.range.start { return None; }
        if index < self.range.end { return Some(index); }
        self.looping.then(|| self.range.start + (index - self.range.start) % (self.range.end - self.range.start))
    }
}

#[cfg(test)]
mod playhead_tests;
#[cfg(test)]
mod work_area_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn output_without_device(shared: Arc<Shared>, producer: Option<JoinHandle<()>>) -> AudioOutput {
        shared.begin_playback_at(0);
        AudioOutput {
            stream: None,
            producer,
            monitor: None,
            shared,
            rate: 48_000,
            start_sample: 0,
            total_samples: 48_000,
            range_start: 0,
            looping: false,
        }
    }

    #[test]
    fn skipped_audio_playback_stops_on_the_same_last_frame_as_the_wall_clock() {
        let root = tempfile::tempdir().unwrap();
        let mut shell = crate::Shell::new(root.path().join("project"),None,Some(crate::i18n::Locale::En)).unwrap();
        shell.state = shell.engine.edit(serde_json::to_string(&motion_core::Command::SetWorkArea {in_frame:20,out_frame:50}).unwrap()).unwrap();
        shell.workspace.preview_playback = crate::preview_playback::Options {
            scope:motion_core::FrameScope::WorkArea, looping:false,skip:crate::preview_playback::Skip::Four,..Default::default()
        };
        let mut audio = output_without_device(Arc::new(Shared::new()),None);
        audio.start_sample = 80_000 - 1; audio.range_start = 32_000; audio.total_samples = 80_000;
        shell.audio = Some(audio); shell.playback = Some((Instant::now(),20.));
        shell.advance_playback().unwrap();
        assert_eq!(shell.frame,49.); assert!(shell.playback.is_none() && shell.audio.is_none());
        let (_,reply) = shell.editing.take().unwrap();
        assert_eq!(reply.recv_timeout(Duration::from_secs(10)).unwrap().unwrap()["frame"],49.);
    }

    #[test]
    fn injected_underrun_and_recovery_keep_playhead_monotonic() {
        let shared = Shared::new();
        shared.ready.store(true, Ordering::Release);
        for _ in 0..4_800 {
            assert!(shared.ring.push([0.25, -0.5]));
        }
        let mut consumed = 0;
        let mut pcm = vec![0.0f32; 4_800 * 2];
        render_callback(&shared, &mut pcm, 2, &mut consumed, 0, 20_000_000);
        assert_eq!(&pcm[..2], &[0.25, -0.5]);
        assert_eq!(shared.device_frames_at(50_000_000, 48_000), 1_440);

        // An empty callback preserves the last source timestamp and outputs
        // silence without consuming or replaying source samples.
        render_callback(&shared, &mut pcm, 2, &mut consumed, 100_000_000, 20_000_000);
        assert!(pcm.iter().all(|sample| *sample == 0.0));
        assert_eq!(consumed, 4_800);
        assert_eq!(shared.anchor_ns.load(Ordering::Relaxed), 20_000_000);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
        assert_eq!(shared.device_frames_at(150_000_000, 48_000), 4_800);

        for _ in 0..960 {
            assert!(shared.ring.push([0.5, -0.25]));
        }
        let mut recovered = vec![0.0f32; 960 * 2];
        render_callback(
            &shared,
            &mut recovered,
            2,
            &mut consumed,
            160_000_000,
            20_000_000,
        );
        // The new anchor extrapolates to 3,840. fetch_max's old value is 4,800:
        // discarding it would make the video visibly rewind on recovery.
        assert_eq!(shared.device_frames_at(160_000_000, 48_000), 4_800);
        assert_eq!(shared.device_frames_at(200_000_000, 48_000), 5_760);
        assert_eq!(shared.device_frames_at(250_000_000, 48_000), 5_760);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn startup_and_composition_end_silence_are_not_underruns() {
        let shared = Shared::new();
        let mut consumed = 0;
        let mut pcm = [1.0f32; 8];
        render_callback(&shared, &mut pcm, 2, &mut consumed, 0, 0);
        assert_eq!(pcm, [0.0; 8]);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 0);
        shared.ready.store(true, Ordering::Release);
        shared.done.store(true, Ordering::Release);
        assert!(shared.ring.push([0.25, -0.5]));
        render_callback(&shared, &mut pcm, 2, &mut consumed, 10, 0);
        assert_eq!(pcm, [0.25, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(consumed, 1);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 0);
        render_callback(&shared, &mut pcm, 2, &mut consumed, 1_000_000, 20_000_000);
        assert_eq!(shared.device_frames_at(2_000_000, 48_000), 1);
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn absent_device_callbacks_report_error_at_the_timeout_boundary() {
        let shared = Arc::new(Shared::new());
        let output = output_without_device(shared.clone(), None);
        shared.last_callback_ns.store(100, Ordering::Release);
        shared.observed_frames.store(960, Ordering::Release);
        shared.check_callback_at(CALLBACK_TIMEOUT_NS + 99);
        assert!(output.error().is_none());
        shared.check_callback_at(CALLBACK_TIMEOUT_NS + 100);
        assert!(shared.stop.load(Ordering::Acquire));
        assert!(output.error().unwrap().contains("no device callback"));
        assert_eq!(output.frame(30), 0.6);
        // Callback silence/late recovery cannot restart a failed output.
        let mut consumed = 960;
        let mut pcm = [1.0f32; 2];
        render_callback(
            &shared,
            &mut pcm,
            2,
            &mut consumed,
            CALLBACK_TIMEOUT_NS + 101,
            0,
        );
        assert_eq!(pcm, [0.0, 0.0]);
        assert_eq!(shared.device_frames_at(u64::MAX, 48_000), 960);
    }

    #[test]
    fn callback_heartbeat_prevents_false_device_loss_during_underruns() {
        let shared = Shared::new();
        shared.begin_playback_at(0);
        shared.ready.store(true, Ordering::Release);
        let mut consumed = 0;
        let mut pcm = [1.0f32; 2];
        for now in [0, CALLBACK_TIMEOUT_NS - 1, CALLBACK_TIMEOUT_NS * 2 - 2] {
            render_callback(&shared, &mut pcm, 2, &mut consumed, now, 0);
            shared.check_callback_at(now + CALLBACK_TIMEOUT_NS - 1);
            assert!(!shared.stop.load(Ordering::Acquire));
        }
        assert_eq!(shared.underruns.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn device_error_keeps_first_failure_and_cancels_queued_audio() {
        let shared = Arc::new(Shared::new());
        let output = output_without_device(shared.clone(), None);
        shared.ready.store(true, Ordering::Release);
        assert!(shared.ring.push([0.25, -0.5]));
        shared.fail("audio output: device disconnected".into());
        shared.fail("audio mix: cancelled".into());
        let mut consumed = 0;
        let mut pcm = [1.0f32; 2];
        render_callback(&shared, &mut pcm, 2, &mut consumed, 1, 0);
        assert_eq!(pcm, [0.0, 0.0]);
        assert_eq!(consumed, 0);
        assert_eq!(output.underruns(), 0);
        assert_eq!(
            output.error().as_deref(),
            Some("audio output: device disconnected")
        );
    }

    #[test]
    fn preempted_clock_writer_has_bounded_read_and_preserves_last_position() {
        let shared = Shared::new();
        shared.observed_frames.store(1_234, Ordering::Release);
        shared.sequence.store(1, Ordering::Release);
        assert_eq!(shared.device_frames_at(1_000_000_000, 48_000), 1_234);
    }

    #[test]
    fn mixer_initialization_failure_is_reported_asynchronously() {
        let shared = Arc::new(Shared::new());
        let worker =
            spawn_producer(shared.clone(), || Err("PCM cache unavailable".into())).unwrap();
        worker.join().unwrap();
        let output = output_without_device(shared, None);
        assert!(output.error().unwrap().contains("PCM cache unavailable"));
        assert!(output.shared.stop.load(Ordering::Acquire));
    }

    #[test]
    fn mixer_initialization_and_mix_panics_stop_even_with_healthy_device_callbacks() {
        for before_panic in [0, MIX_FRAMES] {
            let shared = Arc::new(Shared::new());
            let mut produced = 0;
            let worker = spawn_producer(shared.clone(), move || {
                assert!(produced < before_panic, "injected mixer failure");
                produced += 1;
                Ok(Some([0.5, -0.5]))
            })
            .unwrap();
            worker.join().unwrap();
            let output = output_without_device(shared.clone(), None);
            let mut consumed = 0;
            let mut pcm = [1.0f32; 2];
            render_callback(&shared, &mut pcm, 2, &mut consumed, 1, 0);
            assert_eq!(pcm, [0.0, 0.0]);
            assert_eq!(consumed, 0);
            assert!(output.error().unwrap().contains("mixer worker panicked"));
            assert!(output.error().unwrap().contains("injected mixer failure"));
            assert!(shared.stop.load(Ordering::Acquire));
        }
    }

    #[test]
    fn callback_timeout_starts_with_play_request_and_unprimed_output_keeps_start_frame() {
        let shared = Arc::new(Shared::new());
        shared.check_callback_at(CALLBACK_TIMEOUT_NS * 4);
        assert!(
            !shared.stop.load(Ordering::Acquire),
            "prefill is not device loss"
        );
        let started_at = CALLBACK_TIMEOUT_NS * 4;
        shared.begin_playback_at(started_at);
        shared.check_callback_at(started_at + CALLBACK_TIMEOUT_NS - 1);
        assert!(!shared.stop.load(Ordering::Acquire));
        shared.check_callback_at(started_at + CALLBACK_TIMEOUT_NS);
        assert!(shared.stop.load(Ordering::Acquire));

        let shared = Arc::new(Shared::new());
        let mut output = output_without_device(shared.clone(), None);
        output.start_sample = 960;
        assert!(shared.ring.push([0.5, -0.5]));
        let mut consumed = 0;
        let mut pcm = [1.0f32; 2];
        render_callback(&shared, &mut pcm, 2, &mut consumed, 1, 0);
        assert_eq!(consumed, 0);
        assert_eq!(pcm, [0.0, 0.0]);
        assert_eq!(output.frame(30), 0.6);
    }

    #[test]
    fn long_tail_playback_latency_does_not_count_as_missing_callbacks() {
        let shared = Shared::new();
        shared.begin_playback_at(0);
        shared.ready.store(true, Ordering::Release);
        shared.done.store(true, Ordering::Release);
        assert!(shared.ring.push([0.5, -0.5]));
        let mut consumed = 0;
        let mut pcm = [1.0f32; 2];
        render_callback(
            &shared,
            &mut pcm,
            2,
            &mut consumed,
            0,
            CALLBACK_TIMEOUT_NS * 3,
        );
        for now in 1..=7 {
            let now = now * 1_000_000_000;
            render_callback(
                &shared,
                &mut pcm,
                2,
                &mut consumed,
                now,
                CALLBACK_TIMEOUT_NS * 3,
            );
            shared.check_callback_at(now);
            assert!(!shared.stop.load(Ordering::Acquire));
        }
        assert_eq!(shared.device_frames_at(7_000_000_000, 48_000), 1);
    }

    #[test]
    fn lost_device_releases_a_producer_waiting_on_a_full_queue() {
        let mut state = Shared::new();
        state.epoch = Instant::now() - Duration::from_secs(3);
        state.begin_playback_at(0);
        let shared = Arc::new(state);
        for _ in 0..QUEUE_FRAMES {
            assert!(shared.ring.push([0.0, 0.0]));
        }
        let worker = spawn_producer(shared.clone(), || Ok(Some([0.25, -0.5]))).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !worker.is_finished() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(
            worker.is_finished(),
            "device loss must release the full-queue wait"
        );
        worker.join().unwrap();
        assert!(shared.stop.load(Ordering::Acquire));
        assert!(shared
            .error
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("no device callback"));
    }

    #[test]
    fn stop_does_not_wait_for_background_disk_io_and_cancels_refill() {
        let shared = Arc::new(Shared::new());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        struct OnDrop(mpsc::Sender<()>);
        impl Drop for OnDrop {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let finished = OnDrop(finished_tx);
        let worker = spawn_producer(shared.clone(), move || {
            let _keep_until_worker_exit = &finished;
            entered_tx.send(()).unwrap();
            // Bound the fake disk IO so a regression fails instead of hanging.
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
            Ok(Some([0.5, -0.5]))
        })
        .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let mut output = output_without_device(shared.clone(), Some(worker));
        let began = Instant::now();
        output.stop();
        assert!(
            began.elapsed() < Duration::from_secs(1),
            "stop waited for disk IO"
        );
        release_tx.send(()).unwrap();
        finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            shared.ring.pop().is_none(),
            "cancelled mixer refilled the queue"
        );
        assert!(output.producer.is_none());
    }

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


#[cfg(test)]
mod failure_log_tests {
    use super::*;

    #[test]
    fn watchdog_logs_the_first_device_failure_once_without_callback_io_or_normal_stop_errors() {
        const CHILD: &str = "MOTION_AUDIO_FAILURE_LOG_TEST_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let shared=Arc::new(Shared::new());
            shared.fail("audio output: controlled device disconnection".into());
            shared.fail("audio mix: secondary cancellation".into());
            // Publishing failure is suitable for the device callback: no log IO.
            assert!(!std::fs::read_to_string(root.join("motion-studio.log")).unwrap().contains("controlled device disconnection"));
            let monitor=spawn_monitor(shared).unwrap();
            wait_finished(monitor);
            let clean=Arc::new(Shared::new());
            clean.stop.store(true,Ordering::Release);
            wait_finished(spawn_monitor(clean).unwrap());
            return;
        }
        let root=tempfile::tempdir().unwrap();
        let output=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","audio_output::failure_log_tests::watchdog_logs_the_first_device_failure_once_without_callback_io_or_normal_stop_errors"])
            .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let text=std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert_eq!(text.lines().filter(|line| line.contains(" ERROR ") && line.contains("[motion-audio] output stopped:")).count(),1);
        assert!(text.contains("controlled device disconnection"));
        assert!(!text.contains("secondary cancellation"));
        assert_eq!(String::from_utf8_lossy(&output.stderr).matches("[motion-audio] output stopped:").count(),1);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("controlled device disconnection"));
    }

    fn wait_finished(worker: JoinHandle<()>) {
        let deadline=Instant::now()+Duration::from_secs(5);
        while !worker.is_finished() {
            assert!(Instant::now()<deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod callback_observation_tests {
    use super::*;
    #[test]
    fn invalid_timestamps_are_deferred_until_watchdog_and_reset_after_valid_callback() {
        const CHILD: &str = "MOTION_CALLBACK_TIMESTAMP_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let shared = Shared::new();
            for _ in 0..120 { assert_eq!(shared.callback_latency(None),0); }
            let path = root.join("motion-studio.log");
            assert!(std::fs::read_to_string(&path).unwrap().is_empty());
            shared.drain_callback_evidence(); shared.drain_callback_evidence();
            assert_eq!(shared.callback_latency(Some(Duration::from_millis(3))),3_000_000);
            for _ in 0..60 { shared.callback_latency(None); }
            shared.drain_callback_evidence();
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { let _guard = shared.error.lock().unwrap(); panic!("controlled audio poison"); })).is_err());
            for _ in 0..120 { shared.fail("keep first failure".into()); }
            shared.drain_callback_evidence();
            assert_eq!(shared.error.lock().unwrap_err().into_inner().as_deref(),Some("keep first failure"));
            let text = std::fs::read_to_string(path).unwrap();
            assert_eq!(text.lines().filter(|l| l.contains("site=audio.callback.timestamp_order ")).count(),4);
            assert_eq!(text.lines().filter(|l| l.contains("site=audio.error.publish ")).count(),2);
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "audio_output::callback_observation_tests::invalid_timestamps_are_deferred_until_watchdog_and_reset_after_valid_callback"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(output.status.success(), "{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("persistent audio degradation"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("audio degradation"));
    }
}
