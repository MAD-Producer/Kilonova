//! Real video/native resources under an explicit application source allowance.
//! Recovery uses actual successful frames and the production wall-clock policy.
use super::*;
use motion_render::resource_policy::MemoryPolicy;
use std::cell::RefCell;

const HIGH_BUDGET: u64 = 256 << 20;
const LOW_BUDGET: u64 = 32 << 20;
const RECOVERY_SUCCESSES: u32 = 120;

thread_local! {
    // Test-only full-byte oracle; readbacks and this retained CPU copy are
    // outside profiling. The worker owns and drops it with the Session thread.
    static FRONT: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

fn front_bytes(id: i64) -> motion_host::Result<Vec<u8>> {
    motion_host::session::with_session(id, |session| {
        let graphics = session
            .graphics
            .as_ref()
            .ok_or("no attached native renderer")?;
        let renderer = &graphics.renderer;
        let target = renderer.capture_target(graphics.scratch.width, graphics.scratch.height)?;
        motion_render::Presenter::new_admitted(
            renderer,
            &graphics.scratch.view,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )?
        .draw(renderer, &target.view)?;
        Ok(renderer.read_target(&target)?)
    })
}

fn remember_front(id: i64) -> motion_host::Result<Value> {
    let bytes = front_bytes(id)?;
    let count = bytes.len();
    FRONT.with(|front| *front.borrow_mut() = Some(bytes));
    Ok(json!({"actualReadbackBytes":count,"scope":"test oracle outside profiling"}))
}

fn compare_front(id: i64) -> motion_host::Result<Value> {
    let actual = front_bytes(id)?;
    let same = FRONT.with(|front| {
        front
            .borrow()
            .as_ref()
            .is_some_and(|saved| saved == &actual)
    });
    if !same {
        return Err("budget change modified the retained front before a new render".into());
    }
    FRONT.with(|front| *front.borrow_mut() = None);
    Ok(
        json!({"actualReadbackBytes":actual.len(),"allBytesEqual":true,
        "scope":"retained front before any new frame; outside profiling"}),
    )
}

fn configure(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    evidence: &mut Evidence,
    bytes: u64,
) -> Result<Value> {
    evidence.begin("application_source_budget", json!({"bytes":bytes}))?;
    let policy = MemoryPolicy {
        source_texture_budget_bytes: bytes,
        ..Default::default()
    };
    let reply = shell
        .engine
        .plugin_async(json!({"op":"memory_policy","policy":policy}).to_string())?;
    receive(
        events,
        &shell.engine,
        &reply,
        "application_source_budget",
        evidence,
    )
}

fn state(shell: &mut Shell, events: &mut EventLoop<()>, evidence: &mut Evidence) -> Result<Value> {
    let reply = shell.engine.request(Command::State)?;
    receive(events, &shell.engine, &reply, "pressure_state", evidence)
}

fn require_quality(state: &Value, full: bool) -> Result<u64> {
    let preview = &state["preview"];
    let adaptation = &preview["memoryAdaptation"];
    let level = adaptation["actual"]["level"]
        .as_u64()
        .ok_or("missing actual preview quality")?;
    require(
        preview["mode"] == "high"
            && preview["video"]["streams"] == 4
            && state["renderError"].is_null()
            && adaptation["hasLastValidFrame"] == true,
        "pressure study lost native sources, preview mode, or the valid front",
    )?;
    if full {
        require(
            level == 0
                && adaptation["status"] == "normal"
                && preview["videoResolution"] == "decoded_source"
                && adaptation["actualWidth"] == COMPOSITION[0]
                && adaptation["actualHeight"] == COMPOSITION[1],
            "full-resolution video did not recover",
        )?;
    } else {
        require(
            (1..u64::from(motion_render::preview_memory::PREVIEW_MEMORY_LEVELS)).contains(&level)
                && adaptation["status"] == "degraded"
                && adaptation["refusalCount"].as_u64().is_some_and(|n| n > 0)
                && adaptation["actualWidth"] == COMPOSITION[0].div_ceil(1 << level)
                && adaptation["actualHeight"] == COMPOSITION[1].div_ceil(1 << level),
            "lower application allowance did not produce an observable bounded preview degradation",
        )?;
    }
    Ok(level)
}

fn seek_source_frame(
    shell: &mut Shell,
    frame: u32,
) -> Result<Receiver<std::result::Result<Value, String>>> {
    // Use the ordinary action: it updates Shell.frame before dispatching the
    // same worker seek. accept_edit_state alone intentionally updates only the
    // snapshot, so a raw worker command leaves the driver's expected ID stale.
    shell.action(panels::Action::Seek(f64::from(frame)), None)?;
    Ok(shell
        .editing
        .take()
        .ok_or("native pressure seek did not dispatch a worker request")?
        .1)
}

fn step(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    evidence: &mut Evidence,
    frame: &mut u32,
    records: &mut Vec<Value>,
) -> Result<Value> {
    require(
        *frame < FRAMES,
        "pressure study exhausted its authored source interval",
    )?;
    evidence.begin("stepped_source_frame", json!({"sourceFrame":*frame}))?;
    let began = Instant::now();
    let reply = seek_source_frame(shell, *frame)?;
    let edit = receive(events, &shell.engine, &reply, "pressure_seek", evidence)?;
    shell.accept_edit_state(edit);
    require(
        shell.frame == f64::from(*frame),
        "native pressure seek did not update the shell frame",
    )?;
    let seek_us = began.elapsed().as_micros();
    let mut record = playback::present(shell, events, evidence)?;
    record["seekReplyUs"] = json!(seek_us);
    record["cycleWithSeekUs"] = json!(began.elapsed().as_micros());
    let began = Instant::now();
    let observed = state(shell, events, evidence)?;
    record["stateObservationUs"] = json!(began.elapsed().as_micros());
    record["lastPresentedFrame"] = observed["lastPresentedFrame"].clone();
    record["preview"] = observed["preview"].clone();
    record["renderError"] = observed["renderError"].clone();
    record["uiPerformance"] = observed["desktop"]["uiPerformance"].clone();
    require(
        record["lastPresentedFrame"] == record["sourceFrame"],
        &format!(
            "pressure frame identity differs: requested={} shell={} recorded={} presented={}",
            *frame, shell.frame, record["sourceFrame"], record["lastPresentedFrame"]
        ),
    )?;
    let began = Instant::now();
    let reply = shell
        .engine
        .plugin_async(json!({"op":"resource_memory"}).to_string())?;
    record["resourceMemory"] = receive(
        events,
        &shell.engine,
        &reply,
        "pressure_resources",
        evidence,
    )?;
    record["resourceObservationUs"] = json!(began.elapsed().as_micros());
    record["allocationFrames"] = json!(shell.engine.allocation_frame_peaks()?);
    evidence.record(&record)?;
    records.push(record);
    shell.state = observed.clone();
    *frame += 1;
    Ok(observed)
}

fn finish_phase(
    shell: &mut Shell,
    events: &mut EventLoop<()>,
    fixtures: &Path,
    manifest: &Value,
    evidence: &mut Evidence,
    records: &[Value],
) -> Result<Value> {
    let stopped = study_operation(
        events,
        &shell.engine,
        stop_profile,
        "pressure_profile_stop",
        evidence,
    )?;
    let path = PathBuf::from(
        stopped["file"]
            .as_str()
            .ok_or("missing native profile path")?,
    );
    let bytes = std::fs::read(&path)?;
    let profile: Value = serde_json::from_slice(&bytes)?;
    require(
        profile["frameCount"] == records.len() && profile["overflowFrames"] == 0,
        "pressure profile lost successful completed frames",
    )?;
    // A later phase overwrites the host export; retain these original bytes now.
    std::fs::write(evidence.root.join("native-profile.json"), &bytes)?;
    write_json(
        &evidence.root.join("profile-origin.json"),
        &json!({"hostPath":path,
        "originalBytes":bytes.len(),"retainedWithoutReserialization":true}),
    )?;
    write_json(
        &evidence.root.join("completion-records.json"),
        &json!({"records":records}),
    )?;
    evidence.begin("native_capture_outside_profile", Value::Null)?;
    let (paint, actions) = shell.draw_commands(WINDOW.map(|v| v as f32));
    require(actions.is_empty(), "unexpected pressure-study UI action")?;
    let ui = shell.ui_frame(paint, WINDOW.map(|v| v as f32), true);
    let image = ui
        .composition_image
        .ok_or("missing pressure image mapping")?;
    let viewport = ui.composition.ok_or("missing pressure viewport")?;
    crate::native_test_support::capture_ui(
        events,
        &shell.engine,
        ui,
        evidence.root.join("window.png"),
    )?;
    let pixels = fixture::verify_pixels(fixtures, manifest, 4, image, viewport, evidence)?;
    let mut report = json!({"completedFrames":records.len(),"lastPreview":shell.state["preview"],"pixels":pixels});
    for metric in [
        "cycleWithSeekUs",
        "renderCycleUs",
        "presentReplyUs",
        "stateObservationUs",
        "resourceObservationUs",
    ] {
        report[metric] = playback::distribution(records, metric);
    }
    Ok(report)
}

pub(super) fn run(
    events: &mut EventLoop<()>,
    fixtures: &Path,
    manifest: &Value,
    evidence: &mut Evidence,
) -> Result<Value> {
    let project = fixture::create(fixtures, manifest, 4, evidence)?;
    evidence.begin("pressure_shell_start", Value::Null)?;
    let mut shell = Shell::new(project, None, Some(Locale::En))?;
    let result = (|| -> Result<Value> {
        shell.workspace = workspace::Workspace::default();
        shell.home = None;
        shell.scale = 1.;
        configure(&mut shell, events, evidence, HIGH_BUDGET)?;
        let reply = shell.engine.request(|reply| Command::PreviewMode {
            mode: 1,
            thermal: 0,
            reply,
        })?;
        receive(
            events,
            &shell.engine,
            &reply,
            "pressure_preview_mode",
            evidence,
        )?;
        evidence.begin(
            "pressure_native_window",
            json!({"visible":true,"size":WINDOW}),
        )?;
        #[allow(deprecated)]
        let window = Arc::new(
            events.create_window(
                winit::window::Window::default_attributes()
                    .with_title("Motion Studio native video application budget study")
                    .with_visible(true)
                    .with_resizable(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(WINDOW[0], WINDOW[1])),
            )?,
        );
        let reply = shell
            .engine
            .attach_async(window.clone(), WINDOW[0], WINDOW[1])?;
        shell.state = receive(events, &shell.engine, &reply, "pressure_attach", evidence)?;
        shell.window = Some(window);
        shell.attached = true;
        let original_project = shell.state["project"].clone();
        let revision = shell.state["revision"].clone();
        let mut frame = 0;
        let mut reports = Vec::new();
        for name in ["normal", "limited"] {
            let retained = if name == "limited" {
                study_operation(
                    events,
                    &shell.engine,
                    remember_front,
                    "remember_full_front",
                    evidence,
                )?;
                configure(&mut shell, events, evidence, LOW_BUDGET)?;
                study_operation(
                    events,
                    &shell.engine,
                    compare_front,
                    "compare_full_front",
                    evidence,
                )?
            } else {
                Value::Null
            };
            let mut phase = Evidence::new(&evidence.root.join(name))?;
            study_operation(
                events,
                &shell.engine,
                start_profile,
                "pressure_profile_start",
                &mut phase,
            )?;
            let mut records = Vec::new();
            for _ in 0..3 {
                let observed = step(&mut shell, events, &mut phase, &mut frame, &mut records)?;
                require_quality(&observed, name == "normal")?;
            }
            let mut report =
                finish_phase(&mut shell, events, fixtures, manifest, &mut phase, &records)?;
            report["name"] = json!(name);
            report["sourceBudgetBytes"] = json!(if name == "normal" {
                HIGH_BUDGET
            } else {
                LOW_BUDGET
            });
            report["retainedFront"] = retained;
            reports.push(report);
        }
        let limited = require_quality(&shell.state, false)?;
        study_operation(
            events,
            &shell.engine,
            remember_front,
            "remember_limited_front",
            evidence,
        )?;
        configure(&mut shell, events, evidence, HIGH_BUDGET)?;
        let retained = study_operation(
            events,
            &shell.engine,
            compare_front,
            "compare_limited_front",
            evidence,
        )?;
        // changed() intentionally keeps the retained level. Do not reset it or
        // fake the production clock. Render real successful frames, then wait
        // out the reported cooldown with native message pumping.
        for recovery in 1..=limited {
            let name = format!("recovery-{recovery}");
            let mut phase = Evidence::new(&evidence.root.join(&name))?;
            study_operation(
                events,
                &shell.engine,
                start_profile,
                "pressure_profile_start",
                &mut phase,
            )?;
            let mut records = Vec::new();
            let expected = limited - recovery;
            for _ in 0..RECOVERY_SUCCESSES {
                let observed = step(&mut shell, events, &mut phase, &mut frame, &mut records)?;
                require(
                    observed["renderError"].is_null(),
                    "recovery warm frames failed",
                )?;
            }
            let waiting = Instant::now();
            let before_probe = state(&mut shell, events, &mut phase)?;
            let needs_probe =
                before_probe["preview"]["memoryAdaptation"]["actual"]["level"] != expected;
            let initial_cooldown =
                before_probe["preview"]["memoryAdaptation"]["nextProbeInMs"].clone();
            if needs_probe {
                loop {
                    let observed = state(&mut shell, events, &mut phase)?;
                    if observed["preview"]["memoryAdaptation"]["nextProbeInMs"] == 0 {
                        break;
                    }
                    require(
                        waiting.elapsed() < Duration::from_secs(35),
                        "reported recovery cooldown never expired",
                    )?;
                    let (_send, receive) = std::sync::mpsc::channel::<()>();
                    let _ = crate::native_test_support::receive(
                        events,
                        &receive,
                        Duration::from_millis(50),
                    );
                }
            }
            let wait_us = waiting.elapsed().as_micros();
            let observed = if needs_probe {
                step(&mut shell, events, &mut phase, &mut frame, &mut records)?
            } else {
                before_probe
            };
            require(
                observed["preview"]["memoryAdaptation"]["actual"]["level"] == expected,
                "successful production recovery did not lower the retained quality level",
            )?;
            let mut report =
                finish_phase(&mut shell, events, fixtures, manifest, &mut phase, &records)?;
            report["name"] = json!(name);
            report["sourceBudgetBytes"] = json!(HIGH_BUDGET);
            report["cooldownWaitUs"] = json!(wait_us);
            report["cooldownBeforeWaitMs"] = initial_cooldown;
            report["successfulWarmFrames"] = json!(RECOVERY_SUCCESSES);
            report["probeAfterWait"] = json!(needs_probe);
            reports.push(report);
        }
        require_quality(&shell.state, true)?;
        require(
            shell.state["project"] == original_project && shell.state["revision"] == revision,
            "pressure study changed project or history",
        )?;
        let sources = study_operation(
            events,
            &shell.engine,
            actual_sources,
            "final_pressure_sources",
            evidence,
        )?;
        write_json(
            &evidence.root.join("actual-native-source-frames.json"),
            &sources,
        )?;
        Ok(
            json!({"sourceCount":4,"highSourceBudgetBytes":HIGH_BUDGET,"lowSourceBudgetBytes":LOW_BUDGET,
            "limitedQualityLevel":limited,"restoredFront":retained,"phases":reports,
            "sourceFramesConsumed":frame,"projectUnchanged":true,"actualSources":sources,
            "scope":"Explicit application source-budget change; real files, GPU resources and native pixels; not driver/physical VRAM exhaustion or playback FPS"}),
        )
    })();
    if let Err(error) = &result {
        if !evidence.root.join("failure.json").exists() {
            evidence.failure(Some(&shell.engine), &error.to_string());
        }
    }
    result
}

#[test]
fn stepped_native_pressure_seek_updates_shell_and_worker_frame() {
    let directory = tempfile::tempdir().unwrap();
    let mut shell = Shell::new(directory.path().join("project"), None, Some(Locale::En)).unwrap();
    for frame in [0, 1, 2] {
        let reply = seek_source_frame(&mut shell, frame).unwrap();
        assert_eq!(shell.frame, f64::from(frame));
        let observed = reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(observed["frame"].as_f64(), Some(f64::from(frame)));
        shell.accept_edit_state(observed);
        assert_eq!(shell.frame, f64::from(frame));
    }
}

#[test]
#[ignore = "explicit Release native application-budget study; real four-file video and production recovery cooldown"]
fn real_file_video_pressure_cost_study() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let run = || -> Result<()> {
        require(!cfg!(debug_assertions), "pressure study requires Release")?;
        let compiled = option_env!("MOTION_NATIVE_VIDEO_STUDY_REVISION")
            .ok_or("use the source-bound pressure collector")?;
        require(
            std::env::var("MOTION_NATIVE_VIDEO_STUDY_REVISION")?.as_str() == compiled,
            "pressure study compiled revision mismatch",
        )?;
        let output = PathBuf::from(
            std::env::var_os("MOTION_NATIVE_VIDEO_STUDY_OUTPUT").ok_or("missing output")?,
        );
        let fixtures = PathBuf::from(
            std::env::var_os("MOTION_NATIVE_VIDEO_STUDY_FIXTURES").ok_or("missing fixtures")?,
        );
        require(
            output.is_absolute() && fixtures.is_absolute(),
            "absolute pressure paths required",
        )?;
        std::fs::create_dir(&output)?;
        let _logger = crate::logging::init(output.clone())?;
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(fixtures.join("fixtures.json"))?)?;
        fixture::validate_manifest(&manifest)?;
        write_json(&output.join("source-fixtures.json"), &manifest)?;
        let mut events = EventLoop::builder().with_any_thread(true).build()?;
        let mut evidence = Evidence::new(&output.join("sources-4"))?;
        let report = self::run(&mut events, &fixtures, &manifest, &mut evidence)?;
        write_json(
            &output.join("study.json"),
            &json!({"compiledSourceRevision":compiled,
            "debugAssertions":cfg!(debug_assertions),"composition":COMPOSITION,"window":WINDOW,
            "physicalPressure":false,"playbackFpsClaimed":false,"report":report}),
        )?;
        Ok(())
    };
    if let Err(error) = run() {
        panic!("native application-budget study failed: {error}");
    }
}
