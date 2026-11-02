use super::*;

#[test]
fn camera_bound_failures_are_bounded_on_default_channels() {
    crate::panels::property_authoring::spatial_tests::faults::verify("panels::camera_controls::mapped_tests::stable::camera_bound_failures_are_bounded_on_default_channels", "camera", &["camera.spatial_numeric_begin","camera.spatial_numeric_continue","camera.spatial_numeric_command"], || {
        let mut h=Harness::new(false,Locale::Zh,false);
        h.run(vec![Action::Edit(Command::SpatialKeyframes {target:motion_core::KeyframeTarget::Property {object:7,property:Property::Position,axis:None},edit:motion_core::SpatialKeyEdit::EnableLocal {frames:vec![-3]}})]);
        let healthy=h.state.clone();h.input(Input::default());let point=center(h.rect(Property::Position,0));
        for _ in 0..180 {
            h.panels.camera_controls.cancel();h.state=healthy.clone();
            h.state["project"]["layers"][0]["content"]["camera"]["position"]["spatial_animation"]["keys"]=json!("broken");
            assert!(!h.draw(pointer(point,0)).iter().any(|a|matches!(a,Action::Edit(_))));
            h.state=healthy.clone();h.input(pointer(point,0));
            h.state["project"]["layers"][0]["content"]["camera"]["position"]["spatial_animation"]["keys"]=json!("broken");
            let out=h.draw(Input::default());assert!(!out.iter().any(|a|matches!(a,Action::Edit(_))));h.run(out);
            h.state=healthy.clone();h.input(pointer(point,0));
            h.state["project"]["layers"][0]["content"]["camera"]["position"]["spatial_animation"]["keys"][1]["value"][1]=Value::Null;
            let out=h.draw(pointer([point[0]+20.,point[1]],1));assert!(!out.iter().any(|a|matches!(a,Action::Edit(_))));h.run(out);
        }
        h.state=healthy.clone();h.input(pointer(point,0));h.input(pointer([point[0]+20.,point[1]],1));h.input(pointer([point[0]+20.,point[1]],2));
        assert_ne!(h.state["project"],healthy["project"]);assert_eq!(h.engine.history(0).unwrap()["project"],healthy["project"]);h.engine.request_shutdown().unwrap();
    });
}

#[test]
fn camera_numeric_previews_hold_the_roving_id_in_all_chrome_hosts() {
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for reverse in [false, true] {
                let mut h = Harness::new(floating, locale, reverse);
                h.run(vec![Action::Edit(Command::SpatialKeyframes {
                    target: motion_core::KeyframeTarget::Property {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                    },
                    edit: motion_core::SpatialKeyEdit::EnableLocal { frames: vec![-3] },
                })]);
                h.input(Input::default());
                let before = h.state["project"].clone();
                let point = center(h.rect(Property::Position, 0));
                h.input(pointer(point, 0));
                let mut expected: Project = serde_json::from_value(before.clone()).unwrap();
                for dx in [10., 20., 30.] {
                    let actions = h.draw(pointer([point[0] + dx, point[1]], 1));
                    let value = actions
                        .iter()
                        .find_map(|a| match a {
                            Action::Edit(Command::TrackAt {
                                edit:
                                    motion_core::TrackEdit::SetSpatialKey {
                                        key: 2,
                                        value: motion_core::TrackValue::Vec3(v),
                                    },
                                ..
                            }) => Some(*v),
                            _ => None,
                        })
                        .expect("camera preview must address stable ID 2");
                    let Content::Camera { camera } = &mut expected.layers[0].content else {
                        panic!("camera")
                    };
                    camera
                        .position
                        .spatial_animation
                        .as_mut()
                        .unwrap()
                        .set_value(2, value)
                        .unwrap();
                    h.run(actions);
                    assert_eq!(h.state["project"], json!(expected));
                }
                h.input(pointer([point[0] + 30., point[1]], 2));
                assert_eq!(h.engine.history(0).unwrap()["project"], before);
                assert_eq!(h.engine.history(1).unwrap()["project"], json!(expected));
                h.engine.request_shutdown().unwrap();
            }
        }
    }
}
