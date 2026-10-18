use super::*;
use super::types::ColumnGroup;
use motion_core::compositing::{BlendMode, BlendSpace};
use motion_core::{Content, Engine, Layer, Project};
fn fixture() -> Engine {
    let mut p = Project::new(640, 480, 30, 120).unwrap();
    let child = Layer::solid(7, "Title", [100.; 2], [100., 200., 0.], [1.; 4]);
    let mut controller = Layer::solid(42, "Control", [100.; 2], [50., 70., 0.], [0.; 4]);
    controller.content = Content::Null;
    let duplicate = Layer::solid(99, "Control", [100.; 2], [0.; 3], [1.; 4]);
    p.layers = vec![child, controller, duplicate];
    let mut engine = Engine::new(p).unwrap();
    engine
        .apply(Command::Parent {
            object: 99,
            parent: Some(7),
            frame: 21,
        })
        .unwrap();
    engine
}
fn snapshot(engine: &Engine) -> Value {
    serde_json::json!({"frame":21.5,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(21.5)})
}
fn draw(panels: &mut Panels, state: &Value, input: Input) -> Vec<Action> {
    let mut out = vec![];
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    timeline::render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 0., 1200., 400.),
        panels,
        state,
        &input,
        &mut out,
    );
    out
}
fn click(x: f32, y: f32) -> Input {
    Input {
        mouse: [x, y],
        pressed: Some((MouseButton::Left, [x, y])),
        ..Default::default()
    }
}
#[test]
fn parent_dropdown_commits_real_id_keeps_world_pose_and_undo_restores_project() {
    for locale in [Locale::Zh, Locale::En] {
        let mut engine = fixture();
        let before = engine.project().clone();
        let state = snapshot(&engine);
        let mut panels = Panels {
            locale,
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        assert!(draw(&mut panels, &state, click(560., 118.)).is_empty());
        assert!(panels.timeline_columns.editing());
        assert_eq!(panels.selected, Some(7));
        let actions = draw(&mut panels, &state, click(560., 190.));
        assert!(matches!(
            actions.as_slice(),
            [Action::Edit(Command::Parent {
                object: 7,
                parent: Some(42),
                frame: 21
            })]
        ));
        let mut old = motion_render::Scene::new(&before);
        old.sample(&before, 21., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        for action in actions {
            if let Action::Edit(command) = action {
                engine.apply(command).unwrap();
            }
        }
        let mut new = motion_render::Scene::new(engine.project());
        new.sample(
            engine.project(),
            21.,
            None,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
        assert!(old
            .world_matrix(7)
            .unwrap()
            .abs_diff_eq(new.world_matrix(7).unwrap(), 0.005));
        assert_eq!(
            engine.project().layers[0].parent.as_ref().unwrap().object,
            Some(42)
        );
        assert!(!panels.timeline_columns.editing());
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
}
#[test]
fn blend_dropdown_reads_mode_object_and_changes_only_mode_as_one_undo() {
    let mut engine = fixture();
    engine
        .apply(Command::SetLayerBlend {
            object: 7,
            mode: Some(BlendMode::Screen),
            space: Some(BlendSpace::Srgb),
        })
        .unwrap();
    let before = engine.project().clone();
    let state = snapshot(&engine);
    let mut panels = Panels {
        locale: Locale::Zh,
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    assert_eq!(
        timeline_columns::blend_name(&state["project"]["layers"][0], Locale::Zh),
        "屏幕"
    );
    assert!(draw(&mut panels, &state, click(460., 118.)).is_empty());
    let actions = draw(&mut panels, &state, click(460., 183.));
    assert!(matches!(
        actions.as_slice(),
        [Action::Edit(Command::SetLayerBlend {
            object: 7,
            mode: Some(BlendMode::Multiply),
            space: None
        })]
    ));
    for action in actions {
        if let Action::Edit(command) = action {
            engine.apply(command).unwrap();
        }
    }
    assert_eq!(engine.project().layers[0].blend.mode, BlendMode::Multiply);
    assert_eq!(engine.project().layers[0].blend.space, BlendSpace::Srgb);
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}
#[test]
fn popup_owns_keyboard_and_outside_click_and_reselecting_current_is_noop() {
    let engine = fixture();
    let state = snapshot(&engine);
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    assert!(draw(&mut panels, &state, click(560., 118.)).is_empty());
    let mut actions = vec![];
    keyboard::route(
        &Input {
            keys: vec![(Key::Delete, Default::default())],
            ..Default::default()
        },
        &mut panels,
        &state,
        false,
        None,
        &mut actions,
    );
    assert!(actions.is_empty());
    assert!(draw(&mut panels, &state, click(800., 74.)).is_empty());
    assert!(!panels.timeline_columns.editing());
    assert!(panels.timeline.moving.is_none());
    assert!(draw(&mut panels, &state, click(560., 118.)).is_empty());
    assert!(draw(&mut panels, &state, click(560., 139.)).is_empty());
    assert!(!panels.timeline_columns.editing());
    assert!(draw(&mut panels, &state, click(560., 118.)).is_empty());
    assert!(draw(
        &mut panels,
        &state,
        Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        }
    )
    .is_empty());
    assert!(!panels.timeline_columns.editing());
}
#[test]
fn parent_candidates_exclude_self_descendants_audio_and_camera_cycles() {
    let state = serde_json::json!({"project":{"camera":{"created":true,"parent":{"object":99}},"layers":[
        {"id":7,"name":"Child","content":{"kind":"solid"}},
        {"id":42,"name":"Same name","content":{"kind":"null"}},
        {"id":43,"name":"Same name","content":{"kind":"solid"}},
        {"id":99,"name":"Descendant","content":{"kind":"solid"},"parent":{"object":7}},
        {"id":100,"name":"Grandchild","content":{"kind":"solid"},"parent":{"object":99}},
        {"id":88,"name":"Music","content":{"kind":"audio"}}
    ]}});
    let choices = parent_controls::candidates(&state, 7, Locale::Zh);
    assert_eq!(
        choices.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![43, 42]
    );
    assert_ne!(choices[0].1, choices[1].1);
    let engine = fixture();
    let mut locked = snapshot(&engine);
    locked["project"]["layers"][0]["locked"] = Value::Bool(true);
    let mut panels = Panels::default();
    assert!(draw(&mut panels, &locked, click(560., 118.)).is_empty());
    assert!(!panels.timeline_columns.editing());
}

#[test]
fn parent_menu_keeps_disabled_self_and_descendants_and_keyboard_skips_them() {
    let engine = fixture();
    let state = snapshot(&engine);
    let entries = parent_controls::menu_entries(&state, 7, Locale::Zh);
    assert!(entries
        .iter()
        .any(|(id, _, enabled)| *id == Some(7) && !*enabled));
    assert!(entries
        .iter()
        .any(|(id, _, enabled)| *id == Some(99) && !*enabled));
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    assert!(draw(&mut panels, &state, click(560., 118.)).is_empty());
    assert!(draw(
        &mut panels,
        &state,
        Input {
            keys: vec![(Key::Down, Default::default())],
            ..Default::default()
        }
    )
    .is_empty());
    let actions = draw(
        &mut panels,
        &state,
        Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Edit(Command::Parent {
            object: 7,
            parent: Some(42),
            ..
        })]
    ));
}

#[test]
fn column_layout_keeps_historical_pixels_when_everything_is_visible() {
    let columns = timeline_columns::ColumnRects::new(0., 624., ColumnVisibility::default());
    assert_eq!(
        columns.matte,
        Some(timeline_columns::Slot {
            x: 344.,
            width: 78.
        })
    );
    assert_eq!(
        columns.blend,
        Some(timeline_columns::Slot {
            x: 428.,
            width: 76.
        })
    );
    assert_eq!(
        columns.link,
        Some(timeline_columns::Slot {
            x: 510.,
            width: 14.
        })
    );
    assert_eq!(
        columns.parent,
        Some(timeline_columns::Slot {
            x: 530.,
            width: 86.
        })
    );
    assert_eq!(
        columns.name,
        timeline_columns::Slot {
            x: 112.,
            width: 226.
        }
    );
}

#[test]
fn hidden_columns_stay_right_aligned_and_widen_the_name_column() {
    let base = ColumnVisibility::default();
    let cases = [
        ColumnVisibility { matte: false, ..base },
        ColumnVisibility { blend: false, ..base },
        ColumnVisibility { parent: false, ..base },
        ColumnVisibility {
            blend: false,
            matte: false,
            ..base
        },
        ColumnVisibility {
            blend: false,
            parent: false,
            matte: false,
        },
    ];
    for visibility in cases {
        let columns = timeline_columns::ColumnRects::new(0., 624., visibility);
        let slots: Vec<_> = [columns.matte, columns.blend, columns.link, columns.parent]
            .into_iter()
            .flatten()
            .collect();
        for pair in slots.windows(2) {
            assert_eq!(pair[1].x - pair[0].max_x(), 6.);
        }
        if let Some(first) = slots.first() {
            assert_eq!(columns.name.max_x() + 6., first.x);
            assert_eq!(slots.last().unwrap().max_x(), 616.);
        } else {
            assert_eq!(columns.name.max_x(), 616.);
        }
        assert!(columns.name.width > 226. || slots.is_empty());
    }
    let hidden = timeline_columns::ColumnRects::new(
        0.,
        624.,
        ColumnVisibility {
            blend: false,
            parent: false,
            matte: false,
        },
    );
    assert_eq!(
        hidden.name,
        timeline_columns::Slot {
            x: 112.,
            width: 504.
        }
    );
}

#[test]
fn matte_column_still_needs_room_for_its_header() {
    let narrow = timeline_columns::ColumnRects::new(0., 480., ColumnVisibility::default());
    assert!(narrow.matte.is_none());
    assert!(narrow.blend.is_some());
    assert!(narrow.parent.is_some());
    let roomy = timeline_columns::ColumnRects::new(0., 500., ColumnVisibility::default());
    assert_eq!(roomy.matte.unwrap().width, 78.);
}

#[test]
fn composite_and_hierarchy_groups_toggle_together() {
    let mut visibility = ColumnVisibility::default();
    visibility.toggle_group(ColumnGroup::Composite);
    assert_eq!(
        visibility,
        ColumnVisibility {
            blend: false,
            parent: true,
            matte: false
        }
    );
    visibility.toggle_group(ColumnGroup::Composite);
    assert_eq!(visibility, ColumnVisibility::default());
    visibility.toggle_group(ColumnGroup::Hierarchy);
    assert!(!visibility.parent);
    assert!(visibility.blend && visibility.matte);
}

#[test]
fn link_icon_opens_parent_menu_and_blend_popup_can_overlay_the_preview_safely() {
    let engine = fixture();
    let state = snapshot(&engine);
    let mut panels = Panels {
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    assert!(draw(&mut panels, &state, click(517., 118.)).is_empty());
    assert!(panels.timeline_columns.editing());
    panels.timeline_columns.cancel();
    let layout = Node::Split {
        edge: motion_ui::dock::Edge::Bottom,
        split: 0.3,
        first: Box::new(Node::dock(Panel::Composition.dock())),
        second: Box::new(Node::dock(Panel::Timeline.dock())),
    };
    let size = [1600., 1000.];
    let frame = frames(&layout, size)
        .into_iter()
        .find(|f| f.panel == Panel::Timeline)
        .unwrap();
    let point = [
        frame.content.min[0] + 640. - 158.,
        frame.content.min[1] + 64. + 44. + 11.,
    ];
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut paint = PaintList::default();
    assert!(chrome(
        &mut paint,
        &mut atlas,
        size,
        &layout,
        &mut panels,
        &state,
        &click(point[0], point[1])
    )
    .is_empty());
    let popup = paint
        .shapes
        .iter()
        .find_map(|shape| match shape {
            motion_ui::paint::Shape::Procedural {
                area,
                kind: motion_ui::paint::Pattern::Box { .. },
                ..
            } if area.height() > 300. && area.width() < 320. => Some(*area),
            _ => None,
        })
        .unwrap();
    assert!(
        popup.min[1] < frame.content.min[1],
        "menu must not be clipped to the track panel"
    );
    let actions = chrome(
        &mut PaintList::default(),
        &mut atlas,
        size,
        &layout,
        &mut panels,
        &state,
        &click(popup.min[0] + 30., popup.min[1] + 61.),
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Edit(Command::SetLayerBlend {
            object: 7,
            mode: Some(BlendMode::Multiply),
            ..
        })]
    ));
}


#[test]
fn review327_columns_translate_with_panel_origin() {
    for visibility in [ColumnVisibility::default(), ColumnVisibility { blend: false, parent: false, matte: false }] {
        let base = timeline_columns::ColumnRects::new(0., 624., visibility);
        let shifted = timeline_columns::ColumnRects::new(350., 624., visibility);
        assert_eq!(shifted.name.width, base.name.width, "moving a panel must preserve its name width");
        for (a,b) in [(base.matte, shifted.matte), (base.blend, shifted.blend), (base.link, shifted.link), (base.parent, shifted.parent)] {
            if let (Some(a), Some(b)) = (a,b) {
                assert_eq!(b.x, a.x + 350., "optional column must move with its panel");
            }
        }
    }
}
