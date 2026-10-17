//! Exact PTS lookup with bounded forward prefetch, independent of render ticks.
use crate::platform::VideoDecoder;
use crate::video_cache::{FrameCache, LOOKAHEAD};
use crate::video_frame::DecodedFrame;
use motion_core::{Content, Project, VideoAsset};
use motion_render::Scene;
use motion_media::Result;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};
static PREFETCH_FAILURES: motion_render::observability::FailureCounter =
    motion_render::observability::FailureCounter::new();

#[derive(Clone, Copy)]
struct Request {
    time: u64,
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
    error: Option<String>,
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
}
fn job(s: &State, pts: &[u64]) -> Option<(u64, bool)> {
    let r = s.desired?;
    if s.cache.find(r.time).is_none() {
        return s.error.is_none().then_some((r.time, false));
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
        std::thread::Builder::new()
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
                    .unwrap_or_else(|payload| Err(format!("video decoder worker failed: {}",motion_media::panic_reason(payload))));
                    let mut s = copy.0.lock().unwrap_or_else(|e| e.into_inner());
                    s.seeks += decoder
                        .as_ref()
                        .map_or(0u64, |d| d.seeks())
                        .saturating_sub(previous_seeks);
                    if s.stop {
                        s.in_flight = None;
                        drop(s);
                        if let Err(error) = &result {
                            if error != "video target superseded" {
                                log::error!("video decode discarded failure object={object} source={} sequence={} pts_us={target} {}ms stopped=true reason={error}", media.id, flight.sequence, flight.began.elapsed().as_millis());
                            }
                        }
                        observation.stopped("stop_after_decode");
                        return;
                    }
                    let desired = s.desired.unwrap();
                    let last = times[(desired.index + LOOKAHEAD).min(times.len() - 1)];
                    let phase = s.in_flight.map_or(flight.phase, |active| active.phase);
                    let mut failure = result.as_ref().err()
                        .filter(|error| error.as_str() != "video target superseded" && (target == desired.time || !speculative))
                        .map(|error| (error.clone(), phase));
                    let frame_bytes = result.as_ref().ok().map(|frame| frame.bytes());
                    let cache_budget = s.cache.budget();
                    match result {
                        Ok(frame) => {
                            s.decoded += 1;
                            if frame.pts >= desired.time && frame.pts <= last {
                                if !s.cache.insert(frame, desired.time) {
                                    if target == desired.time {
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
                        Err(e) if e == "video target superseded" => {
                            s.cancelled += 1;
                        }
                        Err(e) => {
                            decoder = None;
                            if target == desired.time {
                                s.error = Some(e);
                            } else if speculative {
                                s.prefetch_failed(media.id, target, &e);
                            }
                        }
                    }
                    s.in_flight = None;
                    drop(s);
                    if let Some((error, phase)) = failure {
                        log::error!("video decode failed object={object} source={} sequence={} pts_us={target} {}ms phase={phase} speculative={speculative} requested_sequence={} requested_pts_us={} discarded={} frame_bytes={frame_bytes:?} cache_budget_bytes={cache_budget} reason={error}", media.id, flight.sequence, flight.began.elapsed().as_millis(), desired.sequence, desired.time, target != desired.time);
                    }
                    if flight.began.elapsed().as_millis() >= 100 {
                        log::debug!("video decode done object={object} source={} sequence={} pts_us={target} {}ms speculative={speculative}", media.id, flight.sequence, flight.began.elapsed().as_millis());
                    }
                    // Do not await another VSync: fill the next available cache slot.
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { object, asset, pts, shared })
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
    fn request(&self, time: u64, sequence: u64, prefetch: bool) -> Result<Value> {
        if sequence == 0 || sequence > i64::MAX as u64 {
            return Err("video sequence must be a positive signed 64-bit value".into());
        }
        let source_time = time;
        let (time, index) = self.key(time)?;
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
        }
        s.prefetch = prefetch;
        self.shared.1.notify_one();
        s.pending_polls = if s.error.is_none() && s.cache.find(time).is_none() {
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
        s.cache.find(r.time).ok_or("video frame pending".into())
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        let mut s = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        s.stop = true;
        s.cache = FrameCache::new();
        self.shared.1.notify_one();
    }
}
fn status(s: &State) -> Value {
    let r = s.desired.unwrap();
    let frame = s.cache.find(r.time);
    let state = if s.error.is_some() {
        "failed"
    } else if frame.is_some() {
        "ready"
    } else {
        "pending"
    };
    json!({"state":state,"sequence":r.sequence,"source_time_us":r.source_time,
        "pts_us":frame.as_ref().map(|f|f.pts),"end_us":frame.as_ref().map(|f|f.end),
        "decode_us":frame.as_ref().map(|f|f.decode_us),"codec_us":frame.as_ref().map(|f|f.codec_us),
        "transfer_us":frame.as_ref().map(|f|f.transfer_us),"pack_us":frame.as_ref().map(|f|f.pack_us),
        "cache_bytes":s.cache.bytes(),"cache_frames":s.cache.len(),"cache_hits":s.hits,"cache_misses":s.misses,"error":s.error,
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
            .ok_or_else(|| "video frames require a host platform".to_owned())
    }
    pub fn clear(&mut self) {
        self.streams.clear();
        self.sequences.clear();
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
            self.streams.remove(&object);
            return Ok(
                json!({"state":"outside","sequence":sequence,"source_time_us":source,"bytes":0,"object":object}),
            );
        }
        let generation = self.generation(asset, object, source as u64)?;
        let mut result =
            self.request_source(root, asset, object, source as u64, generation, false)?;
        self.sequences.insert(object, (sequence, frame, generation));
        result["sequence"] = json!(sequence);
        Ok(result)
    }
    fn generation(&mut self, asset: &VideoAsset, object: u64, time: u64) -> Result<u64> {
        // A layer can change sources while retaining its object id. Retire that
        // stream before interpreting its old timebase or poisoned state.
        if self.streams.get(&object).is_some_and(|stream| stream.asset != *asset) {
            self.streams.remove(&object);
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
    ) -> Result<Value> {
        if self.streams.get(&object).is_some_and(|s| s.asset != *asset) {
            self.streams.remove(&object);
        }
        if !self.streams.contains_key(&object) {
            if self.streams.len() >= 4 {
                return Err("at most four video instances per reader; release idle frames".into());
            }
            self.streams.insert(
                object,
                Stream::new(root.into(), asset.clone(), self.backend()?, object)?,
            );
        }
        let mut result = self.streams[&object].request(source, generation, prefetch)?;
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
    ) -> Result<Arc<DecodedFrame>> {
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
    ) -> Result<Option<Vec<(u64, Arc<DecodedFrame>)>>> {
        self.prepare(project, root, scene, true)
    }
    pub fn prepare_scene_exact(
        &mut self,
        project: &Project,
        root: &Path,
        scene: &Scene,
    ) -> Result<Option<Vec<(u64, Arc<DecodedFrame>)>>> {
        self.prepare(project, root, scene, false)
    }
    fn prepare(
        &mut self,
        project: &Project,
        root: &Path,
        scene: &Scene,
        prefetch: bool,
    ) -> Result<Option<Vec<(u64, Arc<DecodedFrame>)>>> {
        let layers=scene.video_layers();
        self.streams.retain(|object,_|layers.iter().any(|l|l.id==*object));
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
            )?;
            match r["state"].as_str() {
                Some("ready") => {
                    frames.push((layer.id, self.streams[&layer.id].ready(generation)?))
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
        json!({"cacheBytes":bytes,"cacheFrames":frames,"cacheBudgetBytes":budget,"maxCacheBudgetBytes":4 * crate::video_cache::MAX_CACHE_BYTES,
            "cacheHits":hits,"cacheMisses":misses,"decodedFrames":decoded,"cancelledFrames":cancelled,"decoderSeeks":seeks,"prefetchFailures":prefetch_failures,"lookaheadFrames":LOOKAHEAD,"streams":self.streams.len(),"decodeInFlight":in_flight,"decodeWorkersStopped":stopped_workers})
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
            object:7,
            asset:asset.clone(),
            pts:Arc::new(vec![0,33333]),
            shared:Arc::new((Mutex::new(State {
                desired:Some(Request {time:0,source_time:0,index:0,sequence:10}),
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
        assert!(error.contains("state poisoned") && error.contains("object=7"));
        assert_eq!(frames.counter,10);
    }

    #[test]
    fn same_source_reuses_frame_generation_but_bad_timebase_does_not_advance_it() {
        let asset = asset();
        let mut frames = frames(&asset);
        assert_eq!(frames.generation(&asset,7,1000).unwrap(),10);
        assert!(frames.generation(&asset,7,90000).unwrap_err().contains("outside visible video"));
        assert_eq!(frames.counter,10);
        frames.streams.get_mut(&7).unwrap().pts=Arc::new(vec![]);
        assert!(frames.generation(&asset,7,0).unwrap_err().contains("PTS index missing"));
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
        assert_eq!(frames.generation(&asset(),7,0).unwrap_err(),"video generation exhausted");
        assert_eq!(frames.counter,i64::MAX as u64);
    }

    struct PrefetchBackend(Arc<std::sync::atomic::AtomicBool>);
    impl crate::platform::Platform for PrefetchBackend {
        fn name(&self) -> &'static str { "prefetch-test" }
        fn probe_video(&self, _: &Path, _: Option<u32>, _: Option<u32>, _: &dyn Fn() -> Result<()>) -> Result<motion_media::VideoProbe> { Err("not used".into()) }
        fn open_decoder(&self, _: &Path, _: VideoAsset, _: Vec<u64>) -> Result<Box<dyn VideoDecoder>> {
            Ok(Box::new(PrefetchDecoder(self.0.clone())))
        }
        fn decode_audio(&self, _: &Path, _: &Path, _: Option<u32>, _: u64, _: &mut dyn FnMut(f64) -> Result<()>) -> Result<motion_core::AudioAsset> { Err("not used".into()) }
        fn media_capabilities(&self, _: Option<&crate::platform::VideoQuery>) -> Result<Value> { Err("not used".into()) }
        fn attach_surface(&self, _: Arc<dyn wgpu::WindowHandle + Send + Sync>, _: u32, _: u32) -> Result<crate::platform::SurfaceTarget> { Err("not used".into()) }
    }
    struct PrefetchDecoder(Arc<std::sync::atomic::AtomicBool>);
    impl VideoDecoder for PrefetchDecoder {
        fn frame(&mut self, time: u64, check: &dyn Fn() -> Result<()>) -> Result<DecodedFrame> {
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
}

#[cfg(test)]
mod observability_tests;

mod worker_observation;
