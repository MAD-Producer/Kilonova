use super::*;
fn engine() -> motion_core::Engine {
    let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Vector", [100.; 2], [320., 180., 0.], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 12,
        out_frame: 120,
        offset_frame: 12,
    });
    let original = super::super::new_vector(VectorPath {
        id: 1,
        closed: true,
        nodes: [[0., 0.], [100., 0.], [0., 100.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
            })
            .collect(),
    });
    layer.content = Content::Vector { vector: original };
    project.layers.push(layer);
    motion_core::Engine::new(project).unwrap()
}
fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            Action::Seek(_) => {}
            _ => panic!("unexpected vector action"),
        }
    }
}
fn draw_raw(
    controls: &mut Controls,
    engine: &motion_core::Engine,
    input: &Input,
    host: Panel,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let mut actions = vec![];
    let mut y = 0.;
    let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"frame":33,"project":engine.project()});
    let mut list = PaintList::default();
    controls.render(
        &mut list,
        atlas,
        Rect::new(0., 0., 400., 1600.),
        &mut y,
        &engine.project().layers[0],
        &state,
        input,
        Locale::Zh,
        host,
        &mut actions,
    );
    controls.finish_paint(
        &mut list,
        atlas,
        Rect::new(0., 0., 400., 1600.),
        &state,
        input,
        Locale::Zh,
        host,
        &mut actions,
    );
    actions
}
fn draw(
    controls: &mut Controls,
    engine: &motion_core::Engine,
    input: &Input,
    host: Panel,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let mut actions = draw_raw(controls, engine, input, host, atlas);
    // The root plus opens the real add menu; choose its Repeater item.
    if input
        .pressed
        .is_some_and(|(_, p)| p[0] == 378. && p[1] == 12.)
        && controls.menu.is_some()
    {
        for _ in 0..4 {
            actions.extend(draw_raw(
                controls,
                engine,
                &Input {
                    keys: vec![(Key::Down, Default::default())],
                    ..Default::default()
                },
                host,
                atlas,
            ));
        }
        actions.extend(draw_raw(
            controls,
            engine,
            &Input {
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            host,
            atlas,
        ));
    }
    actions
}
fn source(engine: &motion_core::Engine) -> &VectorContent {
    let Content::Vector { vector } = &engine.project().layers[0].content else {
        panic!()
    };
    vector
}
fn repeater(engine: &motion_core::Engine) -> &Repeater {
    let VectorSource::Group { group } = &source(engine).source else {
        panic!()
    };
    let GroupItem::Repeater { repeater, .. } = &group.items[1] else {
        panic!()
    };
    repeater
}
fn press(x: f32, y: f32) -> Input {
    Input {
        mouse: [x, y],
        pressed: Some((MouseButton::Left, [x, y])),
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
#[test]
fn add_repeater_from_ui_preserves_original_geometry_and_style_and_roundtrips() {
    let mut engine = engine();
    let before = engine.project().clone();
    let mut controls = Controls::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let actions = draw(
        &mut controls,
        &engine,
        &press(378., 12.),
        Panel::Properties,
        &mut atlas,
    );
    assert!(matches!(
        &actions[..],
        [Action::Edit(Command::Vector {
            action: VectorAction::EditGroupStructure { .. },
            ..
        })]
    ));
    apply(&mut engine, actions);
    let VectorSource::Group { group } = &source(&engine).source else {
        panic!()
    };
    let GroupItem::Geometry { vector, size, .. } = &group.items[0] else {
        panic!()
    };
    let Content::Vector { vector: original } = &before.layers[0].content else {
        panic!()
    };
    assert_eq!(vector.as_ref(), original);
    assert_eq!(size.value, [100.; 2]);
    assert_eq!(group.items[1].id(), 3);
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), engine.project()).unwrap();
    assert_eq!(
        &motion_core::storage::load(directory.path()).unwrap(),
        engine.project()
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
#[test]
fn repeater_eight_parameter_rows_animate_scrub_and_preserve_vector_components_with_one_undo() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for host in [Panel::Properties, Panel::Vector] {
        for index in 0..8 {
            let mut engine = engine();
            let mut controls = Controls::default();
            let actions = draw(&mut controls, &engine, &press(378., 12.), host, &mut atlas);
            apply(&mut engine, actions);
            let row_y = 110. + index as f32 * 22.;
            draw_raw(&mut controls, &engine, &Input::default(), host, &mut atlas);
            let parameter = ["copies", "offset", "position", "anchor", "scale", "rotation", "start_opacity", "end_opacity"][index];
            // Resolve the rendered fields: the navigation rail changes their
            // horizontal positions, not the value/gesture contract below.
            let (watch, number) = row(&controls, host, 3, parameter, 0);
            let actions = draw(
                &mut controls,
                &engine,
                &at(watch),
                host,
                &mut atlas,
            );
            apply(&mut engine, actions);
            let before = engine.project().clone();
            let sign = if index >= 6 { -1. } else { 1. };
            let start = center(number);
            for input in [
                press(start[0], row_y + 12.),
                Input {
                    mouse: [start[0] + sign * 12., row_y + 12.],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                Input {
                    mouse: [start[0] + sign * 20., row_y + 12.],
                    released: Some((MouseButton::Left, [start[0] + sign * 20., row_y + 12.])),
                    ..Default::default()
                },
            ] {
                let actions = draw(&mut controls, &engine, &input, host, &mut atlas);
                apply(&mut engine, actions);
            }
            let r = repeater(&engine);
            let local = 21;
            match index {
                0 => assert_eq!(
                    (r.copies.keys[0].frame, r.copies.keys[0].value),
                    (local, 23.)
                ),
                1 => assert_eq!(
                    (r.offset.keys[0].frame, r.offset.keys[0].value),
                    (local, 20.)
                ),
                2 => assert_eq!(
                    (r.position.keys[0].frame, r.position.keys[0].value),
                    (local, [120., 0.])
                ),
                3 => assert_eq!(
                    (r.anchor.keys[0].frame, r.anchor.keys[0].value),
                    (local, [20., 0.])
                ),
                4 => assert_eq!(
                    (r.scale.keys[0].frame, r.scale.keys[0].value),
                    (local, [120., 100.])
                ),
                5 => assert_eq!(
                    (r.rotation.keys[0].frame, r.rotation.keys[0].value),
                    (local, 20.)
                ),
                6 => assert_eq!(
                    (r.start_opacity.keys[0].frame, r.start_opacity.keys[0].value),
                    (local, 80.)
                ),
                _ => assert_eq!(
                    (r.end_opacity.keys[0].frame, r.end_opacity.keys[0].value),
                    (local, 80.)
                ),
            }
            assert_ne!(engine.project(), &before);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
        }
    }
}
#[test]
fn repeater_drag_cancels_on_escape_and_collapsing_group_and_other_host_cannot_consume_it() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for collapse in [false, true] {
        let mut engine = engine();
        let mut controls = Controls::default();
        let actions = draw(
            &mut controls,
            &engine,
            &press(378., 12.),
            Panel::Vector,
            &mut atlas,
        );
        apply(&mut engine, actions);
        let before = engine.project().clone();
        draw_raw(&mut controls, &engine, &Input::default(), Panel::Vector, &mut atlas);
        let start = center(row(&controls, Panel::Vector, 3, "copies", 0).1);
        let actions = draw(
            &mut controls,
            &engine,
            &press(start[0], 122.),
            Panel::Vector,
            &mut atlas,
        );
        apply(&mut engine, actions);
        let actions = draw(
            &mut controls,
            &engine,
            &Input {
                mouse: [start[0] + 12., 122.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            Panel::Vector,
            &mut atlas,
        );
        apply(&mut engine, actions);
        assert_ne!(engine.project(), &before);
        assert!(draw(
            &mut controls,
            &engine,
            &Input {
                mouse: [start[0] + 40., 122.],
                released: Some((MouseButton::Left, [start[0] + 40., 122.])),
                ..Default::default()
            },
            Panel::Properties,
            &mut atlas
        )
        .is_empty());
        let input = if collapse {
            press(50., 42.)
        } else {
            Input {
                keys: vec![(Key::Escape, Default::default())],
                ..Default::default()
            }
        };
        let actions = draw(&mut controls, &engine, &input, Panel::Vector, &mut atlas);
        apply(&mut engine, actions);
        assert_eq!(engine.project(), &before);
        assert!(!controls.editing());
    }
}
#[test]
fn removing_repeater_finishes_its_value_gesture_and_undo_restores_each_step() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut engine = engine();
    let mut controls = Controls::default();
    let actions = draw(
        &mut controls,
        &engine,
        &press(378., 12.),
        Panel::Properties,
        &mut atlas,
    );
    apply(&mut engine, actions);
    let before = engine.project().clone();
    draw_raw(&mut controls, &engine, &Input::default(), Panel::Properties, &mut atlas);
    let start = center(row(&controls, Panel::Properties, 3, "copies", 0).1);
    for input in [
        press(start[0], 122.),
        Input {
            mouse: [start[0] + 12., 122.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    ] {
        let actions = draw(
            &mut controls,
            &engine,
            &input,
            Panel::Properties,
            &mut atlas,
        );
        apply(&mut engine, actions);
    }
    assert_eq!(repeater(&engine).copies.value, 15.);
    let edited = engine.project().clone();
    let actions = draw(
        &mut controls,
        &engine,
        &press(378., 100.),
        Panel::Properties,
        &mut atlas,
    );
    apply(&mut engine, actions);
    let VectorSource::Group { group } = &source(&engine).source else {
        panic!()
    };
    assert_eq!(group.items.len(), 1);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &edited);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

fn install_contents(engine: &mut motion_core::Engine) {
    for (id, kind) in [(3, 1), (4, 2), (5, 3), (6, 4)] {
        apply(
            engine,
            vec![structure(
                7,
                StructureEdit::Insert {
                    parent: 1,
                    item: new_item(id, kind, Locale::Zh),
                },
            )],
        );
    }
}
fn expand_all(c: &mut Controls) {
    for key in [
        (7, 1, 0),
        (7, 1, 1),
        (7, 2, 2),
        (7, 3, 2),
        (7, 4, 2),
        (7, 5, 2),
        (7, 6, 2),
    ] {
        c.open.insert(key);
    }
    c.initialized.insert(7);
}
fn center(r: Rect) -> [f32; 2] {
    [r.min[0] + r.width() * 0.5, r.min[1] + r.height() * 0.5]
}
fn at(r: Rect) -> Input {
    let p = center(r);
    press(p[0], p[1])
}
fn row(c: &Controls, host: Panel, item: u64, name: &str, axis: usize) -> (Rect, Rect) {
    c.rects[&(host as u32, 7, item, name.into(), axis)]
}
#[test]
fn nested_shape_navigation_keeps_complete_watch_slots_clear_of_values() {
    let styles = PaintList::default().styles();
    for host in [Panel::Properties, Panel::Vector, Panel::Timeline] {
        for width in [220., 360., 720.] { for depth in [0, 1, 4, 16, 32] { for axes in [1, 2, 4] {
            let area = Rect::new(17., 31., width, 600.);
            let f = layout(&styles, area, 120., depth, 0, axes, host);
            assert_eq!(f.watch.width(), styles.metric("icon.slot", 20.));
            assert!(f.watch.max[0] <= f.caption.min[0]);
            assert!(f.caption.max[0] <= f.number.min[0], "{host:?}/{width}/{depth}/{axes}");
        }}}
    }
}
#[test]
fn shape_navigation_buttons_key_only_the_named_parameter_and_keep_local_time_and_undo() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for host in [Panel::Properties, Panel::Vector] {
        for (item, name) in [(1, "position"), (3, "color"), (4, "width"), (5, "start")] {
            let mut engine = engine();
            install_contents(&mut engine);
            let tree = parameters::tree(source(&engine), [100.; 2], 21., Locale::Zh);
            let parameter = parameters::lookup(&tree, item, name).unwrap();
            for frame in [12, 63] {
                engine.apply(parameter.set(7, item, frame, Some(true))).unwrap();
            }
            let mut controls = Controls::default();
            expand_all(&mut controls);
            draw_raw(&mut controls, &engine, &Input::default(), host, &mut atlas);
            let watch = row(&controls, host, item, name, 0).0;
            let slot = PaintList::default().styles().metric("icon.slot", 20.);
            let point = [PaintList::default().styles().metric("padding.panel", 8.) + slot * 1.5, watch.min[1] + watch.height() * 0.5];
            let before = engine.project().clone();
            let actions = draw_raw(&mut controls, &engine, &press(point[0], point[1]), host, &mut atlas);
            assert_eq!(actions.len(), 1, "{host:?}/{name}: {actions:?}");
            apply(&mut engine, actions);
            let tree = parameters::tree(source(&engine), [100.; 2], 21., Locale::Zh);
            let current = parameters::lookup(&tree, item, name).unwrap();
            assert_eq!(current.keys.iter().map(|(frame, _)| *frame).collect::<Vec<_>>(), vec![0, 21, 51]);
            let added = engine.project().clone();
            let folder = tempfile::tempdir().unwrap();
            motion_core::storage::save(folder.path(), engine.project()).unwrap();
            assert_eq!(motion_core::storage::load(folder.path()).unwrap(), added);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            engine.redo().unwrap();
            assert_eq!(engine.project(), &added);
            for (slot, expected) in [(0, 12.), (2, 63.)] {
                let x = PaintList::default().styles().metric("padding.panel", 8.) + (slot as f32 + 0.5) * PaintList::default().styles().metric("icon.slot", 20.);
                let actions = draw_raw(&mut controls, &engine, &press(x, point[1]), host, &mut atlas);
                assert!(matches!(actions.as_slice(), [Action::Seek(frame)] if *frame == expected), "{host:?}/{name}: {actions:?}");
                assert_eq!(engine.project(), &added);
            }
            let actions = draw_raw(&mut controls, &engine, &press(point[0], point[1]), host, &mut atlas);
            assert_eq!(actions.len(), 1);
            apply(&mut engine, actions);
            assert_eq!(parameters::lookup(&parameters::tree(source(&engine), [100.; 2], 21., Locale::Zh), item, name).unwrap().keys.iter().map(|(frame, _)| *frame).collect::<Vec<_>>(), vec![0, 51]);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &added);
            for input in [Input { focus_lost: true, ..press(point[0], point[1]) }, Input { keys: vec![(Key::Escape, Default::default())], ..press(point[0], point[1]) }] {
                assert!(draw_raw(&mut controls, &engine, &input, host, &mut atlas).is_empty());
            }
            assert_eq!(engine.project(), &added);
        }
    }
}
#[test]
fn shape_content_rows_edit_fill_stroke_trim_and_group_values_in_both_inspector_hosts() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for host in [Panel::Properties, Panel::Vector] {
        for (item, key, axis) in [
            (1, "position", 0),
            (3, "color", 2),
            (4, "width", 0),
            (5, "start", 0),
            (6, "position", 0),
            (2, "size", 1),
        ] {
            let mut e = engine();
            install_contents(&mut e);
            let mut c = Controls::default();
            expand_all(&mut c);
            draw_raw(&mut c, &e, &Input::default(), host, &mut atlas);
            let (watch, mut number) = row(&c, host, item, key, axis);
            let actions = draw_raw(&mut c, &e, &at(watch), host, &mut atlas);
            apply(&mut e, actions);
            if key == "color" {
                let preview = c.color_rects[&(host as u32, 7, item, key.into())];
                assert!(draw_raw(&mut c, &e, &at(preview), host, &mut atlas).is_empty());
                number = row(&c, host, item, key, axis).1;
            }
            let before = e.project().clone();
            let old = parameters::lookup(
                &parameters::tree(source(&e), [100.; 2], 21., Locale::Zh),
                item,
                key,
            )
            .unwrap();
            let start = center(number);
            let direction = if key == "color" { -1. } else { 1. };
            for input in [
                at(number),
                Input {
                    mouse: [start[0] + direction * 10., start[1]],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                Input {
                    mouse: [start[0] + direction * 20., start[1]],
                    released: Some((MouseButton::Left, [start[0] + direction * 20., start[1]])),
                    ..Default::default()
                },
            ] {
                let actions = draw_raw(&mut c, &e, &input, host, &mut atlas);
                apply(&mut e, actions);
            }
            let current = parameters::lookup(
                &parameters::tree(source(&e), [100.; 2], 21., Locale::Zh),
                item,
                key,
            )
            .unwrap();
            assert!(current.animated(), "{item}/{key}");
            assert_eq!(current.keys[0].0, 21);
            assert_ne!(current.values[axis], old.values[axis]);
            for i in 0..old.values.len() {
                if i != axis {
                    assert_eq!(current.values[i], old.values[i]);
                }
            }
            e.undo().unwrap();
            assert_eq!(e.project(), &before);
        }
    }
}
#[test]
fn shape_content_dropdown_blocks_obscured_fields_and_opens_only_in_its_owner_host() {
    let mut e = engine();
    install_contents(&mut e);
    let mut c = Controls::default();
    expand_all(&mut c);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    draw_raw(&mut c, &e, &Input::default(), Panel::Vector, &mut atlas);
    let rect = c.option_rects[&(
        Panel::Vector as u32,
        7,
        6,
        parameters::OptionKind::Composite,
    )];
    let before = e.project().clone();
    assert!(draw_raw(&mut c, &e, &at(rect), Panel::Vector, &mut atlas).is_empty());
    assert!(c.menu_in(Panel::Vector));
    let (watch, _) = row(&c, Panel::Vector, 5, "start", 0);
    assert!(draw_raw(&mut c, &e, &at(watch), Panel::Vector, &mut atlas).is_empty());
    assert_eq!(e.project(), &before);
    assert!(!c.menu_in(Panel::Vector));
    draw_raw(&mut c, &e, &at(rect), Panel::Vector, &mut atlas);
    assert!(draw_raw(
        &mut c,
        &e,
        &Input {
            keys: vec![(Key::Down, Default::default())],
            ..Default::default()
        },
        Panel::Properties,
        &mut atlas
    )
    .is_empty());
    assert!(c.menu_in(Panel::Vector));
    draw_raw(
        &mut c,
        &e,
        &Input {
            keys: vec![(Key::Down, Default::default())],
            ..Default::default()
        },
        Panel::Vector,
        &mut atlas,
    );
    let actions = draw_raw(
        &mut c,
        &e,
        &Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        Panel::Vector,
        &mut atlas,
    );
    apply(&mut e, actions);
    let g = group(source(&e), [100.; 2]);
    let GroupItem::Repeater { repeater, .. } = &g.items[4] else {
        panic!()
    };
    assert_eq!(repeater.composite, Composite::Above);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
}
#[test]
fn shape_content_geometry_tree_and_fields_stay_inside_narrow_and_wide_panels() {
    let e = engine();
    let styles = motion_ui::style::default_stylesheet_arc();
    for width in [160., 180., 220., 286., 400., 720.] {
        for axes in [1, 2, 4] {
            for host in [Panel::Properties, Panel::Vector, Panel::Timeline] {
                let area = Rect::new(0., 0., width, 500.);
                let mut last = 0.;
                for axis in 0..axes {
                    let r = layout(styles.as_ref(), area, 0., 3, axis, axes, host).number;
                    assert!(r.min[0] >= last && r.max[0] <= width);
                    last = r.max[0];
                }
            }
        }
    }
    let n = parameters::tree(source(&e), [100.; 2], 0., Locale::Zh);
    assert!(parameters::lookup(&n, 2, "fill").is_some());
    assert!(parameters::lookup(&n, 2, "stroke_width").is_some());
}

fn key_draw(
    c: &mut keys::Controls,
    e: &motion_core::Engine,
    item: u64,
    key: &str,
    input: &Input,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"frame":33,"revision":e.revision(),"project":e.project()});
    let p = parameters::lookup(
        &parameters::tree(source(e), [100.; 2], 21., Locale::En),
        item,
        key,
    )
    .unwrap();
    let mut out = vec![];
    let mut list = PaintList::default();
    c.prepare(&state, input, &mut out);
    let neutral = Input::default();
    c.lane(
        &mut list,
        Rect::new(100., 100., 400., 22.),
        4.,
        0.,
        &state,
        7,
        item,
        &p,
        12,
        false,
        if c.modal() { &neutral } else { input },
        &mut out,
    );
    c.process(
        &list,
        &state,
        Some(7),
        Rect::new(0., 0., 500., 500.),
        4.,
        0.,
        input,
        &mut out,
    );
    c.overlay(
        &mut list,
        atlas,
        Rect::new(0., 0., 500., 500.),
        input,
        Locale::En,
        &mut out,
    );
    out
}
#[test]
fn shape_content_timeline_keys_move_copy_cancel_and_delete_with_single_undo() {
    use motion_core::vector::groups::KeyEdit;
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for (item, key) in [
        (1, "position"),
        (2, "size"),
        (3, "color"),
        (4, "width"),
        (5, "start"),
        (6, "copies"),
    ] {
        let mut e = engine();
        install_contents(&mut e);
        for frame in [21, 60] {
            let p = parameters::lookup(
                &parameters::tree(source(&e), [100.; 2], f64::from(frame) - 12., Locale::En),
                item,
                key,
            )
            .unwrap();
            e.apply(p.set(7, item, frame, Some(true))).unwrap();
        }
        e.apply(Command::Vector {
            object: 7,
            action: VectorAction::EditGroupKey {
                item,
                parameter: key.into(),
                edit: KeyEdit::Curve {
                    frame: 21,
                    easing: motion_core::Easing {
                        ease: motion_core::Ease::InOut,
                        curve: None,
                    },
                },
            },
        })
        .unwrap();
        let before = e.project().clone();
        let mut c = keys::Controls::default();
        let x = |f| 100. + 4. * f as f32;
        let actions = key_draw(&mut c, &e, item, key, &press(x(21), 111.), &mut atlas);
        apply(&mut e, actions);
        for (frame, release) in [(30, false), (42, true)] {
            let point = [x(frame), 111.];
            let actions = key_draw(
                &mut c,
                &e,
                item,
                key,
                &Input {
                    mouse: point,
                    held: (!release).then_some(MouseButton::Left),
                    released: release.then_some((MouseButton::Left, point)),
                    ..Default::default()
                },
                &mut atlas,
            );
            apply(&mut e, actions);
        }
        let p = parameters::lookup(
            &parameters::tree(source(&e), [100.; 2], 21., Locale::En),
            item,
            key,
        )
        .unwrap();
        assert_eq!(p.keys.iter().map(|(f, _)| *f).collect::<Vec<_>>(), [30, 48]);
        assert_eq!(p.keys[0].1.ease, motion_core::Ease::InOut);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        let mut input = press(x(21), 111.);
        input.modifiers.control = true;
        let actions = key_draw(&mut c, &e, item, key, &input, &mut atlas);
        apply(&mut e, actions);
        let point = [x(45), 111.];
        let actions = key_draw(
            &mut c,
            &e,
            item,
            key,
            &Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            &mut atlas,
        );
        apply(&mut e, actions);
        let p = parameters::lookup(
            &parameters::tree(source(&e), [100.; 2], 21., Locale::En),
            item,
            key,
        )
        .unwrap();
        assert_eq!(
            p.keys.iter().map(|(f, _)| *f).collect::<Vec<_>>(),
            [9, 33, 48]
        );
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
        let actions = key_draw(&mut c, &e, item, key, &press(x(21), 111.), &mut atlas);
        apply(&mut e, actions);
        let actions = key_draw(
            &mut c,
            &e,
            item,
            key,
            &Input {
                mouse: [x(30), 111.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            &mut atlas,
        );
        apply(&mut e, actions);
        let actions = key_draw(
            &mut c,
            &e,
            item,
            key,
            &Input {
                focus_lost: true,
                ..Default::default()
            },
            &mut atlas,
        );
        apply(&mut e, actions);
        assert_eq!(e.project(), &before);
        assert!(!c.editing());
        key_draw(&mut c, &e, item, key, &press(x(21), 111.), &mut atlas);
        key_draw(
            &mut c,
            &e,
            item,
            key,
            &Input {
                mouse: [x(21), 111.],
                released: Some((MouseButton::Left, [x(21), 111.])),
                ..Default::default()
            },
            &mut atlas,
        );
        let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"project":e.project(),"frame":21});
        let mut actions = vec![];
        assert!(c.shortcuts(
            &state,
            Some(7),
            &Input {
                keys: vec![(Key::K, Default::default())],
                ..Default::default()
            },
            &mut actions
        ));
        assert!(matches!(&actions[..],[Action::Seek(f)]if *f==60.));
        actions.clear();
        assert!(c.shortcuts(
            &state,
            Some(7),
            &Input {
                keys: vec![(Key::Delete, Default::default())],
                ..Default::default()
            },
            &mut actions
        ));
        apply(&mut e, actions);
        assert_eq!(e.project().layers.len(), 1);
        let p = parameters::lookup(
            &parameters::tree(source(&e), [100.; 2], 21., Locale::En),
            item,
            key,
        )
        .unwrap();
        assert_eq!(p.keys.len(), 1);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
    }
}
#[test]
fn shape_content_key_popup_samples_clicked_time_and_changes_easing() {
    let mut e = engine();
    install_contents(&mut e);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut c = keys::Controls::default();
    for (frame, v) in [(21, 3.), (60, 9.)] {
        e.apply(Command::Vector {
            object: 7,
            action: VectorAction::SetGroupParameter {
                item: 6,
                parameter: "copies".into(),
                frame,
                value: ParameterValue::Scalar(v),
                animated: Some(true),
            },
        })
        .unwrap();
    }
    let before = e.project().clone();
    let mut input = press(100. + 4. * 40., 111.);
    input.pressed = Some((MouseButton::Right, input.mouse));
    assert!(key_draw(&mut c, &e, 6, "copies", &input, &mut atlas).is_empty());
    let actions = key_draw(
        &mut c,
        &e,
        6,
        "copies",
        &Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        &mut atlas,
    );
    apply(&mut e, actions);
    let p = parameters::lookup(
        &parameters::tree(source(&e), [100.; 2], 28., Locale::En),
        6,
        "copies",
    )
    .unwrap();
    assert_eq!(
        p.keys.iter().map(|(f, _)| *f).collect::<Vec<_>>(),
        [9, 28, 48]
    );
    assert!((p.values[0] - (3. + 6. * 19. / 39.)).abs() < 0.001);
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
    let mut input = press(100. + 4. * 21., 111.);
    input.pressed = Some((MouseButton::Right, input.mouse));
    key_draw(&mut c, &e, 6, "copies", &input, &mut atlas);
    for _ in 0..5 {
        key_draw(
            &mut c,
            &e,
            6,
            "copies",
            &Input {
                keys: vec![(Key::Down, Default::default())],
                ..Default::default()
            },
            &mut atlas,
        );
    }
    let actions = key_draw(
        &mut c,
        &e,
        6,
        "copies",
        &Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        &mut atlas,
    );
    apply(&mut e, actions);
    assert_eq!(
        parameters::lookup(
            &parameters::tree(source(&e), [100.; 2], 9., Locale::En),
            6,
            "copies"
        )
        .unwrap()
        .keys[0]
            .1
            .ease,
        motion_core::Ease::InOut
    );
    e.undo().unwrap();
    assert_eq!(e.project(), &before);
}
#[test]
fn shape_content_animated_filter_reveals_real_tracks_and_counts_match_rendered_tree() {
    let mut e = engine();
    install_contents(&mut e);
    for (item, key) in [(1, "position"), (6, "copies")] {
        let p = parameters::lookup(
            &parameters::tree(source(&e), [100.; 2], 21., Locale::En),
            item,
            key,
        )
        .unwrap();
        e.apply(p.set(7, item, 33, Some(true))).unwrap();
    }
    let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"project":e.project(),"frame":33});
    let rows = crate::panels::timeline::rows(&state);
    assert!(rows[0].keys.contains(&33.));
    let mut panels = crate::panels::Panels::default();
    panels.selected = Some(7);
    panels
        .timeline
        .filters
        .insert(7, crate::panels::types::PropertyFilter::Animated);
    timeline::reveal_animated(&mut panels, &state, 7);
    assert_eq!(timeline::row_count(&rows[0], &panels.timeline), 6);
    assert!(panels.timeline.vector_open.contains(&(7, 6, 2)));
}

#[test]
fn shape_content_number_drag_in_main_and_floating_timeline_selects_owner_and_commits() {
    use motion_ui::dock::Node;
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        let mut e = engine();
        install_contents(&mut e);
        let mut panels = crate::panels::Panels::default();
        panels.focused = Some(Panel::Timeline);
        panels.timeline.expanded.insert(7);
        panels.timeline.collapsed_transforms.insert(7);
        panels
            .timeline
            .vector_open
            .extend([(7, 0, 0), (7, 1, 0), (7, 6, 2)]);
        let layout = Node::dock(Panel::Timeline.dock());
        let mut render = |panels: &mut crate::panels::Panels,
                          e: &motion_core::Engine,
                          input: &Input| {
            let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"project":e.project(),"revision":e.revision(),"frame":33});
            if floating {
                crate::panels::floating_chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    Panel::Timeline,
                    [1280., 900.],
                    panels,
                    &state,
                    input,
                )
            } else {
                crate::panels::chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    [1280., 900.],
                    &layout,
                    panels,
                    &state,
                    input,
                )
            }
        };
        assert!(render(&mut panels, &e, &Input::default()).is_empty());
        let rect = panels.path_controls.groups.rects
            [&(Panel::Timeline as u32, 7, 6, "copies".into(), 0)]
            .1;
        let start = center(rect);
        let before = e.project().clone();
        for input in [
            at(rect),
            Input {
                mouse: [start[0] + 12., start[1]],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            Input {
                mouse: [start[0] + 20., start[1]],
                released: Some((MouseButton::Left, [start[0] + 20., start[1]])),
                ..Default::default()
            },
        ] {
            let actions = render(&mut panels, &e, &input);
            apply(&mut e, actions);
        }
        assert_eq!(panels.selected, Some(7));
        let g = group(source(&e), [100.; 2]);
        let GroupItem::Repeater { repeater, .. } = &g.items[4] else {
            panic!()
        };
        assert_eq!(repeater.copies.value, 23.);
        e.undo().unwrap();
        assert_eq!(e.project(), &before);
    }
}
#[test]
fn shape_content_key_selection_cannot_delete_from_a_different_project() {
    let mut e = engine();
    install_contents(&mut e);
    let p = parameters::lookup(
        &parameters::tree(source(&e), [100.; 2], 21., Locale::En),
        6,
        "copies",
    )
    .unwrap();
    e.apply(p.set(7, 6, 21, Some(true))).unwrap();
    let mut c = keys::Controls::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    key_draw(&mut c, &e, 6, "copies", &press(184., 111.), &mut atlas);
    key_draw(
        &mut c,
        &e,
        6,
        "copies",
        &Input {
            mouse: [184., 111.],
            released: Some((MouseButton::Left, [184., 111.])),
            ..Default::default()
        },
        &mut atlas,
    );
    let state = serde_json::json!({"root":"different","composition":motion_core::composition::MAIN_COMPOSITION,"project":e.project(),"frame":21});
    let mut actions = vec![];
    assert!(c.shortcuts(
        &state,
        Some(7),
        &Input {
            keys: vec![(Key::Delete, Default::default())],
            ..Default::default()
        },
        &mut actions
    ));
    assert!(actions.is_empty());
}

#[test]
fn shape_content_source_cache_reuses_authored_revision_but_never_caches_sampled_time() {
    let mut e = engine();
    install_contents(&mut e);
    let cache = keys::SourceCache::default();
    let snapshot = |e: &motion_core::Engine| serde_json::json!({"root":"fixture","composition":"main","project":e.project(),"revision":e.revision(),"frame":33});
    let state = snapshot(&e);
    let a = keys::cached_layer(&state, 7, &cache).unwrap();
    let mut sought = state.clone();
    sought["frame"] = 60.into();
    let b = keys::cached_layer(&sought, 7, &cache).unwrap();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    e.apply(Command::Vector {
        object: 7,
        action: VectorAction::SetGroupParameter {
            item: 6,
            parameter: "copies".into(),
            frame: 33,
            value: ParameterValue::Scalar(6.),
            animated: Some(true),
        },
    })
    .unwrap();
    let state = snapshot(&e);
    let c = keys::cached_layer(&state, 7, &cache).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&a, &c));
    let mut other = state.clone();
    other["root"] = "different".into();
    let d = keys::cached_layer(&other, 7, &cache).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&c, &d));
    other.as_object_mut().unwrap().remove("revision");
    let d = keys::cached_layer(&other, 7, &cache).unwrap();
    let f = keys::cached_layer(&other, 7, &cache).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&d, &f));
}
