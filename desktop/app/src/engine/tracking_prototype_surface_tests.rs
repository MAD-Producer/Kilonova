//! Frozen actual optical-flow output through native decoding and a real Surface.
use super::*;
use motion_core::tracking::{
    plane::PlaneBinding,
    plate::Rotation,
    point::{Interpolation, Missing, PointBinding},
};

#[path = "tracking_prototype_surface_oracle.rs"]
mod oracle;

fn project(root: &Path, fixture: &Path) {
    fs::create_dir_all(root.join("assets")).unwrap();
    let source = root.join("assets/plate.webm");
    fs::copy(fixture.join("motion.webm"), &source).unwrap();
    let mut probe = motion_desktop_media::platform()
        .probe_video(&source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/plate.webm".into();
    let mut project = motion_core::Project::new(256, 192, 125, 500).unwrap();
    let mut video =
        motion_core::Layer::solid(600, "Native plate", [256., 192.], [128., 96., 0.], [1.; 4]);
    video.content = motion_core::Content::Video {
        video: motion_core::VideoClip::for_source(&probe.asset, false),
    };
    video.timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: 500,
        offset_frame: 0,
    });
    let mut plane = motion_core::Layer::solid(
        700,
        "Measured plane",
        [128., 104.],
        [96., 76., 0.],
        [0., 1., 0., 1.],
    );
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "corner_pin")
        .unwrap();
    let effect = motion_core::EffectInstance::new(
        99,
        motion_effects::builtin::PLUGIN_ID,
        &package.manifest.version,
        &package.hash,
        definition,
        plane.size,
    );
    project.plugin_dependencies.push(effect.dependency());
    plane.effects.push(effect);
    project.layers = vec![
        video,
        plane,
        motion_core::Layer::solid(
            701,
            "Measured point",
            [7., 7.],
            [80., 70., 0.],
            [1., 0., 0., 1.],
        ),
    ];
    project.video_assets.push(probe.asset.clone());
    motion_core::storage::save(root, &project).unwrap();
    let index = motion_media::video_cache_path(root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
}

fn bindings(asset: u64, source_anchor_pts: i64) -> String {
    json!([
        {"op":"set_tracking_plane","object":700,"binding":PlaneBinding {
            asset, track:"plane".into(), effect:99, anchor_local_frame:0, source_anchor_pts,
            speed_numerator:1, speed_denominator:1,
            reference_quad:[[32.,24.],[160.,24.],[160.,128.],[32.,128.]],
            crop:[32.,24.,128.,104.], rotation:Rotation::None,
            interpolation:Interpolation::Linear, max_gap_ticks:50,
            minimum_confidence:0., missing:Missing::Error,
        }},
        {"op":"set_tracking_point","object":701,"binding":PointBinding {
            asset, track:"feature".into(), anchor_local_frame:0, source_anchor_pts,
            speed_numerator:1, speed_denominator:1, reference_pixels:[80.,70.],
            crop:[0.,0.,256.,192.], rotation:Rotation::None,
            interpolation:Interpolation::Linear, max_gap_ticks:50,
            minimum_confidence:0., missing:Missing::Error,
        }},
    ])
    .to_string()
}

fn frame(data: &Asset, index: usize) -> f64 {
    // The frozen original has millisecond PTS. 125 fps maps them exactly to
    // eighth frames; no epsilon or nominal-rate substitution is needed.
    assert_eq!(data.ticks_per_second, 1000);
    (data.source_pts[index] - data.source_pts[0]) as f64 / 8.
}

fn rendered(events: &mut EventLoop<()>, engine: &Engine, frame: f64) -> Value {
    let before = engine.state().unwrap();
    let after = presented(events, engine, [256, 192]);
    assert!(
        after["desktop"]["uiPerformance"]["compositionRenders"]
            .as_u64()
            .unwrap()
            > before["desktop"]["uiPerformance"]["compositionRenders"]
                .as_u64()
                .unwrap()
    );
    assert_eq!(after["lastPresentedFrame"], frame);
    assert!(
        after["renderError"].is_null(),
        "successful Surface has a current error: {after}"
    );
    after
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_prototype_observations_present_retain_refuse_missing_and_recover() {
    const NAME: &str = "engine::tracking_video_replacement_tests::paired::surface::prototype::frozen_prototype_observations_present_retain_refuse_missing_and_recover";
    const CHILD: &str = "MOTION_TRACKED_PROTOTYPE_SURFACE_CHILD";
    let Some(directory) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let directory = std::env::var_os("MOTION_TRACKED_PROTOTYPE_SURFACE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temporary.path().to_path_buf());
        fs::create_dir_all(&directory).unwrap();
        let (output, process) = crate::test_process::capture(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
                .env(CHILD, &directory)
                .env_remove("MOTION_LOG"),
            &directory.join("child-process.json"),
        )
        .unwrap();
        fs::write(directory.join("stdout.log"), &output.stdout).unwrap();
        fs::write(directory.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "child={process}\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = fs::read_to_string(directory.join("logs/motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        assert!(log
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" import_tracking")));
        let cause = "plane observation unavailable source_index=22 visibility=Lost";
        assert!(log.contains(cause));
        assert!(String::from_utf8_lossy(&output.stderr).contains(cause));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(cause));
        assert!(!log
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        return;
    };
    let output = PathBuf::from(directory);
    let _logger = crate::logging::init(output.join("logs")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tracking-prototype");
    let original = fs::read(fixture.join("tracks.mstrack")).unwrap();
    let data = motion_host::tracking::decode::from_slice(&original).unwrap();
    assert_eq!(data.source_pts.len(), 96);
    let root = output.join("project");
    project(&root, &fixture);
    let engine = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let initial = engine.state().unwrap();
    let imported = engine
        .import_tracking(
            fixture.join("tracks.mstrack"),
            AssetRef {
                kind: AssetKind::Video,
                id: 1,
            },
            initial["revision"].as_u64().unwrap(),
            initial["document_generation"].as_u64().unwrap(),
        )
        .unwrap();
    let descriptor = &imported["project"]["tracking_assets"][0];
    let asset = descriptor["id"].as_u64().unwrap();
    let managed = root.join(descriptor["path"].as_str().unwrap());
    assert_eq!(fs::read(&managed).unwrap(), original);
    let bound = engine
        .edit_at_revision(
            bindings(asset, data.source_pts[0]),
            imported["revision"].as_u64().unwrap(),
        )
        .unwrap();
    engine.preview_mode(1, 0).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(256, 192)),
            )
            .unwrap(),
    );
    let attach = engine.attach_async(window.clone(), 256, 192).unwrap();
    crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mut rows = Vec::new();
    let mut recovery = Value::Null;
    let mut saved_image = None;
    for index in [0, 10, 19, 30, 40, 60, 95] {
        engine.seek(frame(&data, index)).unwrap();
        let after = rendered(&mut events, &engine, frame(&data, index));
        let image = front(
            &mut events,
            &engine,
            [256, 192],
            &output.join(format!("measured-{index}.png")),
        );
        rows.push(oracle::pixels(&image, &data, index));
        retained(&mut events, &engine, [256, 192]);
        assert_eq!(
            front(
                &mut events,
                &engine,
                [256, 192],
                &output.join(format!("retained-{index}.png"))
            ),
            image
        );
        if index == 40 {
            saved_image = Some(image.clone());
        }
        if index == 19 {
            let cause = engine.seek(frame(&data, 22)).unwrap_err();
            assert!(cause.contains(&format!(
                "tracking_plane phase=sample object=700 asset={asset} track=plane"
            )));
            assert!(cause.contains("plane observation unavailable source_index=22 visibility=Lost"));
            let reply = engine.present_async(ui([256, 192])).unwrap();
            let refusal =
                crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(30))
                    .unwrap()
                    .unwrap_err();
            assert!(
                refusal.contains("plane observation unavailable source_index=22 visibility=Lost")
            );
            let failed = engine.state().unwrap();
            assert_eq!(failed["lastPresentedFrame"], after["lastPresentedFrame"]);
            assert_eq!(
                front(
                    &mut events,
                    &engine,
                    [256, 192],
                    &output.join("missing-retained.png")
                ),
                image
            );
            let resumed = engine.seek(frame(&data, 30)).unwrap();
            assert!(resumed["renderError"]
                .as_str()
                .unwrap()
                .contains("source_index=22"));
            recovery = json!({"cause":cause,"presentRefusal":refusal,
                "frontPreserved":true,"lastPresentedFrame":failed["lastPresentedFrame"]});
        }
    }
    engine.seek(frame(&data, 40)).unwrap();
    rendered(&mut events, &engine, frame(&data, 40));
    assert_eq!(engine.history(0).unwrap()["project"], imported["project"]);
    rendered(&mut events, &engine, frame(&data, 40));
    let unbound = front(&mut events, &engine, [256, 192], &output.join("undo.png"));
    assert_ne!(unbound, saved_image.clone().unwrap());
    retained(&mut events, &engine, [256, 192]);
    assert_eq!(engine.history(1).unwrap()["project"], bound["project"]);
    rendered(&mut events, &engine, frame(&data, 40));
    assert_eq!(
        front(&mut events, &engine, [256, 192], &output.join("redo.png")),
        saved_image.clone().unwrap()
    );
    retained(&mut events, &engine, [256, 192]);
    engine.save().unwrap();
    drop(engine);
    let reopened = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], bound["project"]);
    reopened.preview_mode(1, 0).unwrap();
    reopened.seek(frame(&data, 40)).unwrap();
    let attach = reopened.attach_async(window, 256, 192).unwrap();
    crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    rendered(&mut events, &reopened, frame(&data, 40));
    assert_eq!(
        front(
            &mut events,
            &reopened,
            [256, 192],
            &output.join("reopened.png")
        ),
        saved_image.unwrap()
    );
    retained(&mut events, &reopened, [256, 192]);
    assert_eq!(fs::read(managed).unwrap(), original);
    fs::write(output.join("report.json"), serde_json::to_vec_pretty(&json!({
        "samples":rows,"missing":recovery,"historyAndReopenExact":true,
        "scope":"Frozen synthetic measured observations through actual Windows Surface; real photographed quality remains unaccepted",
        "sourceSha256":motion_core::storage::resource_sha256(&fs::read(fixture.join("motion.webm")).unwrap()),
        "solveSha256":motion_core::storage::resource_sha256(&original),
    })).unwrap()).unwrap();
    drop(reopened);
    crate::logging::flush();
}
