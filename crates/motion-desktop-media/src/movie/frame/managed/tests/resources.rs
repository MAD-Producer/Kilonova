//! Frozen configuration plus LUT handles survive path replacement and Session
//! closure. The reference is OCIO's separate file reader, not our preparer.
use super::*;
#[path = "../../../../../../motion-effects/tests/support/native_resources.rs"]
mod fixture;

#[test]
#[ignore = "requires fresh native Cube file oracle and actual GPU"]
fn frozen_png_retains_lut_bytes_and_matches_independent_file_cpu() {
    let cases = fixture::cases();
    let mut pixels = 0;
    for index in [0, 3] {
        let case = &cases[index];
        let root = tempfile::tempdir().unwrap();
        let config = fixture::config(case);
        let selection = fixture::selection(&config);
        let signal = ColorSignal {
            encoding: "scene-linear".into(),
            transfer: ColorTransfer::Linear,
            reference: ReferenceSpace::Scene,
            primaries: [[0.64, 0.33], [0.3, 0.6], [0.15, 0.06]],
            white_point: [0.3127, 0.329],
            range: SignalRange::Full {},
            alpha: AlphaMode::Straight,
        };
        let mut working = signal.clone();
        working.alpha = AlphaMode::Premultiplied;
        let mut display = signal.clone();
        display.reference = ReferenceSpace::Display;
        let settings = OcioProject {
            manifest: config.manifest().clone(),
            selection: selection.clone(),
            default_input: signal.clone(),
            authored_input: ColorInput {
                space: selection.input,
                signal: signal.clone(),
            },
            working,
            display,
            output: signal,
            sources: vec![],
        };
        let mut p = Project::new(16, 16, 30, 10).unwrap();
        p.working_precision = ChannelPrecision::Float32;
        p.color_management = Arc::new(ProjectColorManagement::Ocio {
            settings: Box::new(settings),
        });
        p.background = [0.08, 0.1, 0.2, 0.25];
        p.layers.push(Layer::solid(
            1,
            "independent full-chain card",
            [16., 16.],
            [8., 8., 0.],
            [0.2, 0.8, 0.4, 0.6],
        ));
        let resources = p.color_management.resources().unwrap();
        for resource in &resources {
            let path = root.path().join(&resource.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                if resource.path.ends_with("/config.ocio") {
                    case.config.as_bytes()
                } else {
                    case.cube.as_bytes()
                },
            )
            .unwrap();
        }
        let session = Session::new(
            p.clone(),
            root.path().into(),
            Arc::new(NativePlatform(runtime())),
        )
        .unwrap();
        let before = session.engine.snapshot();
        let output = root.path().join("output.png");
        let control = ExportControl::default();
        let plan = FrameFreezePlan::capture(&session, Some(output.clone()), &control)
            .unwrap()
            .with_observation_request(8401);
        for (number, resource) in resources.iter().enumerate() {
            let path = root.path().join(&resource.path);
            fs::rename(&path, root.path().join(format!("retained-{number}"))).unwrap();
            fs::write(path, b"invalid replacement source").unwrap();
        }
        assert_eq!(session.engine.snapshot(), before);
        drop(session);
        let result = plan.export(&control).unwrap();
        assert_eq!(
            result["color"]["configKey"],
            serde_json::to_value(config.key()).unwrap()
        );
        let image = image::open(&output).unwrap().to_rgba8();
        let expected = case.composite[0].map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
        for y in 2..14 {
            for x in 2..14 {
                let got = image.get_pixel(x, y).0;
                assert!(
                    (0..4).all(|c| got[c].abs_diff(expected[c]) <= 1),
                    "case={index} ({x},{y}) actual={got:?} file CPU={expected:?}"
                );
                pixels += 1;
            }
        }
    }
    assert_eq!(pixels, 288);
    println!("native file full-chain PNG pixels={pixels}; frozen config and LUT retained after replacement and Session close");
}
