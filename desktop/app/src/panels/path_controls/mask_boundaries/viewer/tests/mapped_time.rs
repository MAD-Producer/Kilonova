use super::*;
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};

#[test]
fn mapped_boundary_feather_uses_the_exact_local_key_and_cancels_on_a_new_document_generation() {
    for reverse in [false, true] {
        for cancel in [false, true] {
            let mut p = fixture().snapshot();
            let layer = &mut p.layers[0];
            let offset = layer.timeline.unwrap().offset_frame;
            let delta = (33. - f64::from(offset)) / 1.5;
            layer.time_mapping = Some(LayerTimeMapping {
                clock: LayerClock::new(
                    TimeStretch::new(3, 2).unwrap(),
                    if reverse {
                        TimeDirection::Reverse
                    } else {
                        TimeDirection::Forward
                    },
                    0.,
                    -3. + if reverse { delta } else { -delta },
                )
                .unwrap(),
                remap: None,
            });
            layer.masks[0].feather = Track::constant([8., 13.]);
            layer.masks[0].feather_linked = false;
            for (f, value) in [(-6, [4., 9.]), (-3, [8., 13.]), (0, [12., 17.])] {
                layer.masks[0]
                    .feather
                    .upsert(f, value, motion_core::Ease::Linear)
                    .unwrap();
            }
            let mut engine = motion_core::Engine::new(p).unwrap();
            let before = engine.snapshot();
            let mut s = state(&engine);
            s["document_generation"] = 3.into();
            let mut controls = Controls::default();
            controls.tool = Tool::Mask;
            controls.mask = Some(1);
            let projection = projection(1., false);
            let grip = press(&mut controls, &s, &projection, true, 0);
            let end = [grip.screen[0] + 24., grip.screen[1]];
            let actions = draw(
                &mut controls,
                &s,
                &projection,
                &Input {
                    mouse: end,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                true,
            );
            assert!(actions
                .iter()
                .any(|a| matches!(a, Action::Edit(Command::TrackAt { frame: 33., .. }))));
            apply(&mut engine, actions);
            // The projection maps two screen pixels per source unit. The
            // feather grip scales the twelve-unit displacement by 4/3.
            assert!(
                (engine.project().layers[0].masks[0].feather.keys[1].value[0] - 24.).abs() < 1e-3
            );
            let mut expected = before.clone();
            expected.layers[0].masks[0].feather.keys[1].value[0] =
                engine.project().layers[0].masks[0].feather.keys[1].value[0];
            assert_ne!(expected, before);
            assert_eq!(engine.project(), &expected);
            let mut s = state(&engine);
            s["document_generation"] = if cancel { 4 } else { 3 }.into();
            let actions = draw(
                &mut controls,
                &s,
                &projection,
                &if cancel {
                    Input {
                        mouse: end,
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    }
                } else {
                    Input {
                        mouse: end,
                        released: Some((MouseButton::Left, end)),
                        ..Default::default()
                    }
                },
                true,
            );
            apply(&mut engine, actions);
            if cancel {
                assert_eq!(engine.project(), &before);
                assert!(!engine.undo().unwrap());
            } else {
                assert_eq!(engine.project(), &expected);
                assert!(engine.undo().unwrap());
                assert_eq!(engine.project(), &before);
            }
        }
    }
}
