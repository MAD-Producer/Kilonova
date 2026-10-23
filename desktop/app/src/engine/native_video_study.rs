//! Explicit file-decoder/window cost study. Never part of ordinary unit runs.
//! Reply completion is not GPU completion or a monitor scanout measurement.
use super::{Command, Engine};
use crate::{panels, workspace, Locale, Shell};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};
use winit::event_loop::EventLoop;

mod fixture;
mod playback;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const COMPOSITION: [u32; 2] = [1280, 720];
const WINDOW: [u32; 2] = [1280, 800];
const FPS: u32 = 30;
const FRAMES: u32 = 600;
const WARMUP: Duration = Duration::from_secs(1);
const MEASURED: Duration = Duration::from_secs(3);
const REPLY_WAIT: Duration = Duration::from_secs(30);

fn require(condition: bool, reason: &str) -> Result<()> {
    if condition { Ok(()) } else { Err(reason.into()) }
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut file = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut file, value)?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}

struct Evidence {
    root: PathBuf,
    start: Instant,
    events: BufWriter<File>,
    frames: BufWriter<File>,
}

impl Evidence {
    fn new(root: &Path) -> Result<Self> {
        std::fs::create_dir(root)?;
        let create = |name| OpenOptions::new().write(true).create_new(true).open(root.join(name));
        Ok(Self { root: root.to_owned(), start: Instant::now(),
            events: BufWriter::new(create("events.jsonl")?),
            frames: BufWriter::new(create("frames.jsonl")?) })
    }

    /// Flush before a potentially blocking operation, outside the render slice.
    fn begin(&mut self, phase: &str, context: Value) -> Result<u64> {
        let began = Instant::now();
        serde_json::to_writer(&mut self.events, &json!({"phase":phase,
            "elapsedUs":self.start.elapsed().as_micros(),"context":context}))?;
        writeln!(self.events)?;
        self.events.flush()?;
        Ok(began.elapsed().as_micros() as u64)
    }

    fn record(&mut self, value: &Value) -> Result<u64> {
        let began = Instant::now();
        serde_json::to_writer(&mut self.frames, value)?;
        writeln!(self.frames)?;
        self.frames.flush()?;
        Ok(began.elapsed().as_micros() as u64)
    }

    fn failure(&mut self, engine: Option<&Engine>, cause: &str) {
        let result = write_json(&self.root.join("failure.json"), &json!({
            "cause":cause.chars().take(4096).collect::<String>(),
            "elapsedUs":self.start.elapsed().as_micros(),
            "worker":engine.map(Engine::native_wait_snapshot),
            "performanceAccepted":false}));
        if let Err(secondary) = result {
            eprintln!("Native study primary failure: {cause}; evidence failure: {secondary}");
        }
    }
}

fn receive<T>(loop_: &mut EventLoop<()>, engine: &Engine,
              reply: &Receiver<std::result::Result<T, String>>, phase: &str,
              evidence: &mut Evidence) -> Result<T> {
    match crate::native_test_support::receive(loop_, reply, REPLY_WAIT) {
        Ok(Ok(value)) => Ok(value),
        result => {
            let cause = match result {
                Ok(Err(reason)) => format!("{phase}: {reason}"),
                Err(reason) => format!("{phase}: {reason}"),
                Ok(Ok(_)) => unreachable!(),
            };
            evidence.failure(Some(engine), &cause);
            Err(cause.into())
        }
    }
}

fn study_operation(loop_: &mut EventLoop<()>, engine: &Engine,
                   operation: fn(i64) -> motion_host::Result<Value>,
                   name: &str, evidence: &mut Evidence) -> Result<Value> {
    evidence.begin(name, Value::Null)?;
    let reply = engine.request(|reply| Command::TestStudy { operation, reply })?;
    receive(loop_, engine, &reply, name, evidence)
}

fn start_profile(id: i64) -> motion_host::Result<Value> {
    motion_host::ops::preview::start_profiling(id, 4096)
}

fn stop_profile(id: i64) -> motion_host::Result<Value> {
    motion_host::ops::preview::stop_profiling(id)
}

/// This final observation is after timing/profiling. prepare_scene_exact uses
/// the real cached native frames; it can change request/prefetch bookkeeping.
/// It does not turn declared VideoAsset dimensions into decoded-frame evidence.
fn actual_sources(id: i64) -> motion_host::Result<Value> {
    use motion_host::video_frame::VideoPixels;
    motion_host::session::with_session(id, |s| {
        let frames = s.video_frames.prepare_scene_exact(s.engine.project(), &s.root, &s.scene)?
            .ok_or("final native source frames are not ready")?;
        Ok(json!(frames.into_iter().map(|(object, sample)| {
            let frame = &sample.left;
            let pixels = match &frame.pixels {
                VideoPixels::Rgba(bytes) => json!({"format":"rgba8", "payloadBytes":bytes.len(),
                    "capacityBytes":bytes.capacity()}),
                VideoPixels::Yuv(yuv) => json!({"format":"yuv420", "width":yuv.width,
                    "height":yuv.height,"standard":yuv.standard,"range":yuv.range,
                    "chromaLayout":format!("{:?}",yuv.chroma_layout),
                    "payloadBytes":yuv.y.len()+yuv.uv.len(),
                    "capacityBytes":yuv.y.capacity()+yuv.uv.capacity()}),
            };
            json!({"object":object,"width":frame.width,"height":frame.height,
                "ptsUs":frame.pts,"endUs":frame.end,"decodeUs":frame.decode_us,
                "codecUs":frame.codec_us,"transferUs":frame.transfer_us,"packUs":frame.pack_us,
                "decoder":frame.decoder_name,"sourceTransfer":frame.source_transfer,
                "pixels":pixels})
        }).collect::<Vec<_>>()))
    })
}

#[test]
#[ignore = "explicit Release performance study; requires four real files and a native Windows desktop"]
fn real_file_video_window_cost_study() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let run = || -> Result<()> {
        require(!cfg!(debug_assertions), "run the native window study in Release")?;
        let compiled_source = option_env!("MOTION_NATIVE_VIDEO_STUDY_REVISION")
            .ok_or("collect with the explicit source-bound native study runner")?;
        require(std::env::var("MOTION_NATIVE_VIDEO_STUDY_REVISION")?.as_str() == compiled_source,
            "native test binary was compiled for a different study source")?;
        let output = PathBuf::from(std::env::var_os("MOTION_NATIVE_VIDEO_STUDY_OUTPUT")
            .ok_or("missing MOTION_NATIVE_VIDEO_STUDY_OUTPUT")?);
        let fixtures = PathBuf::from(std::env::var_os("MOTION_NATIVE_VIDEO_STUDY_FIXTURES")
            .ok_or("missing MOTION_NATIVE_VIDEO_STUDY_FIXTURES")?);
        require(output.is_absolute() && fixtures.is_absolute(), "explicit absolute evidence paths required")?;
        std::fs::create_dir(&output)?;
        let _logger = crate::logging::init(output.clone())?;
        let manifest: Value = serde_json::from_slice(&std::fs::read(fixtures.join("fixtures.json"))?)?;
        fixture::validate_manifest(&manifest)?;
        write_json(&output.join("source-fixtures.json"), &manifest)?;
        let mut loop_ = EventLoop::builder().with_any_thread(true).build()?;
        let mut runs = Vec::new();
        for count in [1, 4] {
            let mut evidence = Evidence::new(&output.join(format!("sources-{count}")))?;
            match playback::run(&mut loop_, &fixtures, &manifest, count, &mut evidence) {
                Ok(report) => runs.push(report),
                Err(error) => {
                    if !evidence.root.join("failure.json").exists() {
                        evidence.failure(None, &error.to_string());
                    }
                    return Err(error);
                }
            }
        }
        write_json(&output.join("study.json"), &json!({"schemaVersion":1,
            "compiledSourceRevision":compiled_source,
            "debugAssertions":cfg!(debug_assertions),"composition":COMPOSITION,"window":WINDOW,
            "sourceDimensions":[3840,2160],"sourceFps":FPS,"sourceFrames":FRAMES,
            "scope":"Production Shell wall-clock playback, real independent file decoders, native visible-window attach/present replies and test-only native message pumping. One and four sources are different workloads, not optimization before/after.",
            "coldScope":"First completed render in a fresh Session/device; OS file and driver caches are not cleared",
            "timingScope":"Render slice excludes explicit state/resource queries and evidence IO; completion intervals and the playback clock include their overhead. GPU profile uses existing sequence-matched root composition/presentation timestamps.",
            "deadlineScope":"Requested source frame next-boundary versus native present reply; not compositor/monitor scanout deadlines or display FPS",
            "excludes":["production_application_input_dispatch","physical_input","audio",
                "full_conversion_nested_vector_msaa_GPU_timing","external_decoder_bytes",
                "driver_physical_residency","mobile_pressure","Adreno_Mali","optimization_gains"],
            "runs":runs}))?;
        Ok(())
    };
    if let Err(error) = run() {
        panic!("explicit native video study failed; original evidence retained: {error}");
    }
}
