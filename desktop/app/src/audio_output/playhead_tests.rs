//! Exercise the public audio clock and its actual Shell consumer without a
//! device or sleeps. PCM still passes through the production callback and ring.
use super::*;

fn output(rate: u32, start_sample: u64, total_samples: u64) -> AudioOutput {
    let mut shared = Shared::new();
    shared.test_now_ns = Some(AtomicU64::new(0));
    shared.ready.store(true, Ordering::Release);
    shared.begin_playback_at(0);
    AudioOutput {
        stream: None,
        producer: None,
        monitor: None,
        shared: Arc::new(shared),
        rate,
        start_sample,
        total_samples,
        range_start: 0,
        looping: false,
    }
}

fn at(output: &AudioOutput, now: u64) {
    output
        .shared
        .test_now_ns
        .as_ref()
        .unwrap()
        .store(now, Ordering::Relaxed);
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-10,
        "actual={actual} expected={expected}"
    );
}

#[test]
fn audio_only_preview_uses_callback_clock_without_canvas_gates_and_scrub_stops_it() {
    let root = tempfile::tempdir().unwrap();
    let mut shell = crate::Shell::new(
        root.path().join("project"),
        None,
        Some(crate::i18n::Locale::En),
    )
    .unwrap();
    shell.home = None;
    let before = shell.engine.state().unwrap();
    let audio = output(48_000, 0, 480_000);
    for _ in 0..4_800 {
        assert!(audio.shared.ring.push([0.25, -0.5]));
    }
    let mut consumed = 0;
    let mut pcm = vec![0.; 9_600];
    render_callback(&audio.shared, &mut pcm, 2, &mut consumed, 0, 0);
    at(&audio, 100_000_000);
    assert_eq!(&pcm[..2], &[0.25, -0.5]);
    shell.audio = Some(audio);
    shell.playback = Some((Instant::now(), 0.));
    shell.audio_only_preview = true;
    shell.panels.playing = true;
    shell.attached = false;
    shell.present_due = true;
    shell.exposure_presentations.insert(9);
    assert!(
        shell.playback_ready(Instant::now()),
        "audio must not wait for a canvas presentation"
    );
    let ui = shell.ui_frame(crate::PaintList::default(), [1600., 1000.], false);
    assert!(
        ui.composition.is_some(),
        "frozen presentation still needs the canvas mapping"
    );
    shell.advance_playback().unwrap();
    assert_eq!(shell.frame, 3.);
    assert!(
        !shell.playback_ready(Instant::now()),
        "host seeks must remain serial"
    );
    let (_, reply) = shell.editing.take().unwrap();
    shell.accept_edit_state(
        reply
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
    );
    assert_eq!(shell.state["frame"], 3.);
    shell
        .action(crate::panels::Action::Seek(12.), None)
        .unwrap();
    assert!(shell.audio.is_none() && shell.playback.is_none() && !shell.audio_only_preview);
    let (_, reply) = shell.editing.take().unwrap();
    shell.accept_edit_state(
        reply
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
    );
    assert_eq!(shell.state["frame"], 12.);
    for key in ["project", "revision", "canUndo", "canRedo"] {
        assert_eq!(shell.state[key], before[key], "{key}");
    }
    assert!(shell
        .ui_frame(crate::PaintList::default(), [1600., 1000.], false)
        .composition
        .is_some());
}

#[test]
fn audio_only_preview_output_failure_stops_with_original_cause_in_default_channels() {
    const CHILD: &str = "MOTION_AUDIO_ONLY_FAILURE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut shell =
            crate::Shell::new(root.join("project"), None, Some(crate::i18n::Locale::En)).unwrap();
        let mut audio = output(48_000, 0, 480_000);
        let shared = audio.shared.clone();
        let missing = root.join("missing-audio.pcm");
        audio.producer = Some(
            spawn_producer(shared.clone(), move || {
                let bytes = std::fs::read(&missing)
                    .map_err(|error| format!("read {}: {error}", missing.display()))?;
                Ok(bytes.first().map(|_| [0.; 2]))
            })
            .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !shared.stop.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(shared.stop.load(Ordering::Acquire));
        audio.producer.take().unwrap().join().unwrap();
        shell.audio = Some(audio);
        shell.playback = Some((Instant::now(), 0.));
        shell.audio_only_preview = true;
        shell.panels.playing = true;
        shell.advance_playback().unwrap();
        assert!(shell.playback.is_none() && shell.audio.is_none() && !shell.audio_only_preview);
        let message = shell.panels.message.as_ref().unwrap();
        assert!(
            message.contains("missing-audio.pcm") && message.contains("os error 2"),
            "{message}"
        );
        let text = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(
            text.contains("ERROR")
                && text.contains("missing-audio.pcm")
                && text.contains("os error 2")
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "audio_output::playhead_tests::audio_only_preview_output_failure_stops_with_original_cause_in_default_channels", "--nocapture"])
        .env_remove("MOTION_LOG").env(CHILD, root.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ERROR") && stderr.contains("missing-audio.pcm"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("missing-audio.pcm"));
    if let Some(destination) = std::env::var_os("MOTION_AUDIO_PREVIEW_ERROR_EVIDENCE") {
        let destination = std::path::PathBuf::from(destination);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::copy(root.path().join("logs/motion-studio.log"), destination.join("default-file.log")).unwrap();
        std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
    }
}

#[test]
fn public_clock_rescales_device_samples_and_retains_latency_underrun_and_end_boundaries() {
    for rate in [44_100, 96_000] {
        for fps in [24, 30, 60] {
            let output = output(rate, 48_000, 51_600);
            let shared = &output.shared;
            close(output.frame(fps), f64::from(fps));
            // Submit 50 ms, played from 20 ms after the callback timestamp.
            for _ in 0..rate / 20 {
                assert!(shared.ring.push([0.25, -0.5]));
            }
            let mut consumed = 0;
            let mut pcm = vec![0.; (rate / 20) as usize * 2];
            render_callback(
                shared,
                &mut pcm,
                2,
                &mut consumed,
                1_000_000_000,
                20_000_000,
            );
            assert!(pcm.chunks_exact(2).all(|p| p == [0.25, -0.5]));
            at(&output, 1_010_000_000);
            close(output.frame(fps), f64::from(fps));
            // 30 ms is an integral number of samples at both device rates.
            at(&output, 1_050_000_000);
            close(output.frame(fps), 1.03 * f64::from(fps));
            at(&output, 1_100_000_000);
            close(output.frame(fps), 1.05 * f64::from(fps));

            // A real empty callback outputs silence, retaining the final sample.
            render_callback(
                shared,
                &mut pcm,
                2,
                &mut consumed,
                1_100_000_000,
                20_000_000,
            );
            assert!(pcm.iter().all(|&p| p == 0.));
            assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
            at(&output, 1_150_000_000);
            close(output.frame(fps), 1.05 * f64::from(fps));

            for _ in 0..rate / 20 {
                assert!(shared.ring.push([0.5, -0.25]));
            }
            render_callback(
                shared,
                &mut pcm,
                2,
                &mut consumed,
                1_160_000_000,
                20_000_000,
            );
            at(&output, 1_160_000_000);
            close(output.frame(fps), 1.05 * f64::from(fps));
            at(&output, 1_210_000_000);
            close(output.frame(fps), 1.075 * f64::from(fps));
            assert!(output.error().is_none());
        }
    }
}

#[test]
fn shell_uses_audio_position_for_seek_and_stops_at_the_final_composition_frame() {
    let directory = tempfile::tempdir().unwrap();
    let mut shell = crate::Shell::new(
        directory.path().join("project"),
        None,
        Some(crate::Locale::En),
    )
    .unwrap();
    let fps = shell.state["project"]["fps"].as_u64().unwrap() as u32;
    let frames = shell.state["project"]["frames"].as_u64().unwrap();
    let total_samples = frames * 48_000 / u64::from(fps);
    let output = output(44_100, total_samples - 4_800, total_samples);
    let shared = output.shared.clone();
    for _ in 0..4_410 {
        assert!(shared.ring.push([0.25, -0.5]));
    }
    let mut consumed = 0;
    let mut pcm = vec![0.; 4_410 * 2];
    render_callback(
        &shared,
        &mut pcm,
        2,
        &mut consumed,
        1_000_000_000,
        20_000_000,
    );
    at(&output, 1_070_000_000);
    shell.audio = Some(output);
    shell.playback = Some((Instant::now(), 0.));
    shell.advance_playback().unwrap();
    let expected = frames as f64 - 0.05 * f64::from(fps);
    close(shell.frame, expected);
    assert!(shell.playback.is_some());
    let (_, reply) = shell.editing.take().unwrap();
    let state = reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    close(state["frame"].as_f64().unwrap(), expected);
    assert!(shell.panels.message.is_none());

    at(shell.audio.as_ref().unwrap(), 1_120_000_000);
    shell.advance_playback().unwrap();
    assert!(shell.audio.is_none());
    assert!(shell.playback.is_none());
    close(shell.frame, (frames - 1) as f64);
    let (_, reply) = shell.editing.take().unwrap();
    let state = reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    close(state["frame"].as_f64().unwrap(), (frames - 1) as f64);
    assert!(shell.panels.message.is_none());
    assert!(shared.stop.load(Ordering::Acquire));
}
