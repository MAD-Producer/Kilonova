use super::*;

pub(super) fn distribution(records: &[Value], name: &str) -> Value {
    let mut values: Vec<_> = records.iter().filter_map(|r|r[name].as_f64()).collect();
    values.sort_by(f64::total_cmp);
    if values.is_empty() { return Value::Null; }
    let percentile = |p:f64| values[((values.len() as f64*p).ceil() as usize).saturating_sub(1)];
    json!({"count":values.len(),"p50":percentile(0.5),"p95":percentile(0.95),
        "p99":percentile(0.99),"max":values.last()})
}

pub(super) fn present(shell: &mut Shell, loop_: &mut EventLoop<()>, evidence: &mut Evidence) -> Result<Value> {
    let began = Instant::now();
    let expected = shell.frame;
    let mut polls = 0u32;
    let mut ui_us = 0u128;
    let mut present_us = 0u128;
    loop {
        require(began.elapsed() < REPLY_WAIT, "native video render never became ready")?;
        let build = Instant::now();
        let (paint, actions) = shell.draw_commands(WINDOW.map(|v| v as f32));
        require(actions.is_empty(), "unexpected UI action in the passive study")?;
        let ui = shell.ui_frame(paint, WINDOW.map(|v| v as f32), false);
        require(ui.composition.is_some(), "composition viewport missing")?;
        ui_us += build.elapsed().as_micros();
        shell.track_exposure_present(0, true);
        let started = Instant::now();
        let reply = shell.engine.present_async(ui)?;
        let result = crate::native_test_support::receive(loop_, &reply, REPLY_WAIT);
        present_us += started.elapsed().as_micros();
        match result {
            Ok(result) => {
                shell.finish_exposure_present(0, &result);
                shell.present_due = false;
                shell.pending = !result?;
            }
            Err(cause) => {
                evidence.failure(Some(&shell.engine), &format!("present reply: {cause}"));
                return Err(cause.into());
            }
        }
        polls += 1;
        if !shell.pending { break; }
    }
    Ok(json!({"sourceFrame":expected,"sourceFrameBucket":expected.floor() as u64,
        "uiBuildUs":ui_us,"presentReplyUs":present_us,
        "renderCycleUs":began.elapsed().as_micros(),"presentPolls":polls}))
}

fn observe(shell: &mut Shell, loop_: &mut EventLoop<()>, evidence: &mut Evidence,
           record: &mut Value) -> Result<()> {
    let began = Instant::now();
    let reply = shell.engine.request(Command::State)?;
    let state = receive(loop_, &shell.engine, &reply, "frame_state", evidence)?;
    record["stateObservationUs"] = json!(began.elapsed().as_micros());
    record["lastPresentedFrame"] = state["lastPresentedFrame"].clone();
    record["preview"] = state["preview"].clone();
    record["uiPerformance"] = state["desktop"]["uiPerformance"].clone();
    let began = Instant::now();
    let reply = shell.engine.plugin_async(json!({"op":"resource_memory"}).to_string())?;
    record["resourceMemory"] = receive(loop_, &shell.engine, &reply, "resource_memory", evidence)?;
    record["resourceObservationUs"] = json!(began.elapsed().as_micros());
    require(record["sourceFrame"] == record["lastPresentedFrame"],
        "native reply did not present the requested source frame")?;
    require(state["preview"]["mode"] == "high" && state["preview"]["videoResolution"] == "decoded_source"
        && state["preview"]["memoryAdaptation"]["status"] == "normal"
        && state["preview"]["memoryAdaptation"]["actualWidth"] == COMPOSITION[0]
        && state["preview"]["memoryAdaptation"]["actualHeight"] == COMPOSITION[1],
        "preview silently changed quality/resolution or refused the fixed study")?;
    require(state["renderError"].is_null(), "native render reported an error")?;
    shell.state = state;
    Ok(())
}

pub(super) fn run(loop_: &mut EventLoop<()>, fixtures: &Path, manifest: &Value,
                  count: usize, evidence: &mut Evidence) -> Result<Value> {
    let project = fixture::create(fixtures, manifest, count, evidence)?;
    evidence.begin("shell_start", json!({"sources":count}))?;
    let mut shell = Shell::new(project, None, Some(Locale::En))?;
    let result = (|| -> Result<Value> {
        shell.workspace = workspace::Workspace::default();
        shell.home = None;
        shell.scale = 1.;
        evidence.begin("memory_policy", Value::Null)?;
        let policy = motion_render::resource_policy::MemoryPolicy {
            source_texture_budget_bytes:256 << 20,
            ..Default::default()
        };
        let reply = shell.engine.plugin_async(json!({"op":"memory_policy","policy":policy}).to_string())?;
        receive(loop_, &shell.engine, &reply, "memory_policy", evidence)?;
        // Reuse the existing typed host operation on the same serial worker.
        let reply = shell.engine.request(|reply| Command::PreviewMode {mode:1,thermal:0,reply})?;
        receive(loop_, &shell.engine, &reply, "preview_mode", evidence)?;
        evidence.begin("native_window_create", json!({"window":WINDOW,"visible":true}))?;
        #[allow(deprecated)]
        let window = Arc::new(loop_.create_window(winit::window::Window::default_attributes()
            .with_title(format!("Motion Studio native video study — {count} sources"))
            .with_visible(true).with_resizable(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(WINDOW[0],WINDOW[1])))?);
        let monitor = window.current_monitor().map(|m| json!({"name":m.name(),
            "refreshRateMillihertz":m.refresh_rate_millihertz(),"scaleFactor":m.scale_factor()}));
        evidence.begin("native_attach", Value::Null)?;
        let reply = shell.engine.attach_async(window.clone(),WINDOW[0],WINDOW[1])?;
        shell.state = receive(loop_, &shell.engine, &reply, "attach", evidence)?;
        shell.window = Some(window.clone());
        shell.attached = true;
        let before_project = shell.state["project"].clone();
        let before_revision = shell.state["revision"].clone();
        let profile = study_operation(loop_, &shell.engine, start_profile, "profile_start", evidence)?;
        write_json(&evidence.root.join("profile-start.json"), &profile)?;
        evidence.begin("cold_first_present", json!({"sourceFrame":0}))?;
        let mut cold = present(&mut shell, loop_, evidence)?;
        cold["phase"] = json!("fresh_session_first_render");
        observe(&mut shell, loop_, evidence, &mut cold)?;
        let mut persistence_us = vec![evidence.record(&cold)?];
        shell.action(panels::Action::TogglePlay, None)?;
        require(shell.audio.is_none(), "video-only study unexpectedly started audio")?;
        let clock = shell.playback.ok_or("production playback did not start")?.0;
        let horizon = WARMUP + MEASURED;
        let mut previous_completion = 0u128;
        let mut previous_bucket = None;
        let mut warmup = Vec::new();
        let mut records = Vec::new();
        while clock.elapsed() < horizon {
            // Use production admission, including its next-composition clock.
            // Pump native messages during waits; do not invent fixed frame IDs.
            while !shell.playback_ready(Instant::now()) && clock.elapsed() < horizon {
                let (_send, receive) = std::sync::mpsc::channel::<()>();
                let _ = crate::native_test_support::receive(loop_, &receive, Duration::from_millis(1));
            }
            if clock.elapsed() >= horizon { break; }
            let event_us = evidence.begin("playback_cycle", json!({"clockUs":clock.elapsed().as_micros()}))?;
            if clock.elapsed() >= horizon { break; }
            let began = Instant::now();
            shell.advance_playback()?;
            let admission_us = clock.elapsed().as_micros();
            let (_, reply) = shell.editing.take().ok_or("playback did not dispatch a seek")?;
            let state = receive(loop_, &shell.engine, &reply, "playback_seek", evidence)?;
            shell.accept_edit_state(state);
            let seek_us = began.elapsed().as_micros();
            let mut record = present(&mut shell, loop_, evidence)?;
            let completion_us = clock.elapsed().as_micros();
            let bucket = shell.frame.floor() as u64;
            record["phase"] = json!(if admission_us < WARMUP.as_micros() {"warmup"} else {"measured"});
            record["playbackAdmissionUs"] = json!(admission_us);
            record["seekReplyUs"] = json!(seek_us);
            record["cycleWithSeekUs"] = json!(began.elapsed().as_micros());
            record["completionFromPlaybackUs"] = json!(completion_us);
            record["completionIntervalUs"] = json!(completion_us-previous_completion);
            record["eventWriteUs"] = json!(event_us);
            record["sourceDeadlineUs"] = json!((u128::from(bucket)+1)*1_000_000/u128::from(FPS));
            record["requestedSourceDeadlineMissed"] = json!(completion_us > (u128::from(bucket)+1)*1_000_000/u128::from(FPS));
            record["skippedSourceBuckets"] = json!(previous_bucket.map_or(0, |previous:u64|bucket.saturating_sub(previous).saturating_sub(1)));
            record["repeatedSourceBucket"] = json!(previous_bucket == Some(bucket));
            previous_bucket = Some(bucket);
            previous_completion = completion_us;
            observe(&mut shell, loop_, evidence, &mut record)?;
            persistence_us.push(evidence.record(&record)?);
            if admission_us < WARMUP.as_micros() { warmup.push(record); }
            else { records.push(record); }
        }
        let elapsed_us = clock.elapsed().as_micros();
        shell.action(panels::Action::TogglePlay, None)?;
        require(shell.playback.is_none() && !shell.engine.failed(), "playback did not stop cleanly")?;
        let stop = study_operation(loop_, &shell.engine, stop_profile, "profile_stop", evidence)?;
        let profile_path = PathBuf::from(stop["file"].as_str().ok_or("profile did not return an output file")?);
        let profile_bytes = std::fs::read(&profile_path)?;
        let profile: Value = serde_json::from_slice(&profile_bytes)?;
        // Keep the primary host report byte-for-byte. A second JSON float
        // parse/serialize can otherwise change an exact fractional frame.
        std::fs::write(evidence.root.join("native-profile.json"), &profile_bytes)?;
        write_json(&evidence.root.join("completion-records.json"), &json!({"cold":cold,
            "warmup":warmup,"measured":records,"playbackWallUs":elapsed_us,
            "recordPersistenceUs":persistence_us}))?;
        require(!warmup.is_empty() && !records.is_empty(), "fixed source interval produced no complete samples")?;
        require(shell.state["project"] == before_project && shell.state["revision"] == before_revision,
            "study changed project/history")?;
        require(shell.state["preview"]["video"]["streams"] == count,
            "native playback did not create the required independent decoder streams")?;
        require(profile["overflowFrames"] == 0, "native recorder overflowed")?;
        require(elapsed_us < u128::from(FRAMES)*1_000_000/u128::from(FPS),
            "study exceeded the source/project horizon; looped playback is not accepted")?;
        let sources = study_operation(loop_, &shell.engine, actual_sources, "final_source_observation", evidence)?;
        write_json(&evidence.root.join("actual-native-source-frames.json"), &sources)?;
        let sources_array = sources.as_array().ok_or("native source frames missing")?;
        require(sources_array.len() == count && sources_array.iter().all(|s|
            s["width"] == 3840 && s["height"] == 2160 && s["pixels"]["payloadBytes"].as_u64().is_some_and(|b|b>0)),
            "actual decoded sources are not the required full 4K frames")?;
        evidence.begin("native_ui_capture_excluded_from_timing", Value::Null)?;
        let (paint, actions) = shell.draw_commands(WINDOW.map(|v|v as f32));
        require(actions.is_empty(), "unexpected action before final capture")?;
        let ui = shell.ui_frame(paint, WINDOW.map(|v|v as f32), true);
        let image = ui.composition_image.ok_or("missing composition image mapping")?;
        let viewport = ui.composition.ok_or("missing composition viewport")?;
        crate::native_test_support::capture_ui(loop_, &shell.engine, ui, evidence.root.join("window.png"))?;
        let pixels = fixture::verify_pixels(fixtures, manifest, count, image, viewport, evidence)?;
        let field = |name:&str| distribution(&records, name);
        let report = json!({"sourceCount":count,"monitor":monitor,"policy":policy,
            "cold":cold,"warmupCount":warmup.len(),"measuredCount":records.len(),
            "playbackWallUs":elapsed_us,"warmupClockUs":WARMUP.as_micros(),"measuredClockUs":MEASURED.as_micros(),
            "requestedSourceInterval":[0,120],"intervalSemantics":"admit production clock seeks before four seconds; first one second warmup, next three seconds measured; complete the final admitted render even if its reply crosses the boundary",
            "cycleWithSeekUs":field("cycleWithSeekUs"),"completionIntervalUs":field("completionIntervalUs"),
            "stateObservationUs":field("stateObservationUs"),"resourceObservationUs":field("resourceObservationUs"),
            "requestedSourceDeadlineMisses":records.iter().filter(|r|r["requestedSourceDeadlineMissed"] == true).count(),
            "skippedSourceBuckets":records.iter().filter_map(|r|r["skippedSourceBuckets"].as_u64()).sum::<u64>(),
            "repeatedSourceBuckets":records.iter().filter(|r|r["repeatedSourceBucket"] == true).count(),
            "nativeProfile":"native-profile.json","actualSources":sources,"pixels":pixels,
            "finalPreview":shell.state["preview"],"native":shell.engine.native_wait_snapshot()});
        write_json(&evidence.root.join("report.json"), &report)?;
        Ok(report)
    })();
    if let Err(error) = &result {
        if !evidence.root.join("failure.json").exists() {
            evidence.failure(Some(&shell.engine), &error.to_string());
        }
    }
    // Failure evidence has already been flushed before the existing Engine Drop
    // waits for its worker. The external study runner bounds this test process.
    result
}
