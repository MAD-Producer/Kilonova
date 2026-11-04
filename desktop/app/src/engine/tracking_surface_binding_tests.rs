//! Hold the encoded plate fixed so changing pixels must come from the solve.
use super::*;
use motion_core::tracking::{
    asset::Kind,
    plate::{Calibration, Rotation},
    point::{Interpolation, Missing, PointBinding},
};

#[derive(Clone, Copy, Debug)]
enum BindingCase {
    Point,
    Plane,
    Lens,
}

fn observation_case(data: &mut Asset, case: BindingCase, changed: bool) {
    let track = &mut data.tracks[0];
    track.id = if changed { "new" } else { "old" }.into();
    track.kind = match case {
        BindingCase::Point => Kind::Point,
        BindingCase::Plane => Kind::Plane,
        BindingCase::Lens => Kind::Camera,
    };
    track.reference_index = matches!(case, BindingCase::Plane).then_some(0);
    for sample in &mut track.samples {
        sample.observation = Some(match case {
            BindingCase::Point => {
                Observation::Point(if changed { [144., 66.] } else { [128., 72.] })
            }
            BindingCase::Plane => Observation::Plane(if changed && sample.source_index != 0 {
                [[0.5, 0., 80.], [0., 0.5, 30.], [0., 0., 1.]]
            } else {
                [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
            }),
            BindingCase::Lens => {
                let Some(Observation::Camera {
                    camera_to_world, ..
                }) = sample.observation
                else {
                    panic!("camera fixture must contain a camera observation")
                };
                Observation::Camera {
                    camera_to_world,
                    calibration: Calibration::new(
                        data.source_extent,
                        [180., 160.],
                        [128., 72.],
                        [if changed { -0.04 } else { 0.04 }, 0., 0.],
                        [0.001, -0.001],
                    )
                    .unwrap(),
                }
            }
        });
    }
    data.validate().unwrap();
}

fn marker_oracle(image: &image::RgbaImage, case: BindingCase, changed: bool) -> Value {
    if matches!(case, BindingCase::Lens) {
        assert!(
            image
                .pixels()
                .filter(|p| p[0] > 20 || p[1] > 20 || p[2] > 20)
                .count()
                > 100
        );
        return json!({"kind":"nonempty_lens_output","independentLensAccuracy":false});
    }
    // Only examine the marker region; the native video plate lives at top left.
    let pixels: Vec<_> = image
        .enumerate_pixels()
        .filter_map(|(x, y, p)| {
            (x > 200 && y > 150 && p[1] > 240 && p[0] < 10 && p[2] < 10)
                .then_some([f64::from(x), f64::from(y)])
        })
        .collect();
    let (expected_center, expected_area) = match (case, changed) {
        (BindingCase::Point, false) => ([319.5, 239.5], 64. * 48.),
        // Source displacement (16,-6) maps 256x144 -> 640x480: (40,-20).
        (BindingCase::Point, true) => ([359.5, 219.5], 64. * 48.),
        (BindingCase::Plane, false) => ([319.5, 239.5], 128. * 72.),
        // Plane coordinates map into the 128x72 layer, not composition axes.
        // Half scale about center, followed by source displacement (16,-6).
        (BindingCase::Plane, true) => ([327.5, 236.5], 64. * 36.),
        _ => unreachable!(),
    };
    assert!(
        (pixels.len() as f64 - expected_area).abs() <= expected_area * 0.1,
        "{case:?} changed={changed} marker area={} expected={expected_area}",
        pixels.len()
    );
    let center =
        [0, 1].map(|axis| pixels.iter().map(|p| p[axis]).sum::<f64>() / pixels.len() as f64);
    for axis in 0..2 {
        assert!(
            (center[axis] - expected_center[axis]).abs() < 1.,
            "{case:?} changed={changed} center={center:?} expected={expected_center:?}"
        );
    }
    json!({"center":center,"expectedCenter":expected_center,"area":pixels.len(),"expectedArea":expected_area})
}

fn run_case(
    events: &mut EventLoop<()>,
    window: Arc<winit::window::Window>,
    output: &Path,
    case: BindingCase,
) -> Value {
    fs::create_dir_all(output.join("project/assets")).unwrap();
    let root = output.join("project");
    let source = root.join("assets/original.mp4");
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video/video-ctts.mp4");
    fs::copy(&original, &source).unwrap();
    let mut old_solve = solve(&source, "video-ctts.mp4", 0.);
    observation_case(&mut old_solve, case, false);
    let mut new_solve = solve(&source, "video-ctts.mp4", 0.);
    observation_case(&mut new_solve, case, true);
    assert_eq!(old_solve.source_sha256, new_solve.source_sha256);
    assert_eq!(old_solve.source_pts, new_solve.source_pts);
    let old_document = output.join("old.mstrack");
    let new_document = output.join("new.mstrack");
    fs::write(
        &old_document,
        motion_host::tracking::encode::to_vec(&old_solve).unwrap(),
    )
    .unwrap();
    fs::write(
        &new_document,
        motion_host::tracking::encode::to_vec(&new_solve).unwrap(),
    )
    .unwrap();
    let platform = motion_desktop_media::platform();
    let mut probe = platform
        .probe_video(&source, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/original.mp4".into();
    let mut project = authored::camera_fixture();
    let mut camera = project.camera.tracking.take().unwrap();
    project.tracking_assets.clear();
    project.layers.clear();
    project.video_assets = vec![probe.asset.clone()];
    project.background = [0., 0., 0., 1.];
    let extent = [project.width, project.height];
    let (video_size, video_position) = if matches!(case, BindingCase::Lens) {
        ([256., 144.], [320., 240., 0.])
    } else {
        ([64., 36.], [70., 50., 0.])
    };
    let mut video =
        motion_core::Layer::solid(50, "Native plate", video_size, video_position, [1.; 4]);
    video.content = motion_core::Content::Video {
        video: motion_core::VideoClip::for_source(&probe.asset, false),
    };
    video.timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: 48,
        offset_frame: 0,
    });
    video.three_d = matches!(case, BindingCase::Lens);
    project.layers.push(video);
    if !matches!(case, BindingCase::Lens) {
        let size = if matches!(case, BindingCase::Point) {
            [64., 48.]
        } else {
            [128., 72.]
        };
        let mut marker = motion_core::Layer::solid(
            51,
            "Independently measured marker",
            size,
            [320., 240., 0.],
            [0., 1., 0., 1.],
        );
        if matches!(case, BindingCase::Plane) {
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
                size,
            );
            project.plugin_dependencies.push(effect.dependency());
            marker.effects.push(effect);
        }
        project.layers.push(marker);
    }
    motion_core::storage::save(&root, &project).unwrap();
    let index = motion_media::video_cache_path(&root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let engine = Engine::start(root.clone(), platform, 8 << 30).unwrap();
    let state = engine.state().unwrap();
    let state = engine
        .import_tracking(
            old_document,
            AssetRef {
                kind: AssetKind::Video,
                id: 1,
            },
            state["revision"].as_u64().unwrap(),
            state["document_generation"].as_u64().unwrap(),
        )
        .unwrap();
    let asset = state["project"]["tracking_assets"][0]["id"]
        .as_u64()
        .unwrap();
    let binding = match case {
        BindingCase::Point => json!(PointBinding {
            asset,
            track: "old".into(),
            anchor_local_frame: 0,
            source_anchor_pts: old_solve.source_pts[0],
            speed_numerator: 1,
            speed_denominator: 1,
            reference_pixels: [128., 72.],
            crop: [0., 0., 256., 144.],
            rotation: Rotation::None,
            interpolation: Interpolation::Linear,
            max_gap_ticks: 100000,
            minimum_confidence: 0.8,
            missing: Missing::Error,
        }),
        BindingCase::Plane => json!(motion_core::tracking::plane::PlaneBinding {
            asset,
            track: "old".into(),
            effect: 99,
            anchor_local_frame: 0,
            source_anchor_pts: old_solve.source_pts[0],
            speed_numerator: 1,
            speed_denominator: 1,
            reference_quad: [[0., 0.], [256., 0.], [256., 144.], [0., 144.]],
            crop: [0., 0., 256., 144.],
            rotation: Rotation::None,
            interpolation: Interpolation::Linear,
            max_gap_ticks: 100000,
            minimum_confidence: 0.8,
            missing: Missing::Error,
        }),
        BindingCase::Lens => {
            camera.asset = asset;
            camera.track = "old".into();
            camera.source_anchor_pts = old_solve.source_pts[0];
            camera.crop = [0., 0., 256., 144.];
            json!(camera)
        }
    };
    let (kind, object) = match case {
        BindingCase::Point => ("point", 51),
        BindingCase::Plane => ("plane", 51),
        BindingCase::Lens => ("camera", 0),
    };
    engine
        .edit_at_revision(
            json!({"op":format!("set_tracking_{kind}"),"object":object,"binding":binding})
                .to_string(),
            state["revision"].as_u64().unwrap(),
        )
        .unwrap();
    engine.preview_mode(1, 0).unwrap();
    engine.seek(24.).unwrap();
    let attach = engine
        .attach_async(window.clone(), extent[0], extent[1])
        .unwrap();
    crate::native_test_support::receive(events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let before = invalidated(events, &engine, extent);
    let warm = front(events, &engine, extent, &output.join("before.png"));
    let old_oracle = marker_oracle(&warm, case, false);
    retained(events, &engine, extent);
    assert_eq!(
        front(events, &engine, extent, &output.join("before-retained.png")),
        warm
    );
    prepare(&engine, "surface-binding-pair", &original);
    let mut next_binding = binding;
    next_binding["track"] = json!("new");
    media(&engine,json!({"op":"finish_tracked_video_replacement","request_id":"surface-binding-pair",
        "target":{"kind":"video","id":1},"revision":before["revision"],"document_generation":before["document_generation"],
        "solves":[{"asset":asset,"path":new_document}],
        "bindings":[{"kind":kind,"composition":project.composition_id,"object":object,"binding":next_binding}]})).unwrap();
    let after = invalidated(events, &engine, extent);
    assert_eq!(
        after["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    assert_eq!(
        after["project"]["layers"],
        before["project"]["layers"]
            .as_array()
            .map(|layers| {
                let mut layers = layers.clone();
                if kind != "camera" {
                    layers[1][format!("tracking_{kind}")]["track"] = json!("new");
                }
                layers
            })
            .map(Value::Array)
            .unwrap()
    );
    let changed = front(events, &engine, extent, &output.join("after.png"));
    let new_oracle = marker_oracle(&changed, case, true);
    assert_ne!(
        changed, warm,
        "{case:?} solve-only change must alter actual front"
    );
    retained(events, &engine, extent);
    assert_eq!(
        front(events, &engine, extent, &output.join("after-retained.png")),
        changed
    );
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
    invalidated(events, &engine, extent);
    assert_eq!(
        front(events, &engine, extent, &output.join("undo.png")),
        warm
    );
    retained(events, &engine, extent);
    assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
    invalidated(events, &engine, extent);
    assert_eq!(
        front(events, &engine, extent, &output.join("redo.png")),
        changed
    );
    retained(events, &engine, extent);
    engine.save().unwrap();
    drop(engine);
    let reopened = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    assert_eq!(reopened.state().unwrap()["project"], after["project"]);
    reopened.preview_mode(1, 0).unwrap();
    reopened.seek(24.).unwrap();
    let attach = reopened.attach_async(window, extent[0], extent[1]).unwrap();
    crate::native_test_support::receive(events, &attach, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    invalidated(events, &reopened, extent);
    assert_eq!(
        front(events, &reopened, extent, &output.join("reopened.png")),
        changed
    );
    retained(events, &reopened, extent);
    let report = json!({"case":format!("{case:?}"),"sameEncodedPlate":true,"sameSourcePts":true,
        "beforeOracle":old_oracle,"afterOracle":new_oracle,
        "before":before["desktop"]["uiPerformance"],"after":after["desktop"]["uiPerformance"],
        "reopened":reopened.state().unwrap()["desktop"]["uiPerformance"],
        "oneRevision":true,"undoRedoReopenExactPixels":true,"normalPresentReuses":true});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    drop(reopened);
    report
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn point_plane_and_nonlinear_lens_replacement_invalidate_actual_surface_with_identical_plate() {
    const NAME: &str = "engine::tracking_video_replacement_tests::paired::surface::bindings::point_plane_and_nonlinear_lens_replacement_invalidate_actual_surface_with_identical_plate";
    const CHILD: &str = "MOTION_TRACKED_BINDINGS_SURFACE_CHILD";
    let Some(directory) = std::env::var_os(CHILD) else {
        let temporary = tempfile::tempdir().unwrap();
        let directory = std::env::var_os("MOTION_TRACKED_BINDINGS_SURFACE_ARTIFACTS")
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
        assert!(log.contains("tracked video replacement start request=surface-binding-pair"));
        assert!(!log
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        return;
    };
    let output = PathBuf::from(directory);
    let _logger = crate::logging::init(output.join("logs")).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(640, 480)),
            )
            .unwrap(),
    );
    let mut reports = Vec::new();
    for case in [BindingCase::Point, BindingCase::Plane, BindingCase::Lens] {
        reports.push(run_case(
            &mut events,
            window.clone(),
            &output.join(format!("{case:?}")),
            case,
        ));
    }
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
    crate::logging::flush();
}
