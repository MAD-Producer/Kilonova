use super::*;
use motion_core::{
    Camera, Command, Composition, CompositionClip, Content, Layer, LayerTimeline, Property,
};

fn layers(frames: i32, color: [f32; 4], x: f32) -> Vec<Layer> {
    let mut near = Layer::camera(1, "Near camera", 64, 64);
    let Content::Camera { camera } = &mut near.content else {
        unreachable!()
    };
    camera.position.value = [32., 32., -80.];
    let mut far = Layer::camera(2, "Far camera", 64, 64);
    let Content::Camera { camera } = &mut far.content else {
        unreachable!()
    };
    camera.position.value = [32., 32., -140.];
    far.timeline = Some(LayerTimeline {
        in_frame: frames / 2,
        out_frame: frames,
        offset_frame: 0,
    });
    let mut card = Layer::solid(3, "Spatial card", [18., 28.], [x, 32., 0.], color);
    card.three_d = true;
    vec![near, far, card]
}

fn fixture() -> Project {
    let mut project = Project::new(64, 64, 30, 6).unwrap();
    project.version = motion_core::CURRENT_PROJECT_VERSION;
    project.background = [0., 0., 0., 1.];
    project.layers = layers(6, [1., 0., 0., 1.], 49.);
    project.compositions.push(Composition {
        id: "comp-camera-child".into(),
        name: "Child".into(),
        width: 64,
        height: 64,
        fps: 60,
        frames: 12,
        background: [0.; 4],
        camera: Camera::new(64, 64),
        layers: layers(12, [0., 1., 0., 1.], 19.),
        expressions: vec![],
    });
    let mut reference = Layer::solid(4, "Child", [64.; 2], [32., 32., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-camera-child".into()),
    };
    project.layers.insert(2, reference);
    project.validate().unwrap();
    project
}

fn decoded(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let probe = crate::av::probe(path, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.asset.frame_count, 6);
    let last_pts = *probe.timestamps.last().unwrap();
    let first = probe.first_rgba;
    let mut decoder = crate::av::Decoder::open(path, probe.asset, probe.timestamps).unwrap();
    let last = decoder
        .frame(last_pts, &|| Ok(()))
        .unwrap()
        .rgba()
        .unwrap()
        .into_owned();
    (first, last)
}

#[test]
fn frozen_nested_camera_export_survives_live_camera_edits_and_reference_removal() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    fs::create_dir(&root).unwrap();
    let project = fixture();
    let mut session = Session::new(project.clone(), root, crate::platform()).unwrap();
    let frozen = FrozenMovie::from_session(&session).unwrap();
    let control = FrozenMovie::from_session(&session).unwrap();
    for object in [1, 2] {
        session
            .engine
            .apply(Command::SetVector {
                object,
                property: Property::Position,
                frame: 0,
                value: [32., 32., -400.],
            })
            .unwrap();
    }
    session
        .engine
        .apply(Command::Remove {
            object: 4,
            frame: 0,
        })
        .unwrap();
    let changed = FrozenMovie::from_session(&session).unwrap();
    assert_eq!(frozen.project, project);
    assert_eq!(control.project, project);
    let live_before = session.engine.snapshot();
    let actual = directory.path().join("frozen.mp4");
    let expected = directory.path().join("control.mp4");
    let live = directory.path().join("changed.mp4");
    for (movie, path) in [(frozen, &actual), (control, &expected), (changed, &live)] {
        assert!(
            movie
                .export(path, &ExportControl::default(), |_| {})
                .unwrap()
                .bytes
                > 0
        );
    }
    let pixels = decoded(&actual);
    assert_eq!(
        pixels,
        decoded(&expected),
        "frozen export followed live edits"
    );
    assert_ne!(
        pixels,
        decoded(&live),
        "fixture must distinguish the live document"
    );
    assert_ne!(
        pixels.0, pixels.1,
        "camera range switches must change actual decoded frames"
    );
    for frame in [&pixels.0, &pixels.1] {
        assert!(
            frame
                .chunks_exact(4)
                .any(|p| p[1] > 180 && p[0] < 80 && p[2] < 80),
            "child green card missing"
        );
        assert!(
            frame
                .chunks_exact(4)
                .any(|p| p[0] > 180 && p[1] < 80 && p[2] < 80),
            "root red card missing"
        );
    }
    assert_eq!(session.engine.project(), &live_before);
}
