use super::*;

#[test]
fn malformed_bound_numeric_values_use_default_error_channels_and_recover() {
    super::super::faults::verify("panels::property_authoring::spatial_tests::worker::stable::malformed_bound_numeric_values_use_default_error_channels_and_recover", "properties", &["properties.spatial_numeric_command", "properties.scrub_command"], || {
        let (_folder,engine,healthy,_)=start("plain");let mut atlas=TextAtlas::from_system(12.).unwrap();
        for _ in 0..180 {
            let mut p=ready(Panel::Properties);assert!(begin(&mut p,&mut atlas,&healthy).is_empty());
            let mut bad=healthy.clone();bad["project"]["layers"][0]["transform"]["position"]["spatial_animation"]["keys"][1]["value"][1]=Value::Null;
            let mut actions=vec![];properties::process_field(&mut p,&bad,&pointer(60.,false,false),&mut actions);
            assert!(matches!(actions.as_slice(),[Action::History(2)]));apply(&engine,actions);
            let mut actions=vec![];properties::process_field(&mut p,&healthy,&Input {focus_lost:true,..Default::default()},&mut actions);apply(&engine,actions);
        }
        let mut p=ready(Panel::Properties);begin(&mut p,&mut atlas,&healthy);
        let mut actions=vec![];properties::process_field(&mut p,&healthy,&pointer(60.,false,false),&mut actions);apply(&engine,actions);apply(&engine,vec![Action::History(4)]);
        assert_eq!(engine.state().unwrap()["project"],healthy["project"]);engine.request_shutdown().unwrap();
    });
}

#[test]
fn repeated_roving_scrubs_keep_four_keys_and_cancel_deleted_targets() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for mode in ["plain", "forward", "reverse"] {
        for host in [Panel::Properties, Panel::Timeline] {
            for cancel in [false, true] {
                let (p, at) = fixture(Channel::Position, mode, true);
                let folder = tempfile::tempdir().unwrap();
                motion_core::storage::save(folder.path(), &p).unwrap();
                let engine = crate::engine::Engine::start(
                    folder.path().to_owned(),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let before = engine.seek(at).unwrap();
                let mut panels = ready(host);
                assert!(begin(&mut panels, &mut atlas, &before).is_empty());
                let mut expected = p.clone();
                for (mouse, value) in [(60., 341.), (80., 351.), (100., 361.)] {
                    let current = engine.state().unwrap();
                    let mut actions = vec![];
                    properties::process_field(
                        &mut panels,
                        &current,
                        &pointer(mouse, false, false),
                        &mut actions,
                    );
                    assert!(
                        actions.iter().any(|a| matches!(
                            a,
                            Action::Edit(Command::TrackAt {
                                edit: TrackEdit::SetSpatialKey { key: 2, .. },
                                ..
                            })
                        )),
                        "{actions:?}"
                    );
                    apply(&engine, actions);
                    expected.layers[0]
                        .transform
                        .position
                        .spatial_animation
                        .as_mut()
                        .unwrap()
                        .set_value(2, [value, 180., 0.])
                        .unwrap();
                    assert_eq!(engine.state().unwrap()["project"], json!(expected));
                }
                let mut current = engine.state().unwrap();
                if cancel {
                    current["project"]["layers"][0]["transform"]["position"]["spatial_animation"]
                        ["keys"]
                        .as_array_mut()
                        .unwrap()
                        .remove(1);
                }
                let mut actions = vec![];
                properties::process_field(
                    &mut panels,
                    &current,
                    &pointer(100., false, true),
                    &mut actions,
                );
                assert!(
                    matches!(actions.as_slice(),[Action::History(op)] if *op==if cancel {4}else{3})
                );
                apply(&engine, actions);
                if cancel {
                    assert_eq!(engine.state().unwrap()["project"], before["project"]);
                } else {
                    engine.save().unwrap();
                    assert_eq!(motion_core::storage::load(folder.path()).unwrap(), expected);
                    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
                    assert_eq!(engine.history(1).unwrap()["project"], json!(expected));
                }
                let mut late = vec![];
                properties::process_field(
                    &mut panels,
                    &before,
                    &pointer(120., false, true),
                    &mut late,
                );
                assert!(late.is_empty());
                engine.request_shutdown().unwrap();
            }
        }
    }
}
