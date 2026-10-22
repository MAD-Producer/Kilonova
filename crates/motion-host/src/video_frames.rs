//! Exact PTS lookup with bounded forward prefetch, independent of render ticks.
use crate::platform::VideoDecoder;
use crate::video_cache::{FrameCache, LOOKAHEAD};
use crate::video_frame::{DecodedFrame, PreparedVideoFrame};
use motion_core::{Content, Project, VideoAsset};
use motion_render::Scene;
use motion_media::{Error as MediaError, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::Instant,
};
const MAX_DECODER_WORKERS: usize = 4;
static PREFETCH_FAILURES: motion_render::observability::FailureCounter =
    motion_render::observability::FailureCounter::new();
#[derive(Clone, Copy)]
struct Request {
    time: u64,
    next: Option<u64>,
    source_time: u64,
    index: usize,
    sequence: u64,
}
#[derive(Clone, Copy)]
struct DecodeFlight {
    time: u64,
    sequence: u64,
    speculative: bool,
    phase: &'static str,
    began: Instant,
}
impl DecodeFlight {
    fn status(self) -> Value {
        json!({"pts_us":self.time,"sequence":self.sequence,"speculative":self.speculative,
            "phase":self.phase,"elapsed_ms":self.began.elapsed().as_millis().min(u128::from(u64::MAX)) as u64})
    }
}
struct State {
    desired: Option<Request>,
    cache: FrameCache,
    error: Option<MediaError>,
    stop: bool,
    prefetch: bool,
    failed_prefetch: Option<u64>,
    prefetch_failures: u64,
    last_prefetch_error: Option<String>,
    hits: u64,
    misses: u64,
    decoded: u64,
    cancelled: u64,
    seeks: u64,
    in_flight: Option<DecodeFlight>,
    pending_polls: u64,
    worker_stopped: bool,
}
impl State {
    fn prefetch_failed(&mut self, source: u64, time: u64, reason: &str) {
        self.failed_prefetch = Some(time);
        self.prefetch_failures = self.prefetch_failures.saturating_add(1);
        self.last_prefetch_error = Some(reason.chars().take(2048).collect());
        PREFETCH_FAILURES.failure("video_prefetch", format_args!("source={source} pts_us={time} reason={reason}"));
    }
}
struct Stream {
    object: u64,
    asset: VideoAsset,
    pts: Arc<Vec<u64>>,
    shared: Arc<(Mutex<State>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}
fn job(s: &State, pts: &[u64]) -> Option<(u64, bool)> {
    let r = s.desired?;
    if s.cache.find(r.time).is_none() {
        return s.error.is_none().then_some((r.time, false));
    }
    if let Some(next) = r.next.filter(|next| s.cache.find(*next).is_none()) {
        return s.error.is_none().then_some((next, false));
    }
    if !s.prefetch || s.failed_prefetch.is_some() {
        return None;
    }
    let expected = s.cache.find(r.time)?.bytes();
    if !s.cache.can_prefetch(expected) {
        return None;
    }
    pts.iter()
        .skip(r.index + 1)
        .take(LOOKAHEAD)
        .find(|t| s.failed_prefetch != Some(**t) && s.cache.find(**t).is_none())
        .map(|t| (*t, true))
}
impl Stream {
    fn new(
        root: PathBuf,
        asset: VideoAsset,
        platform: Arc<dyn crate::platform::Platform>,
        object: u64,
    ) -> Result<Self> {
        let pts = Arc::new(motion_media::load_video_index(&root, &asset)?);
        let shared = Arc::new((
            Mutex::new(State {
                desired: None,
                cache: FrameCache::for_size(asset.display_width, asset.display_height),
                error: None,
                stop: false,
                prefetch: false,
                failed_prefetch: None,
                prefetch_failures: 0,
                last_prefetch_error: None,
                hits: 0,
                misses: 0,
                decoded: 0,
                cancelled: 0,
                seeks: 0,
                in_flight: None,
                pending_polls: 0,
                worker_stopped: false,
            }),
            Condvar::new(),
        ));
        let copy = shared.clone();
        let times = pts.clone();
        let media = asset.clone();
        let backend = platform.clone();
        let worker = std::thread::Builder::new()
            .name("motion-video-frames".into())
            .spawn(move || {
                let mut observation = worker_observation::WorkerObservation::new(object, media.id, copy.clone());
                log::info!("video decoder worker started object={object} source={} path={}", media.id, media.path);
                log::logger().flush();
                let mut decoder: Option<Box<dyn VideoDecoder>> = None;
                let mut announced_open = false;
                loop {
                    let flight = {
                        let (lock, cv) = &*copy;
                        let s = lock.lock().unwrap_or_else(|e| e.into_inner());
                        let mut s = cv
                            .wait_while(s, |s| !s.stop && job(s, &times).is_none())
                            .unwrap_or_else(|e| e.into_inner());
                        if s.stop {
                            drop(s);
                            observation.stopped("stop");
                            return;
                        }
                        let (time, speculative) = job(&s, &times).unwrap();
                        let flight = DecodeFlight { time, speculative,
                            sequence:s.desired.unwrap().sequence,
                            phase:if decoder.is_none() {"open"} else {"decode"}, began:Instant::now() };
                        s.in_flight = Some(flight);
                        flight
                    };
                    let target = flight.time;
                    let speculative = flight.speculative;
                    let check = || {
                        let s = copy.0.lock().map_err(|_| "video frame state poisoned")?;
                        let valid = s.desired.is_some_and(|r| {
                            target == r.time
                                || Some(target) == r.next
                                || (s.prefetch
                                    && target > r.time
                                    && target <= times[(r.index + LOOKAHEAD).min(times.len() - 1)])
                        });
                        if s.stop || !valid {
                            Err("video target superseded".into())
                        } else {
                            Ok(())
                        }
                    };
                    let previous_seeks = decoder.as_ref().map_or(0u64, |d| d.seeks());
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                        || -> Result<Arc<DecodedFrame>> {
                            check()?;
                            if decoder.is_none() {
                                if !announced_open {
                                    log::info!("video decoder open start object={object} source={} sequence={} pts_us={target} path={}", media.id, flight.sequence, media.path);
                                    log::logger().flush();
                                    announced_open = true;
                                }
                                let base = root.canonicalize().map_err(|e| e.to_string())?;
                                let source = root
                                    .join(&media.path)
                                    .canonicalize()
                                    .map_err(|e| e.to_string())?;
                                if !source.starts_with(base)
                                    || source.metadata().map_err(|e| e.to_string())?.len()
                                        != media.bytes
                                {
                                    return Err("owned video source missing or changed".into());
                                }
                                decoder = Some(backend.open_decoder(
                                    &source,
                                    media.clone(),
                                    times.as_ref().clone(),
                                )?);
                            }
                            if let Some(active) = &mut copy.0.lock().unwrap_or_else(|e|e.into_inner()).in_flight {
                                active.phase = "decode";
                            }
                            Ok(Arc::new(decoder.as_mut().unwrap().frame(target, &check)?))
                        },
                    ))
                    .unwrap_or_else(|payload| Err(MediaError::Panic {
                        operation: "video decoder worker", reason: motion_media::panic_reason(payload),
                    }));
                    let mut s = copy.0.lock().unwrap_or_else(|e| e.into_inner());
                    s.seeks += decoder
                        .as_ref()
                        .map_or(0u64, |d| d.seeks())
                        .saturating_sub(previous_seeks);
                    if s.stop {
                        s.in_flight = None;
                        drop(s);
                        if let Err(error) = &result {
                            if error.to_string() != "video target superseded" {
                                log::error!("video decode discarded failure object={object} source={} sequence={} pts_us={target} {}ms stopped=true reason={error}", media.id, flight.sequence, flight.began.elapsed().as_millis());
                            }
                        }
                        observation.stopped("stop_after_decode");
                        return;
                    }
                    let desired = s.desired.unwrap();
                    let required = target == desired.time || Some(target) == desired.next;
                    let last = times[(desired.index + LOOKAHEAD).min(times.len() - 1)];
                    let phase = s.in_flight.map_or(flight.phase, |active| active.phase);
                    let mut failure = result.as_ref().err()
                        .filter(|error| error.to_string() != "video target superseded" && (required || !speculative))
                        .map(|error| (error.clone(), phase));
                    let frame_bytes = result.as_ref().ok().map(|frame| frame.bytes());
                    let cache_budget = s.cache.budget();
                    match result {
                        Ok(frame) => {
                            s.decoded += 1;
                            if frame.pts >= desired.time && frame.pts <= last {
                                let pinned = [desired.time, desired.next.unwrap_or(desired.time)];
                                if !s.cache.insert_required(frame, &pinned) {
                                    if required {
                                        let reason = "video source frame exceeds its stream cache budget";
                                        s.error = Some(reason.into());
                                        failure = Some((reason.into(), "cache"));
                                    } else {
                                        s.prefetch_failed(media.id, target, "video source frame exceeds its stream cache budget");
                                    }
                                } else if speculative {
                                    PREFETCH_FAILURES.success();
                                }
                            } else {
                                s.cancelled += 1;
                            }
                        }
                        Err(e) if e.to_string() == "video target superseded" => {
                            s.cancelled += 1;
                        }
                        Err(e) => {
                            decoder = None;
                            if required {
                                s.error = Some(e);
                            } else if speculative {
                                s.prefetch_failed(media.id, target, &e.to_string());
                            }
                        }
                    }
                    s.in_flight = None;
                    drop(s);
                    if let Some((error, phase)) = failure {
                        log::error!("video decode failed object={object} source={} sequence={} pts_us={target} {}ms phase={phase} speculative={speculative} requested_sequence={} requested_pts_us={} discarded={} frame_bytes={frame_bytes:?} cache_budget_bytes={cache_budget} reason={error}", media.id, flight.sequence, flight.began.elapsed().as_millis(), desired.sequence, desired.time, !required);
                    }
                    if flight.began.elapsed().as_millis() >= 100 {
                        log::debug!("video decode done object={object} source={} sequence={} pts_us={target} {}ms speculative={speculative}", media.id, flight.sequence, flight.began.elapsed().as_millis());
                    }
                    // Do not await another VSync: fill the next available cache slot.
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { object, asset, pts, shared, worker: Some(worker) })
    }
    fn key(&self, time: u64) -> Result<(u64, usize)> {
        if time < self.asset.video_start_us || time >= self.asset.video_end_us {
            return Err("source time outside visible video".into());
        }
        let index = self
            .pts
            .partition_point(|p| *p <= time)
            .checked_sub(1)
            .ok_or("video PTS index missing")?;
        Ok((self.pts[index], index))
    }
    #[cfg(test)]
    fn request(&self, time: u64, sequence: u64, prefetch: bool) -> Result<Value> {
        self.request_blend(time, sequence, prefetch, false)
    }
    fn request_blend(&self, time: u64, sequence: u64, prefetch: bool, blend: bool) -> Result<Value> {
        if sequence == 0 || sequence > i64::MAX as u64 {
            return Err("video sequence must be a positive signed 64-bit value".into());
        }
        let source_time = time;
        let (time, index) = self.key(time)?;
        let next = blend.then(|| self.pts.get(index + 1).copied()).flatten()
            .filter(|next| source_time > time && *next < self.asset.video_end_us);
        let mut s = self
            .shared
            .0
            .lock()
            .map_err(|_| "video frame state poisoned")?;
        if let Some(old) = s.desired {
            if sequence < old.sequence {
                return Err("stale video request sequence".into());
            }
            if sequence == old.sequence && time != old.time {
                return Err("video sequence already used for another target".into());
            }
        }
        if s.desired.is_none_or(|r| r.sequence != sequence) {
            s.desired = Some(Request {
                time,
                next,
                source_time,
                index,
                sequence,
            });
            s.error = None;
            s.failed_prefetch = None;
            s.cache
                .retain_window(time, self.pts[(index + LOOKAHEAD).min(self.pts.len() - 1)]);
            if s.cache.find(time).is_some() {
                s.hits += 1;
            } else {
                s.misses += 1;
            }
        }
        if let Some(r) = &mut s.desired {
            r.source_time = source_time;
            if r.next != next {
                r.next = next;
                s.error = None;
                s.failed_prefetch = None;
            }
        }
        s.cache.require_pair(self.asset.display_width, self.asset.display_height, next.is_some(),
            &[time, next.unwrap_or(time)]);
        s.prefetch = prefetch;
        self.shared.1.notify_one();
        s.pending_polls = if s.error.is_none() && (s.cache.find(time).is_none()
            || next.is_some_and(|next| s.cache.find(next).is_none())) {
            s.pending_polls.saturating_add(1)
        } else { 0 };
        let count = s.pending_polls;
        let active = s.in_flight;
        let result = status(&s);
        drop(s);
        // No per-frame IO. Preserve only the first observation and escalation;
        // do not hold the state mutex while a host's file/console sink runs.
        if count == 1 || count == 60 {
            let phase = active.map_or("queued", |f| f.phase);
            let elapsed = active.map_or(0, |f| f.began.elapsed().as_millis());
            let active_sequence = active.map(|f|f.sequence);
            let active_pts = active.map(|f|f.time);
            if count == 1 {
                log::debug!("video pending object={} source={} sequence={sequence} pts_us={time} count={count} phase={phase}", self.object, self.asset.id);
            } else {
                log::error!("persistent video pending object={} source={} sequence={sequence} pts_us={time} count={count} phase={phase} active_sequence={active_sequence:?} active_pts_us={active_pts:?} decode_elapsed_ms={elapsed}", self.object, self.asset.id);
            }
        }
        Ok(result)
    }
    fn ready(&self, sequence: u64) -> Result<Arc<DecodedFrame>> {
        Ok(self.prepared(sequence)?.left)
    }
    fn prepared(&self, sequence: u64) -> Result<PreparedVideoFrame> {
        let s = self
            .shared
            .0
            .lock()
            .map_err(|_| "video frame state poisoned")?;
        let r = s
            .desired
            .filter(|r| r.sequence == sequence)
            .ok_or("video frame superseded")?;
        if let Some(e) = &s.error {
            return Err(e.clone());
        }
        let left = s.cache.find(r.time).ok_or("video frame pending")?;
        let next = r.next.map(|next| {
            let frame = s.cache.find(next).ok_or("adjacent video frame pending")?;
            let weight = (r.source_time - r.time) as f64 / (next - r.time) as f64;
            Ok::<_, String>((frame, weight as f32))
        }).transpose()?;
        Ok(PreparedVideoFrame { left, next })
    }
    fn stop(&self) -> Result<()> {
        let mut poisoned = false;
        let mut s = self.shared.0.lock().unwrap_or_else(|e| { poisoned = true; e.into_inner() });
        s.stop = true;
        s.cache = FrameCache::new();
        self.shared.1.notify_one();
        if poisoned { Err("video frame state poisoned while stopping decoder".into()) } else { Ok(()) }
    }
    fn retire(mut self) -> (Option<JoinHandle<()>>, Result<()>) {
        let stopped = self.stop();
        (self.worker.take(), stopped)
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        if let Err(error) = self.stop() { log::error!("video stream cleanup: {error}"); }
    }
}
fn status(s: &State) -> Value {
    let r = s.desired.unwrap();
    let frame = s.cache.find(r.time);
    let state = if s.error.is_some() {
        "failed"
    } else if frame.is_some() && r.next.is_none_or(|next| s.cache.find(next).is_some()) {
        "ready"
    } else {
        "pending"
    };
    json!({"state":state,"sequence":r.sequence,"source_time_us":r.source_time,
        "next_pts_us":r.next,"blend_weight":r.next.map(|next| (r.source_time - r.time) as f64 / (next - r.time) as f64),
        "pts_us":frame.as_ref().map(|f|f.pts),"end_us":frame.as_ref().map(|f|f.end),
        "decode_us":frame.as_ref().map(|f|f.decode_us),"codec_us":frame.as_ref().map(|f|f.codec_us),
        "transfer_us":frame.as_ref().map(|f|f.transfer_us),"pack_us":frame.as_ref().map(|f|f.pack_us),
        "cache_bytes":s.cache.bytes(),"cache_frames":s.cache.len(),"cache_hits":s.hits,"cache_misses":s.misses,"error":s.error.as_ref().map(ToString::to_string),
        "prefetch_failures":s.prefetch_failures,"prefetch_blocked_pts_us":s.failed_prefetch,"last_prefetch_error":s.last_prefetch_error,
        "in_flight":s.in_flight.map(DecodeFlight::status),"pending_polls":s.pending_polls,"worker_stopped":s.worker_stopped})
}
#[derive(Default)]
pub struct VideoFrames {
    streams: HashMap<u64, Stream>,
    // Public sequences never share a namespace with automatic preview generations.
    sequences: HashMap<u64, (u64, f64, u64)>,
    counter: u64,
    platform: Option<Arc<dyn crate::platform::Platform>>,
    retiring: Vec<JoinHandle<()>>,
    retirement_error: Option<String>,
    pending_retirement_error: Option<String>,
    // Exposure visits one local composition at a time. Keep at most the same
    // four workers, oldest idle first, instead of reopening at every parent.
    sampling_recent: Vec<u64>,
}
impl VideoFrames {
    pub fn new(platform: Arc<dyn crate::platform::Platform>) -> Self {
        Self {
            platform: Some(platform),
            ..Default::default()
        }
    }
    fn backend(&self) -> Result<Arc<dyn crate::platform::Platform>> {
        self.platform
            .clone()
            .ok_or_else(|| "video frames require a host platform".into())
    }
    pub fn clear(&mut self) {
        for (_, stream) in self.streams.drain() {
            let (worker, stopped) = stream.retire();
            if let Some(worker) = worker { self.retiring.push(worker); }
            if let Err(error) = stopped {
                self.retirement_error = Some(error.to_string());
                self.pending_retirement_error = Some(error.to_string());
            }
        }
        self.sequences.clear();
        self.sampling_recent.clear();
    }
    /// Stop and join every owned decoder. Only call on an independent export
    /// worker: native decoder closure can block. Retain that worker's slot and
    /// frozen sources until this returns instead of detaching cancelled work.
    pub fn finish(&mut self) -> Result<()> {
        self.clear();
        let mut error = self.pending_retirement_error.take();
        for worker in self.retiring.drain(..) {
            if let Err(payload) = worker.join() {
                let reason: String = format!("video decoder retirement failed: {}", motion_media::panic_reason(payload))
                    .chars().take(1024).collect();
                if let Some(primary) = &mut error {
                    if primary.len() < 2048 { primary.push_str(&format!("; cleanup: {reason}")); }
                } else { error = Some(reason); }
            }
        }
        if let Some(error) = error {
            self.retirement_error = Some(error.clone());
            Err(error.into())
        } else { Ok(()) }
    }
    /// Supersede unfinished exposure reads without discarding completed caches
    /// or restarting failed decoders. Retiring native calls retain their worker
    /// slots until they actually return, just as ordinary scene eviction does.
    pub(crate) fn cancel_pending_sampling(&mut self) -> Result<()> {
        let mut pending = Vec::new();
        for object in &self.sampling_recent {
            let Some(stream) = self.streams.get(object) else { continue; };
            let state = stream.shared.0.lock().map_err(|_| "video frame state poisoned while canceling exposure")?;
            if state.error.is_none() && state.desired.is_some_and(|request| {
                state.in_flight.is_some() || state.cache.find(request.time).is_none()
                    || request.next.is_some_and(|pts| state.cache.find(pts).is_none())
            }) {
                pending.push(*object);
            }
        }
        for object in pending { self.retire(object); }
        self.reap_retired()
    }
    fn retire(&mut self, object: u64) {
        self.sampling_recent.retain(|id| *id != object);
        if let Some(stream) = self.streams.remove(&object) {
            let (worker, stopped) = stream.retire();
            if let Some(worker) = worker { self.retiring.push(worker); }
            if let Err(error) = stopped {
                self.retirement_error = Some(error.to_string());
                self.pending_retirement_error = Some(error.to_string());
            }
        }
    }
    /// Join only completed workers. A slow native decoder never blocks this
    /// caller and still consumes its slot until it actually releases resources.
    fn reap_retired(&mut self) -> Result<()> {
        if let Some(error) = self.pending_retirement_error.take() { return Err(error.into()); }
        let mut index = 0;
        while index < self.retiring.len() {
            if self.retiring[index].is_finished() {
                if let Err(payload) = self.retiring.swap_remove(index).join() {
                    let error = format!("video decoder retirement failed: {}", motion_media::panic_reason(payload));
                    log::error!("{error}");
                    self.retirement_error = Some(error.clone());
                    return Err(error.into());
                }
            } else { index += 1; }
        }
        Ok(())
    }
    pub fn request(
        &mut self,
        project: &Project,
        root: &Path,
        object: u64,
        frame: f64,
        sequence: u64,
    ) -> Result<Value> {
        if !frame.is_finite() || frame < 0.0 || frame >= f64::from(project.frames) {
            return Err("video composition time outside project".into());
        }
        let layer = project
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or("video object missing")?;
        let Content::Video { video } = &layer.content else {
            return Err("object is not video".into());
        };
        if sequence == 0 || sequence > i64::MAX as u64 {
            return Err("invalid video sequence".into());
        }
        if let Some((old, time, _)) = self.sequences.get(&object) {
            if sequence < *old {
                return Err("stale video request sequence".into());
            }
            if sequence == *old && frame != *time {
                return Err("video sequence reused for another composition time".into());
            }
        }
        self.sequences
            .retain(|id, _| project.layers.iter().any(|l| l.id == *id));
        let asset = project
            .video_assets
            .iter()
            .find(|a| a.id == video.asset)
            .ok_or("video asset missing")?;
        let source = video.source_time_us(layer.local_frame(frame), project.fps);
        if !layer.active(frame, project.frames)
            || source < asset.video_start_us as i64
            || source >= asset.video_end_us as i64
        {
            self.sequences.insert(object, (sequence, frame, 0));
            self.retire(object);
            return Ok(
                json!({"state":"outside","sequence":sequence,"source_time_us":source,"bytes":0,"object":object}),
            );
        }
        let generation = self.generation(asset, object, source as u64)?;
        let mut result =
            self.request_source(root, asset, object, source as u64, generation, false, false)?;
        self.sequences.insert(object, (sequence, frame, generation));
        result["sequence"] = json!(sequence);
        Ok(result)
    }
    fn generation(&mut self, asset: &VideoAsset, object: u64, time: u64) -> Result<u64> {
        // A layer can change sources while retaining its object id. Retire that
        // stream before interpreting its old timebase or poisoned state.
        if self.streams.get(&object).is_some_and(|stream| stream.asset != *asset) {
            self.retire(object);
        }
        if let Some(stream) = self.streams.get(&object) {
            let desired = stream.shared.0.lock()
                .map_err(|_| format!("video frame state poisoned: object={object}"))?.desired;
            if let Some(request) = desired {
                let (target, _) = stream.key(time)?;
                if request.time == target { return Ok(request.sequence); }
            }
        }
        let next = self.counter.checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or("video generation exhausted")?;
        self.counter = next;
        Ok(next)
    }
    fn request_source(
        &mut self,
        root: &Path,
        asset: &VideoAsset,
        object: u64,
        source: u64,
        generation: u64,
        prefetch: bool,
        blend: bool,
    ) -> Result<Value> {
        self.reap_retired()?;
        if self.streams.get(&object).is_some_and(|s| s.asset != *asset) {
            self.retire(object);
        }
        if !self.streams.contains_key(&object) {
            if self.streams.len() >= MAX_DECODER_WORKERS {
                return Err("at most four video instances per reader; release idle frames".into());
            }
            if self.streams.len() + self.retiring.len() >= MAX_DECODER_WORKERS {
                // A pending state is explicit, retryable scheduling backpressure.
                return Ok(json!({"state":"pending","reason":"decoder_retirement", "sequence":generation,
                    "object":object,"width":asset.display_width,"height":asset.display_height}));
            }
            self.streams.insert(
                object,
                Stream::new(root.into(), asset.clone(), self.backend()?, object)?,
            );
        }
        let mut result = self.streams[&object].request_blend(source, generation, prefetch, blend)?;
        result["object"] = json!(object);
        result["width"] = json!(asset.display_width);
        result["height"] = json!(asset.display_height);
        Ok(result)
    }
    pub fn frame(
        &self,
        project: &Project,
        object: u64,
        sequence: u64,
    ) -> motion_media::Result<Arc<DecodedFrame>> {
        let (current, time, generation) =
            self.sequences.get(&object).ok_or("video request missing")?;
        if *current != sequence {
            return Err("video request superseded".into());
        }
        let layer = project
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or("video object removed")?;
        let Content::Video { video } = &layer.content else {
            return Err("object is no longer video".into());
        };
        let stream = self.streams.get(&object).ok_or("video reader missing")?;
        let desired = stream
            .shared
            .0
            .lock()
            .map_err(|_| "video frame state poisoned")?
            .desired
            .ok_or("video target missing")?;
        if !layer.active(*time, project.frames)
            || stream.asset.id != video.asset
            || stream
                .key(video.source_time_us(layer.local_frame(*time), project.fps) as u64)?
                .0
                != desired.time
        {
            return Err("video clip changed; request its current source time".into());
        }
        stream.ready(*generation)
    }
    pub fn prepare_scene(
        &mut self,
        project: &Project,
        root: &Path,
        scene: &Scene,
        _frame: f64,
    ) -> Result<Option<Vec<(u64, PreparedVideoFrame)>>> {
        self.prepare(project, root, scene, true, false)
    }
    pub fn prepare_scene_exact(
        &mut self,
        project: &Project,
        root: &Path,
        scene: &Scene,
    ) -> Result<Option<Vec<(u64, PreparedVideoFrame)>>> {
        self.prepare(project, root, scene, false, false)
    }
    /// Exact local-composition sampling. Inactive cached decoders survive a
    /// visit to another scope; eviction still respects the existing four-worker
    /// limit including retiring workers. This does not restart failed streams.
    pub fn prepare_scene_sampling_exact(&mut self, project:&Project, root:&Path,
        scene:&Scene) -> Result<Option<Vec<(u64,PreparedVideoFrame)>>> {
        self.prepare(project,root,scene,false,true)
    }
    fn prepare(
        &mut self,
        project: &Project,
        root: &Path,
        scene: &Scene,
        prefetch: bool,
        retain_idle: bool,
    ) -> Result<Option<Vec<(u64, PreparedVideoFrame)>>> {
        let layers=scene.video_layers();
        let inactive: Vec<_> = self.streams.keys().filter(|object| !layers.iter().any(|l| l.id == **object)).copied().collect();
        if retain_idle {
            let missing=layers.iter().filter(|layer| !self.streams.contains_key(&layer.id)).count();
            let allowed=MAX_DECODER_WORKERS.saturating_sub(missing);
            let mut inactive=inactive;
            inactive.sort_unstable_by_key(|object|self.sampling_recent.iter().position(|id|id==object));
            for object in inactive {
                if self.streams.len()<=allowed {break;}
                self.retire(object);
            }
        } else {
            for object in inactive { self.retire(object); }
        }
        self.reap_retired()?;
        let mut frames = Vec::new();
        let mut pending = false;
        for layer in layers {
            if layer.video.is_none() {
                continue;
            }
            let source = layer.video.as_ref().unwrap();
            let asset = project
                .video_assets
                .iter()
                .find(|a| a.id == source.asset)
                .ok_or("video asset missing")?;
            let generation = self.generation(asset, layer.id, source.source_time_us)?;
            let r = self.request_source(
                root,
                asset,
                layer.id,
                source.source_time_us,
                generation,
                prefetch,
                layer.sampling.frame_blend == motion_core::render_sampling::FrameBlend::Linear,
            )?;
            if retain_idle {
                self.sampling_recent.retain(|id|*id!=layer.id && self.streams.contains_key(id));
                if self.streams.contains_key(&layer.id) && self.sampling_recent.len()<MAX_DECODER_WORKERS {
                    self.sampling_recent.push(layer.id);
                }
            }
            match r["state"].as_str() {
                Some("ready") => {
                    frames.push((layer.id, self.streams[&layer.id].prepared(generation)?))
                }
                Some("failed") => {
                    return Err(r["error"].as_str().unwrap_or("video decode failed").into())
                }
                _ => pending = true,
            }
        }
        if pending {
            Ok(None)
        } else {
            Ok(Some(frames))
        }
    }
    pub fn cache_bytes(&self) -> Result<usize> {
        self.streams.values().try_fold(0usize, |total, stream| {
            let state = stream.shared.0.lock().map_err(|_| "video frame state poisoned while measuring cache")?;
            total.checked_add(state.cache.bytes()).ok_or_else(|| "video frame cache accounting overflow".into())
        })
    }
    pub fn metrics(&self) -> Value {
        let mut bytes = 0;
        let mut budget = 0;
        let mut frames = 0;
        let mut hits = 0;
        let mut misses = 0;
        let mut decoded = 0;
        let mut cancelled = 0;
        let mut seeks = 0;
        let mut prefetch_failures = 0u64;
        let mut in_flight = Vec::new();
        let mut stopped_workers = 0;
        for stream in self.streams.values() {
            let s = stream.shared.0.lock().unwrap_or_else(|e| e.into_inner());
            bytes += s.cache.bytes();
            budget += s.cache.budget();
            frames += s.cache.len();
            hits += s.hits;
            misses += s.misses;
            decoded += s.decoded;
            cancelled += s.cancelled;
            seeks += s.seeks;
            prefetch_failures = prefetch_failures.saturating_add(s.prefetch_failures);
            stopped_workers += usize::from(s.worker_stopped);
            if let Some(active) = s.in_flight {
                let mut status = active.status();
                status["object"] = json!(stream.object);
                status["source"] = json!(stream.asset.id);
                in_flight.push(status);
            }
        }
        json!({"cacheBytes":bytes,"cacheFrames":frames,"cacheBudgetBytes":budget,
            "maxCacheBudgetBytes":MAX_DECODER_WORKERS as u64 * motion_core::MAX_VIDEO_PIXELS * 8,
            "holdMaxCacheBudgetBytes":MAX_DECODER_WORKERS * crate::video_cache::MAX_CACHE_BYTES,
            "cacheHits":hits,"cacheMisses":misses,"decodedFrames":decoded,"cancelledFrames":cancelled,"decoderSeeks":seeks,"prefetchFailures":prefetch_failures,"lookaheadFrames":LOOKAHEAD,"streams":self.streams.len(),"decodeInFlight":in_flight,"decodeWorkersStopped":stopped_workers,
            "retiringStreams":self.retiring.iter().filter(|worker| !worker.is_finished()).count(),
            "maxDecoderWorkers":MAX_DECODER_WORKERS,"retirementError":self.retirement_error})
    }
}

#[cfg(test)]
mod generation_failure_tests {
    use super::*;

    fn asset() -> VideoAsset {
        serde_json::from_value(json!({
            "id":1,"path":"assets/test.mp4","bytes":4,"mime":"video/avc","track":0,
            "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
            "video_start_us":0,"video_end_us":66666,"duration_us":66666,
            "frame_count":2,"variable_frame_rate":false,"nominal_frame_rate":30.0,
            "color_standard":1,"color_range":1,"audio_asset":null
        })).unwrap()
    }

    fn frames(asset: &VideoAsset) -> VideoFrames {
        let mut frames = VideoFrames {counter:10,..Default::default()};
        let stream = Stream {
            worker: None,
            object:7,
            asset:asset.clone(),
            pts:Arc::new(vec![0,33333]),
            shared:Arc::new((Mutex::new(State {
                desired:Some(Request {time:0,next:None,source_time:0,index:0,sequence:10}),
                cache:FrameCache::new(),error:None,stop:false,prefetch:false,failed_prefetch:None,prefetch_failures:0,last_prefetch_error:None,
                hits:0,misses:0,decoded:0,cancelled:0,seeks:0,
                in_flight:None,pending_polls:0,worker_stopped:false,
            }),Condvar::new())),
        };
        frames.streams.insert(7,stream);
        frames
    }

    #[test]
    fn poisoned_video_state_is_an_error_without_fabricating_a_new_generation() {
        let asset = asset();
        let mut frames = frames(&asset);
        let shared = frames.streams[&7].shared.clone();
        assert!(std::panic::catch_unwind(|| {
            let _guard = shared.0.lock().unwrap();
            panic!("injected frame-state poison");
        }).is_err());
        let error = frames.generation(&asset,7,0).unwrap_err();
        assert!(error.to_string().contains("state poisoned") && error.to_string().contains("object=7"));
        assert_eq!(frames.counter,10);
    }

    #[test]
    fn same_source_reuses_frame_generation_but_bad_timebase_does_not_advance_it() {
        let asset = asset();
        let mut frames = frames(&asset);
        assert_eq!(frames.generation(&asset,7,1000).unwrap(),10);
        assert!(frames.generation(&asset,7,90000).unwrap_err().to_string().contains("outside visible video"));
        assert_eq!(frames.counter,10);
        frames.streams.get_mut(&7).unwrap().pts=Arc::new(vec![]);
        assert!(frames.generation(&asset,7,0).unwrap_err().to_string().contains("PTS index missing"));
        assert_eq!(frames.counter,10);
    }

    #[test]
    fn changed_source_retires_the_old_timebase_before_allocating_a_generation() {
        let original = asset();
        let mut frames = frames(&original);
        let mut next = original.clone();
        next.id=2;
        next.video_start_us=100000;
        next.video_end_us=200000;
        assert_eq!(frames.generation(&next,7,100000).unwrap(),11);
        assert!(!frames.streams.contains_key(&7));
    }

    #[test]
    fn generation_exhaustion_respects_the_decoders_signed_sequence_contract() {
        let mut frames=VideoFrames {counter:i64::MAX as u64,..Default::default()};
        assert_eq!(frames.generation(&asset(),7,0).unwrap_err().to_string(),"video generation exhausted");
        assert_eq!(frames.counter,i64::MAX as u64);
    }

    struct PrefetchBackend(Arc<std::sync::atomic::AtomicBool>);
    impl crate::platform::Platform for PrefetchBackend {
        fn name(&self) -> &'static str { "prefetch-test" }
        fn probe_video(&self, _: &Path, _: Option<u32>, _: Option<u32>, _: &dyn Fn() -> motion_media::Result<()>) -> motion_media::Result<motion_media::VideoProbe> { Err("not used".into()) }
        fn open_decoder(&self, _: &Path, _: VideoAsset, _: Vec<u64>) -> motion_media::Result<Box<dyn VideoDecoder>> {
            Ok(Box::new(PrefetchDecoder(self.0.clone())))
        }
        fn decode_audio(&self, _: &Path, _: &Path, _: Option<u32>, _: u64, _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>) -> motion_media::Result<motion_core::AudioAsset> { Err("not used".into()) }
        fn media_capabilities(&self, _: Option<&crate::platform::VideoQuery>) -> motion_media::Result<Value> { Err("not used".into()) }
        fn attach_surface(&self, _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>, _: u32, _: u32, _budget: motion_render::AllocationBudget) -> crate::Result<crate::platform::SurfaceTarget> { Err("not used".into()) }
    }
    struct PrefetchDecoder(Arc<std::sync::atomic::AtomicBool>);
    impl VideoDecoder for PrefetchDecoder {
        fn frame(&mut self, time: u64, check: &dyn Fn() -> motion_media::Result<()>) -> motion_media::Result<DecodedFrame> {
            check()?;
            if time != 0 && self.0.load(std::sync::atomic::Ordering::Acquire) {
                return Err("injected speculative decode failure".into());
            }
            Ok(DecodedFrame {
                pixels:crate::video_frame::VideoPixels::Rgba(vec![255;16]),
                pts:time,end:if time==0 {33333}else{66666},width:2,height:2,
                decode_us:0,codec_us:0,transfer_us:0,pack_us:0,
                source_transfer:"test",decoder_name:"controlled".into(),
            })
        }
    }
    #[test]
    fn speculative_decode_failure_is_diagnosed_without_poisoning_the_required_frame() {
        let root = tempfile::tempdir().unwrap();
        let asset = asset();
        std::fs::create_dir(root.path().join("assets")).unwrap();
        std::fs::write(root.path().join(&asset.path),b"1234").unwrap();
        let index = motion_media::video_cache_path(root.path(),&asset).unwrap();
        std::fs::create_dir_all(index.parent().unwrap()).unwrap();
        motion_media::save_index(&index,&asset,&[0,33333]).unwrap();
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let stream = Stream::new(root.path().into(),asset,Arc::new(PrefetchBackend(fail.clone())),7).unwrap();
        stream.request(0,1,true).unwrap();
        let deadline = std::time::Instant::now()+std::time::Duration::from_secs(5);
        loop {
            assert!(std::time::Instant::now()<deadline);
            if stream.shared.0.lock().unwrap().failed_prefetch.is_some() { break; }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(stream.ready(1).is_ok());
        {
            let state = stream.shared.0.lock().unwrap();
            let report = status(&state);
            assert_eq!(report["state"],"ready");
            assert_eq!(report["prefetch_failures"],1);
            assert_eq!(report["prefetch_blocked_pts_us"],33333);
            assert_eq!(report["last_prefetch_error"],"injected speculative decode failure");
        }
        fail.store(false,std::sync::atomic::Ordering::Release);
        stream.request(0,2,true).unwrap();
        loop {
            assert!(std::time::Instant::now()<deadline);
            if stream.shared.0.lock().unwrap().cache.find(33333).is_some() { break; }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(stream.ready(2).is_ok());
        let mut frames = VideoFrames::default();
        frames.streams.insert(7,stream);
        assert_eq!(frames.metrics()["prefetchFailures"],1);
    }

    #[test]
    fn retired_worker_panic_preserves_the_cause_in_result_and_metrics() {
        let mut frames = VideoFrames::default();
        let worker = std::thread::spawn(|| panic!("injected retirement cause"));
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while !worker.is_finished() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        frames.retiring.push(worker);
        let error = frames.reap_retired().unwrap_err();
        assert!(error.to_string().contains("injected retirement cause"), "{error}");
        assert_eq!(frames.metrics()["retirementError"], error.to_string());
        assert_eq!(frames.metrics()["retiringStreams"], 0);
    }
}

#[cfg(test)]
mod observability_tests;
#[cfg(test)]
mod frame_blend_tests;

mod worker_observation;
