use super::*;
use motion_core::{Ease, Engine, Keyframe, Layer, Project};
use motion_ui::input::Modifiers;
use motion_ui::paint::{Pattern, Shape};

fn fixture() -> Engine {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Timing", [100.; 2], [100.; 3], [1.; 4]);
    layer.transform.position.keys = [20, 70]
        .into_iter()
        .map(|frame| Keyframe {
            auto_bezier: false,
            frame,
            value: [frame as f32; 3],
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        })
        .collect();
    layer.transform.opacity.keys = [20, 90]
        .into_iter()
        .map(|frame| Keyframe {
            auto_bezier: false,
            frame,
            value: 0.5,
            ease: Ease::Hold,
            curve: None,
            spatial: None,
        })
        .collect();
    project.layers.push(layer);
    Engine::new(project).unwrap()
}
fn state(engine: &Engine) -> Value {
    serde_json::json!({"root":"timing-fixture","composition":motion_core::composition::MAIN_COMPOSITION,"document_generation":1,"revision":engine.revision(),
        "project":engine.project(),"timeline_layers":engine.project().timeline_layers(0.).unwrap(),"frame":0})
}
fn draw(
    panels: &mut Panels,
    engine: &Engine,
    input: Input,
    floating: bool,
    atlas: &mut TextAtlas,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let state = state(engine);
    let actions = if floating {
        floating_chrome(
            &mut paint,
            atlas,
            Panel::Timeline,
            [1000., 800.],
            panels,
            &state,
            &input,
        )
    } else {
        chrome(
            &mut paint,
            atlas,
            [1000., 800.],
            &dock::Node::dock(Panel::Timeline.dock()),
            panels,
            &state,
            &input,
        )
    };
    (paint, actions)
}
fn run(engine: &mut Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            Action::Seek(_) => {}
            other => panic!("unexpected key drag action: {other:?}"),
        }
    }
}
fn drag(expanded: bool) {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            let mut engine = fixture();
            let before = engine.project().clone();
            let mut panels = Panels {
                locale,
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            panels.select_only(Some(7));
            panels
                .animation_controls
                .begin_row_probe(7, Property::Position, None);
            if expanded {
                panels.timeline.expanded.insert(7);
            }
            let (paint, actions) =
                draw(&mut panels, &engine, Input::default(), floating, &mut atlas);
            assert!(actions.is_empty());
            let track = panels.timeline.zoom_track.unwrap();
            let ppf = track.width() / 120.;
            let x = track.min[0] + 20. * ppf;
            let y = if expanded {
                let row = panels.animation_controls.probed_row(None).unwrap();
                (row.min[1] + row.max[1]) * 0.5
            } else {
                paint
                    .shapes
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Procedural {
                            area,
                            kind: Pattern::Diamond,
                            ..
                        } if ((area.min[0] + area.max[0]) * 0.5 - x).abs() < 0.01 => {
                            Some((area.min[1] + area.max[1]) * 0.5)
                        }
                        _ => None,
                    })
                    .min_by(f32::total_cmp)
                    .expect("actual collapsed key diamond")
            };
            let point = [x, y];
            let (_, actions) = draw(
                &mut panels,
                &engine,
                Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            run(&mut engine, actions);
            let release = [x + 5. * ppf, y + 30.];
            let (_, actions) = draw(
                &mut panels,
                &engine,
                Input {
                    mouse: release,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            assert!(
                actions.iter().any(|a| matches!(
                    a,
                    Action::Edit(Command::MoveKey { .. }) | Action::EditBatch(_)
                )),
                "expanded={expanded} {locale:?} floating={floating}: {actions:?}"
            );
            run(&mut engine, actions);
            let (_, actions) = draw(
                &mut panels,
                &engine,
                Input {
                    mouse: [x + 60. * ppf, y],
                    released: Some((MouseButton::Left, release)),
                    ..Default::default()
                },
                floating,
                &mut atlas,
            );
            run(&mut engine, actions);
            assert_eq!(
                engine.project().layers[0].transform.position.keys[0].frame,
                25
            );
            assert_eq!(
                engine.project().layers[0].transform.position.keys[0].value,
                [20.; 3]
            );
            assert_eq!(
                engine.project().layers[0].transform.opacity.keys[0].frame,
                if expanded { 20 } else { 25 }
            );
            assert_eq!(
                engine.project().layers[0].timeline,
                before.layers[0].timeline
            );
            let after = engine.project().clone();
            let folder = tempfile::tempdir().unwrap();
            motion_core::storage::save(folder.path(), &after).unwrap();
            assert_eq!(motion_core::storage::load(folder.path()).unwrap(), after);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            engine.redo().unwrap();
            assert_eq!(engine.project(), &after);
        }
    }
}
#[test]
fn expanded_property_key_drags_time_without_changing_values() {
    drag(true)
}
#[test]
fn collapsed_summary_key_drags_all_properties_without_moving_the_layer() {
    drag(false)
}

fn start(panels: &mut Panels, view: &Value, copy: bool, property: bool) -> Vec<Action> {
    let row = timeline::rows(view).remove(0);
    let mut out = vec![];
    timeline_key_drag::begin(
        panels,
        view,
        &row,
        20,
        property.then_some((Property::Position, None)),
        5.,
        0.,
        &Input {
            pressed: Some((MouseButton::Left, [100., 100.])),
            held: Some(MouseButton::Left),
            modifiers: Modifiers {
                control: copy,
                ..Default::default()
            },
            ..Default::default()
        },
        &mut out,
    );
    out
}
fn advance(panels: &mut Panels, view: &Value, frame: u32, released: bool) -> Vec<Action> {
    let point = [100. + (frame as f32 - 20.) * 5., 300.];
    let mut out = vec![];
    timeline_key_drag::update(
        panels,
        view,
        &timeline::rows(view),
        5.,
        0.,
        &Input {
            mouse: point,
            held: (!released).then_some(MouseButton::Left),
            released: released.then_some((MouseButton::Left, point)),
            ..Default::default()
        },
        &mut out,
        3.,
    );
    out
}
#[test]
fn pending_worker_batches_allow_continuation_and_return_to_origin() {
    for copy in [false, true] {
        let mut engine = fixture();
        let before = engine.project().clone();
        let view = state(&engine);
        let mut panels = Panels::default();
        let mut pending = start(&mut panels, &view, copy, false);
        pending.extend(advance(&mut panels, &view, 25, false));
        pending.extend(advance(&mut panels, &view, 30, false));
        let occupied = advance(&mut panels, &view, 90, false);
        assert!(
            occupied.is_empty(),
            "opacity collision must veto the whole bundle"
        );
        let origin = advance(&mut panels, &view, 20, false);
        if copy {
            assert!(origin.is_empty(), "copied source remains occupied");
        }
        pending.extend(origin);
        pending.extend(advance(&mut panels, &view, 35, true));
        assert!(panels.timeline.timing_drag.is_none());
        assert_eq!(
            pending
                .iter()
                .filter(|a| matches!(a, Action::History(2)))
                .count(),
            1
        );
        assert_eq!(
            pending
                .iter()
                .filter(|a| matches!(a, Action::History(3)))
                .count(),
            1
        );
        run(&mut engine, pending);
        for frames in [
            engine.project().layers[0]
                .transform
                .position
                .keys
                .iter()
                .map(|k| k.frame)
                .collect::<Vec<_>>(),
            engine.project().layers[0]
                .transform
                .opacity
                .keys
                .iter()
                .map(|k| k.frame)
                .collect(),
        ] {
            assert!(frames.contains(&35));
            assert_eq!(frames.contains(&20), copy);
        }
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
}
#[test]
fn focus_scope_and_readonly_changes_cancel_the_transaction() {
    for case in 0..8 {
        let mut engine = fixture();
        let before = engine.project().clone();
        let mut panels = Panels::default();
        let actions = start(&mut panels, &state(&engine), false, true);
        run(&mut engine, actions);
        let actions = advance(&mut panels, &state(&engine), 25, false);
        run(&mut engine, actions);
        let mut view = state(&engine);
        let mut input = Input {
            held: Some(MouseButton::Left),
            mouse: [175., 100.],
            ..Default::default()
        };
        match case {
            0 => input.focus_lost = true,
            1 => input.keys.push((Key::Escape, Modifiers::default())),
            2 => view["root"] = "another-project".into(),
            3 => view["composition"] = "another-composition".into(),
            4 => view["observing"] = true.into(),
            5 => view["project"]["layers"][0]["locked"] = true.into(),
            6 => panels.timeline.graph_mode = true,
            _ => panels.select_only(None),
        }
        let mut out = vec![];
        timeline_key_drag::update(
            &mut panels,
            &view,
            &timeline::rows(&view),
            5.,
            0.,
            &input,
            &mut out,
            3.,
        );
        assert_eq!(
            out.iter()
                .filter(|a| matches!(a, Action::History(4)))
                .count(),
            1,
            "case {case}: {out:?}"
        );
        run(&mut engine, out);
        assert_eq!(engine.project(), &before);
        assert!(advance(&mut panels, &view, 40, true).is_empty());
    }
}
#[test]
fn readonly_diamonds_still_select_and_seek_without_beginning_edits() {
    let engine = fixture();
    let mut view = state(&engine);
    view["observing"] = true.into();
    let mut panels = Panels::default();
    let actions = start(&mut panels, &view, false, true);
    assert!(matches!(actions.as_slice(), [Action::Seek(20.)]));
    assert_eq!(panels.selected, Some(7));
    assert!(panels.timeline.timing_drag.is_none());
}

#[test]
fn summary_bundles_hidden_axes_masks_and_shape_keys_in_composition_time() {
    use motion_core::{
        masks::LayerMask,
        vector::{
            groups::{Composite, GroupItem, VectorGroup},
            FillRule, PathNode, VectorContent, VectorPath, VectorSource,
        },
        Content, LayerTimeline, Track,
    };
    let mut project = fixture().project().clone();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    let layer = &mut project.layers[0];
    layer.timeline = Some(LayerTimeline {
        in_frame: 12,
        out_frame: 120,
        offset_frame: 12,
    });
    for k in &mut layer.transform.position.keys {
        k.frame -= 12;
    }
    for k in &mut layer.transform.opacity.keys {
        k.frame -= 12;
    }
    layer.transform.position.separate().unwrap();
    let mut geometry = Track::constant([0.; 6]);
    geometry.upsert(8, [1.; 6], Ease::Linear).unwrap();
    let mut mask = LayerMask::new(
        4,
        VectorPath {
            tangents: Default::default(), id: 4,
            closed: false,
            nodes: vec![
                PathNode { id: 1, geometry },
                PathNode {
                    id: 2,
                    geometry: Track::constant([10.; 6]),
                },
            ],
        },
    );
    mask.opacity.upsert(8, 0.8, Ease::Linear).unwrap();
    layer.masks.push(mask);
    let mut color = Track::constant([1.; 4]);
    color.upsert(8, [0.5; 4], Ease::Linear).unwrap();
    layer.content = Content::Vector {
        vector: VectorContent {
            source: VectorSource::Group {
                group: Box::new(VectorGroup {
                    id: 1,
                    name: "Group".into(),
                    transform: Default::default(),
                    items: vec![GroupItem::Fill {
                        id: 2,
                        name: "Fill".into(),
                        color,
                        fill_rule: FillRule::NonZero,
                        composite: Composite::Below,
                    }],
                }),
            },
            fill: None,
            fill_rule: FillRule::NonZero,
            stroke: None,
            trim: None,
        },
    };
    let mut engine = Engine::new(project).unwrap();
    let before = engine.project().clone();
    let mut panels = Panels::default();
    let actions = start(&mut panels, &state(&engine), false, false);
    run(&mut engine, actions);
    let actions = advance(&mut panels, &state(&engine), 25, true);
    let commands = actions
        .iter()
        .find_map(|a| {
            if let Action::EditBatch(commands) = a {
                Some(commands)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        commands.len(),
        7,
        "three axes, opacity, mask scalar, authored node, fill"
    );
    assert!(commands.iter().any(|c| matches!(
        c,
        Command::MoveKey {
            axis: Some(motion_core::Axis::Z),
            from: 20,
            to: 25,
            ..
        }
    )));
    run(&mut engine, actions);
    let after = engine.project().clone();
    let moved = &after.layers[0];
    assert_eq!(moved.masks[0].opacity.keys[0].frame, 13);
    assert_eq!(moved.masks[0].path.nodes[0].geometry.keys[0].frame, 13);
    assert!(moved.masks[0].path.nodes[1].geometry.keys.is_empty());
    let view = state(&engine);
    let rows = timeline::rows(&view);
    assert!(rows[0].keys.contains(&25.));
    assert!(!rows[0].keys.contains(&20.));
    assert_eq!(moved.timeline, before.layers[0].timeline);
    let folder = tempfile::tempdir().unwrap();
    motion_core::storage::save(folder.path(), &after).unwrap();
    assert_eq!(motion_core::storage::load(folder.path()).unwrap(), after);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
}

#[test]
fn nonexistent_clicked_keys_are_bounded_and_recover_at_default_log_level() {
    const CHILD: &str = "MOTION_TIMING_KEY_SOURCE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let engine = fixture();
        let good = state(&engine);
        let mut bad = good.clone();
        bad["project"]["layers"][0]["transform"]["position"]["keys"] = serde_json::json!([]);
        bad["timeline_layers"][0]["properties"]["position"]["keys"] = serde_json::json!([]);
        let mut panels = Panels::default();
        let errors = || {
            std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .lines()
                .filter(|line| {
                    line.contains("ERROR") && line.contains("site=timeline.key_drag_source ")
                })
                .count()
        };
        for _ in 0..59 {
            assert!(start(&mut panels, &bad, false, true).is_empty());
        }
        assert_eq!(errors(), 0);
        start(&mut panels, &bad, false, true);
        assert_eq!(errors(), 1);
        for _ in 0..80 {
            start(&mut panels, &bad, false, true);
        }
        assert_eq!(errors(), 1);
        start(&mut panels, &good, false, true);
        for _ in 0..59 {
            start(&mut panels, &bad, false, true);
        }
        assert_eq!(errors(), 1);
        start(&mut panels, &bad, false, true);
        assert_eq!(errors(), 2);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let result=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","panels::timeline_key_drag_tests::nonexistent_clicked_keys_are_bounded_and_recover_at_default_log_level"])
        .env_remove("MOTION_LOG").env(CHILD,root.path()).output().unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&result.stderr)
            .matches("site=timeline.key_drag_source ")
            .count(),
        2
    );
    assert!(!String::from_utf8_lossy(&result.stdout).contains("timeline.key_drag_source"));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    assert!(file.contains("object=7 frame=20 phase=begin"));
}
