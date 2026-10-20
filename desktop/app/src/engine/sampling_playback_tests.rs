//! Paced native presentation costs. This driver uses production playback and
//! reply barriers, but does not measure OS event dispatch or physical input.
use super::*;
use crate::{panels, workspace, Locale, Shell};
use motion_core::{render_sampling::Shutter, vector::*, Content, Ease, Layer, Project};

fn drive_frame(shell: &mut Shell) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !shell.playback_ready(Instant::now()) {
        assert!(Instant::now() < deadline, "playback admission stalled");
        std::thread::sleep(Duration::from_millis(1));
    }
    let began = Instant::now();
    shell.advance_playback().unwrap();
    assert!(
        !shell.playback_ready(Instant::now()),
        "seek must block the next tick"
    );
    let (_, reply) = shell.editing.take().unwrap();
    shell.accept_edit_state(
        reply
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap(),
    );
    let seek_us = began.elapsed().as_micros();
    let expected = shell.frame;
    let mut polls = 0;
    let mut ui_us = 0;
    let mut present_us = 0;
    loop {
        let build = Instant::now();
        let (paint, actions) = shell.draw_commands([1280., 800.]);
        assert!(actions.is_empty());
        let ui = shell.ui_frame(paint, [1280., 800.], false);
        assert!(ui.composition.is_some());
        ui_us += build.elapsed().as_micros();
        shell.track_exposure_present(0, true);
        let present = Instant::now();
        let result = shell
            .engine
            .present_async(ui)
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        present_us += present.elapsed().as_micros();
        shell.finish_exposure_present(0, &result);
        shell.present_due = false;
        shell.pending = !result.unwrap();
        polls += 1;
        if !shell.pending {
            break;
        }
        assert!(
            !shell.playback_ready(Instant::now()),
            "unfinished exposure must hold the playback clock"
        );
        assert!(
            Instant::now() < deadline,
            "native composition never became ready"
        );
    }
    let render_us = began.elapsed().as_micros();
    let state = shell.engine.state().unwrap();
    assert_eq!(state["lastPresentedFrame"], expected);
    assert!(shell.exposure_presentations.is_empty());
    serde_json::json!({"frame":expected,"seek_us":seek_us,"ui_build_us":ui_us,
        "present_reply_us":present_us,"render_cycle_us":render_us,"present_polls":polls})
}

#[test]
fn paced_sampling_playback_records_native_costs_without_starving_frames() {
    const CHILD: &str = "MOTION_SAMPLING_PLAYBACK_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        let mut reports = Vec::new();
        for (name, blur, raster) in [
            ("baseline", false, false),
            ("blur", true, false),
            ("raster", false, true),
            ("combined", true, true),
        ] {
            let mut project = Project::new(640, 360, 30, 3000).unwrap();
            project.version = 21;
            project.background = [0., 0., 0., 1.];
            for id in 1..=4 {
                let mut layer = Layer::solid(
                    id,
                    "Sampling playback study",
                    [24., 24.],
                    [80., 72. * id as f32, 0.],
                    [1.; 4],
                );
                layer.content = Content::Vector {
                    vector: VectorContent::shape(ShapeKind::Ellipse),
                };
                layer.transform.scale.value = [600., 600., 100.];
                layer
                    .transform
                    .position
                    .upsert(0, [80., 72. * id as f32, 0.], Ease::Linear)
                    .unwrap();
                layer
                    .transform
                    .position
                    .upsert(300, [560., 72. * id as f32, 0.], Ease::Linear)
                    .unwrap();
                layer.sampling.continuous_rasterization = raster;
                layer.sampling.motion_blur = blur.then_some(Shutter {
                    frames: 0.5,
                    samples: 8,
                });
                project.layers.push(layer);
            }
            let directory = root.join(name);
            motion_core::storage::save(&directory, &project).unwrap();
            let mut shell = Shell::new(directory, None, Some(Locale::En)).unwrap();
            shell.workspace = workspace::Workspace::default();
            shell.home = None;
            #[allow(deprecated)]
            let window = Arc::new(
                event_loop
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800)),
                    )
                    .unwrap(),
            );
            shell.state = shell
                .engine
                .attach_async(window.clone(), 1280, 800)
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap();
            shell.window = Some(window);
            shell.attached = true;
            let before = shell.state["project"].clone();
            shell.action(panels::Action::TogglePlay, None).unwrap();
            let mut warmup = Vec::new();
            for _ in 0..5 {
                warmup.push(drive_frame(&mut shell));
            }
            let began = Instant::now();
            let mut previous = began;
            let mut records = Vec::new();
            for _ in 0..60 {
                let mut record = drive_frame(&mut shell);
                let now = Instant::now();
                record["completion_interval_us"] =
                    serde_json::json!(now.duration_since(previous).as_micros());
                previous = now;
                records.push(record);
            }
            let wall_us = began.elapsed().as_micros();
            shell.action(panels::Action::TogglePlay, None).unwrap();
            assert!(shell.playback.is_none());
            let state = shell.engine.state().unwrap();
            assert_eq!(state["project"], before);
            assert!(!shell.engine.failed());
            for pair in records.windows(2) {
                assert!(pair[1]["frame"].as_f64().unwrap() > pair[0]["frame"].as_f64().unwrap());
            }
            let (paint, actions) = shell.draw_commands([1280., 800.]);
            assert!(actions.is_empty());
            let ui = shell.ui_frame(paint, [1280., 800.], true);
            shell
                .engine
                .capture_ui(ui, root.join(format!("{name}.png")))
                .unwrap();
            reports.push(
                serde_json::json!({"mode":name,"warmup":warmup,"frames":records,
                "wall_us":wall_us,"completed_frames_per_second":60_000_000. / wall_us as f64,
                "preview":state["preview"],"composition":[640,360],"window":[1280,800]}),
            );
        }
        std::fs::write(root.join("costs.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "debug_assertions":cfg!(debug_assertions),"scope":"Paced production Shell playback, UI build, native surface presentation and one state observation per completed frame; excludes OS event dispatch, physical input, audio and video decoding. Five warmup plus sixty completions per mode; no per-frame CPU pixel readback. Cost sample, not a cross-device FPS guarantee.",
            "runs":reports
        })).unwrap()).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","engine::sampling_playback_tests::paced_sampling_playback_records_native_costs_without_starving_frames"])
        .env(CHILD,root.path()).env("MOTION_UI_LAYOUT",root.path().join("layout.json"))
        .env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(directory) = std::env::var_os("MOTION_SAMPLING_PLAYBACK_ARTIFACTS") {
        let directory = PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        for name in [
            "costs.json",
            "baseline.png",
            "blur.png",
            "raster.png",
            "combined.png",
            "motion-studio.log",
        ] {
            std::fs::copy(root.path().join(name), directory.join(name)).unwrap();
        }
        std::fs::write(directory.join("stderr.log"), output.stderr).unwrap();
    }
}
