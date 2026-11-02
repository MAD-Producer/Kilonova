use motion_core::{
    layer_time::*, Camera, Command, Content, Ease, Engine, Layer, LayerTimeline, Project,
};

fn fixture(reverse: bool) -> Engine {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut c = Camera::new(640, 360);
    c.created = true;
    for (f, v) in [(-6, 40.), (-3, 60.), (0, 80.)] {
        c.fov.upsert(f, v, Ease::Linear).unwrap();
    }
    let mut l = Layer::camera(7, "Lens", 640, 360);
    l.content = Content::Camera {
        camera: Box::new(c),
    };
    l.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 120,
        offset_frame: 2,
    });
    l.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -6. },
        )
        .unwrap(),
        remap: None,
    });
    p.layers.push(l);
    Engine::new(p).unwrap()
}
fn edit(frame: f64, film: f32) -> Command {
    Command::CameraLens {
        object: 7,
        frame,
        film_width_mm: film,
        focal_length_mm: 35.,
    }
}

#[test]
fn lens_command_preserves_exact_signed_identity_and_roundtrips_with_one_undo() {
    for reverse in [false, true] {
        let mut e = fixture(reverse);
        let before = e.snapshot();
        e.apply(edit(6.5, 24.)).unwrap();
        let after = e.snapshot();
        let mut expected = before.clone();
        let c = expected.camera_for_mut(7).unwrap();
        c.film_width_mm = 24.;
        c.fov.keys[1].value = motion_core::lens::Lens::from_focal_length(24., 35., 640, 360)
            .unwrap()
            .vertical_fov_degrees as f32;
        assert_eq!(after, expected);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        e.redo().unwrap();
        assert_eq!(e.project(), &after);
        let dir = tempfile::tempdir().unwrap();
        motion_core::storage::save(dir.path(), &after).unwrap();
        assert_eq!(motion_core::storage::load(dir.path()).unwrap(), after);
        // JSON and the public catalogue expose an actual continuous coordinate.
        let parsed=motion_core::parse_commands(r#"{"op":"camera_lens","object":7,"frame":6.5,"film_width_mm":24,"focal_length_mm":35}"#).unwrap();
        e.apply_batch(parsed).unwrap();
        assert_eq!(e.project(), &after);
        let catalogue = motion_core::CommandRegistry::catalogue();
        let schema = catalogue["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "camera_lens")
            .unwrap();
        assert_eq!(
            schema["inputSchema"]["properties"]["frame"]["type"],
            "number"
        );
    }
}
#[test]
fn lens_failures_preserve_film_fov_history_and_the_complete_batch() {
    for reverse in [false, true] {
        for bad in [
            edit(7., 36.),
            edit(f64::NAN, 36.),
            edit(-1., 36.),
            edit(120., 36.),
            edit(6.5, 0.),
        ] {
            let mut e = fixture(reverse);
            let before = e.snapshot();
            assert!(e.apply_batch(vec![edit(6.5, 24.), bad]).is_err());
            assert_eq!(e.project(), &before);
            assert!(!e.undo().unwrap());
        }
        let mut e = fixture(reverse);
        e.apply(Command::Flags {
            object: 7,
            visible: true,
            locked: true,
        })
        .unwrap();
        let before = e.snapshot();
        assert!(e.apply(edit(6.5, 24.)).is_err());
        assert_eq!(e.project(), &before);
        let mut p = before.clone();
        p.layer_mut(7).unwrap().locked = false;
        p.camera_for_mut(7).unwrap().fov.keys.clear();
        let mut e = Engine::new(p.clone()).unwrap();
        e.apply(edit(7., 24.)).unwrap();
        let mut expected = p;
        let c = expected.camera_for_mut(7).unwrap();
        c.film_width_mm = 24.;
        c.fov.value = motion_core::lens::Lens::from_focal_length(24., 35., 640, 360)
            .unwrap()
            .vertical_fov_degrees as f32;
        assert_eq!(e.project(), &expected);
    }
}
