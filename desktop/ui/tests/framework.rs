use motion_ui::{
    dock::{DockId, Edge, Node},
    input::Event,
    interaction::{HitRegion, Router, Target},
    registry::{Language, PanelDescriptor, PanelRegistry},
    widgets::{NumberEditor, NumberEvent, NumberOptions, TextEditor},
    workspace::Workspace,
    Input, Key, MouseButton, Rect,
};

#[test]
fn opened_number_text_can_scrub_again_without_losing_uncommitted_text_or_ime() {
    let rect = Rect::new(0., 0., 100., 30.);
    let options = NumberOptions::default();
    let mut field = NumberEditor::default();
    let click = Input {
        mouse: [10., 10.],
        pressed: Some((MouseButton::Left, [10., 10.])),
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    let release = Input {
        mouse: [10., 10.],
        released: Some((MouseButton::Left, [10., 10.])),
        ..Default::default()
    };
    assert!(field.update(25., options, &click, rect).is_empty());
    assert!(field.update(25., options, &release, rect).is_empty());
    assert!(field
        .update(
            25.,
            options,
            &Input {
                text: "32.5".into(),
                ..Default::default()
            },
            rect
        )
        .is_empty());
    // A click inside an open field must retain its pending typed value.
    assert!(field.update(25., options, &click, rect).is_empty());
    assert!(field.update(25., options, &release, rect).is_empty());
    assert_eq!(field.text.text, "32.5");
    assert!(field
        .update(
            25.,
            options,
            &Input {
                composing: true,
                ..click.clone()
            },
            rect
        )
        .is_empty());
    assert!(!field.dragging());
    assert!(field.update(25., options, &click, rect).is_empty());
    let events = field.update(
        25.,
        options,
        &Input {
            mouse: [30., 10.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        rect,
    );
    assert_eq!(
        events,
        vec![
            NumberEvent::Begin { value: 32.5 },
            NumberEvent::Change { value: 52.5 }
        ]
    );
    assert_eq!(
        field.update(
            52.5,
            options,
            &Input {
                focus_lost: true,
                ..Default::default()
            },
            rect
        ),
        vec![NumberEvent::Cancel { value: 32.5 }]
    );
    assert!(!field.interacting());
}

#[test]
fn coalesced_out_and_back_numeric_drag_never_enters_text_input() {
    use motion_ui::ui::accumulate;
    let mut input = Input::default();
    let mut number = NumberEditor::default();
    let area = Rect::new(0., 0., 100., 30.);
    for event in [
        Event::MouseMoved {
            position: [10., 10.],
        },
        Event::MousePressed {
            position: [10., 10.],
            button: MouseButton::Left,
        },
        Event::MouseMoved {
            position: [40., 10.],
        },
        Event::MouseMoved {
            position: [10., 10.],
        },
        Event::MouseReleased {
            position: [10., 10.],
            button: MouseButton::Left,
        },
    ] {
        accumulate(&mut input, event);
    }
    let events = number.update(25., NumberOptions::default(), &input, area);
    assert_eq!(
        events,
        vec![
            NumberEvent::Begin { value: 25. },
            NumberEvent::Commit { value: 25. }
        ]
    );
    assert!(!number.text.active());
    input.begin_frame();
    accumulate(
        &mut input,
        Event::MousePressed {
            position: [10., 10.],
            button: MouseButton::Left,
        },
    );
    accumulate(
        &mut input,
        Event::MouseReleased {
            position: [10., 10.],
            button: MouseButton::Left,
        },
    );
    assert!(number
        .update(25., NumberOptions::default(), &input, area)
        .is_empty());
    assert!(number.text.active());
}
fn region(window: u64, panel: u32, widget: u64, rect: Rect, layer: u32) -> HitRegion {
    HitRegion {
        target: Target {
            window,
            panel: DockId(panel),
            widget,
        },
        rect,
        clip: rect,
        layer,
        keyboard: true,
    }
}
#[test]
fn focus_capture_modal_and_windows_have_one_input_owner() {
    let a = region(1, 100, 1, Rect::new(0., 0., 100., 30.), 0);
    let b = region(1, 200, 1, Rect::new(110., 0., 100., 30.), 0);
    let other = region(2, 100, 1, a.rect, 0);
    let mut router = Router::default();
    let input = Input {
        mouse: [20., 10.],
        pressed: Some((MouseButton::Left, [20., 10.])),
        held: Some(MouseButton::Left),
        ..Default::default()
    };
    router.begin_frame(1, &input, &[a, b]);
    assert_eq!(router.focus(), Some(a.target));
    assert_eq!(router.captured(), Some(a.target));
    let input = Input {
        mouse: [170., 10.],
        held: Some(MouseButton::Left),
        text: "42".into(),
        ..Default::default()
    };
    router.begin_frame(1, &input, &[a, b]);
    assert_eq!(router.input_for(a.target, &input).text, "42");
    assert_eq!(
        router.input_for(a.target, &input).held,
        Some(MouseButton::Left)
    );
    assert!(router.input_for(b.target, &input).text.is_empty());
    assert!(router.input_for(other.target, &input).text.is_empty());
    router.set_modal(Some((1, DockId(200))));
    router.begin_frame(1, &input, &[a, b]);
    assert!(router.input_for(a.target, &input).focus_lost);
    assert!(router.input_for(a.target, &input).text.is_empty());
    assert_eq!(router.captured(), None);
    let click = Input {
        mouse: [170., 10.],
        pressed: Some((MouseButton::Left, [170., 10.])),
        ..Default::default()
    };
    router.begin_frame(1, &click, &[a, b]);
    assert_eq!(router.focus(), Some(b.target));
    router.begin_frame(
        1,
        &Input {
            focus_lost: true,
            ..Default::default()
        },
        &[a, b],
    );
    assert_eq!(router.focus(), None);
}
#[test]
fn clipping_and_tab_navigation_exclude_other_panels_and_windows() {
    let mut a = region(1, 100, 1, Rect::new(0., 0., 100., 100.), 0);
    a.clip = Rect::new(0., 0., 50., 50.);
    let b = region(1, 100, 2, Rect::new(110., 0., 100., 30.), 0);
    let mut router = Router::default();
    let click = Input {
        mouse: [80., 80.],
        pressed: Some((MouseButton::Left, [80., 80.])),
        ..Default::default()
    };
    router.begin_frame(1, &click, &[a, b]);
    assert_eq!(router.focus(), None);
    let tab = Input {
        keys: vec![(Key::Tab, Default::default())],
        ..Default::default()
    };
    router.begin_frame(1, &tab, &[a, b]);
    assert_eq!(router.focus(), Some(a.target));
    router.begin_frame(1, &tab, &[a, b]);
    assert_eq!(router.focus(), Some(b.target));
    assert_eq!(router.forget_panel(1, DockId(100)), None);
    assert_eq!(router.focus(), None);
}
#[test]
fn click_enter_keeps_full_precision_and_invalid_text_never_writes_zero() {
    let area = Rect::new(0., 0., 100., 30.);
    let mut field = NumberEditor::default();
    let value = 10.123456789;
    let click = Input {
        mouse: [10., 10.],
        pressed: Some((MouseButton::Left, [10., 10.])),
        ..Default::default()
    };
    assert!(field
        .update(value, NumberOptions::default(), &click, area)
        .is_empty());
    assert!(!field.text.active());
    field.update(
        value,
        NumberOptions::default(),
        &Input {
            mouse: [10., 10.],
            released: Some((MouseButton::Left, [10., 10.])),
            ..Default::default()
        },
        area,
    );
    assert_eq!(field.text.text, "10.12");
    let enter = Input {
        keys: vec![(Key::Enter, Default::default())],
        ..Default::default()
    };
    let events = field.update(value, NumberOptions::default(), &enter, area);
    assert_eq!(
        events,
        vec![NumberEvent::Begin { value }, NumberEvent::Commit { value }]
    );
    field.update(value, NumberOptions::default(), &click, area);
    field.update(
        value,
        NumberOptions::default(),
        &Input {
            mouse: [10., 10.],
            released: Some((MouseButton::Left, [10., 10.])),
            ..Default::default()
        },
        area,
    );
    let invalid = Input {
        text: "not a number".into(),
        keys: enter.keys,
        ..Default::default()
    };
    assert!(field
        .update(value, NumberOptions::default(), &invalid, area)
        .is_empty());
    assert!(field.text.error.is_some());
    assert!(field.text.active());
}
#[test]
fn number_drag_emits_one_begin_commit_and_cancel_restores_origin() {
    let area = Rect::new(0., 0., 100., 30.);
    let mut field = NumberEditor::default();
    let options = NumberOptions {
        speed: 0.5,
        ..Default::default()
    };
    field.update(
        25.,
        options,
        &Input {
            mouse: [10., 10.],
            pressed: Some((MouseButton::Left, [10., 10.])),
            ..Default::default()
        },
        area,
    );
    assert_eq!(
        field.update(
            25.,
            options,
            &Input {
                mouse: [30., 10.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            area
        ),
        vec![
            NumberEvent::Begin { value: 25. },
            NumberEvent::Change { value: 35. }
        ]
    );
    assert_eq!(
        field.update(
            35.,
            options,
            &Input {
                mouse: [110., 10.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            area
        ),
        vec![NumberEvent::Change { value: 75. }]
    );
    assert_eq!(
        field.update(
            75.,
            options,
            &Input {
                mouse: [110., 10.],
                released: Some((MouseButton::Left, [110., 10.])),
                ..Default::default()
            },
            area
        ),
        vec![NumberEvent::Commit { value: 75. }]
    );
    field.update(
        75.,
        options,
        &Input {
            mouse: [10., 10.],
            pressed: Some((MouseButton::Left, [10., 10.])),
            ..Default::default()
        },
        area,
    );
    field.update(
        75.,
        options,
        &Input {
            mouse: [30., 10.],
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        area,
    );
    assert_eq!(field.cancel(), vec![NumberEvent::Cancel { value: 75. }]);
    assert!(field.cancel().is_empty());
}
#[test]
fn unicode_cursor_and_selection_do_not_split_utf8_or_erase_on_focus() {
    let mut edit = TextEditor::default();
    edit.begin("甲🙂乙");
    assert_eq!(edit.text, "甲🙂乙");
    edit.input(&Input {
        keys: vec![
            (Key::Left, Default::default()),
            (Key::Backspace, Default::default()),
        ],
        ..Default::default()
    });
    assert_eq!(edit.text, "甲乙");
    edit.input(&Input {
        text: "🌊".into(),
        ..Default::default()
    });
    assert_eq!(edit.text, "甲🌊乙");
    edit.begin("unchanged");
    edit.input(&Input {
        focus_lost: true,
        ..Default::default()
    });
    assert_eq!(edit.text, "unchanged");
    assert!(!edit.active());
}
#[test]
fn custom_panel_ids_round_trip_and_bad_drops_keep_ownership() {
    let mut registry = PanelRegistry::default();
    for id in [100, 200, 300] {
        registry
            .register(PanelDescriptor::new(DockId(id), "面板", "Panel"))
            .unwrap();
    }
    assert!(registry
        .register(PanelDescriptor::new(DockId(100), "重复", "Duplicate"))
        .is_err());
    assert_eq!(
        registry
            .get(DockId(100))
            .unwrap()
            .title
            .get(Language::Chinese),
        "面板"
    );
    let root = Node::Split {
        edge: Edge::Left,
        split: 0.5,
        first: Box::new(Node::dock(DockId(100))),
        second: Box::new(Node::dock(DockId(200))),
    };
    let mut workspace = Workspace::from_root(root);
    assert!(workspace.valid_for(&registry));
    assert!(workspace.float(DockId(100)));
    let before = serde_json::to_value(&workspace).unwrap();
    assert!(!workspace.redock_to(DockId(100), DockId(999), None));
    assert_eq!(serde_json::to_value(&workspace).unwrap(), before);
    assert!(workspace.redock_to(DockId(100), DockId(200), None));
    assert!(workspace.close(DockId(100)));
    assert!(workspace.valid_for(&registry));
    let mut reopened: Workspace =
        serde_json::from_value(serde_json::to_value(&workspace).unwrap()).unwrap();
    reopened.redock(DockId(100));
    assert!(reopened.valid_for(&registry));
    assert_eq!(reopened.owned_panels().len(), 2);
}
#[test]
fn modal_dismissal_is_a_request_and_preserves_the_layer_until_host_accepts() {
    use motion_ui::overlay::{DismissReason, Kind, Overlay, Overlays};
    let mut overlays = Overlays::default();
    overlays.open(Overlay {
        id: 7,
        window: 1,
        area: Rect::new(10., 10., 100., 100.),
        kind: Kind::Modal,
    });
    assert!(overlays.blocks_background(2));
    let outside = Input {
        pressed: Some((MouseButton::Left, [0., 0.])),
        ..Default::default()
    };
    assert_eq!(overlays.dismiss_request(1, &outside), None);
    let escape = Input {
        keys: vec![(Key::Escape, Default::default())],
        ..Default::default()
    };
    assert_eq!(
        overlays.dismiss_request(1, &escape),
        Some((7, DismissReason::Escape))
    );
    assert_eq!(overlays.top(1).unwrap().id, 7);
    overlays.close(7);
    assert!(!overlays.blocks_background(2));
    overlays.open(Overlay {
        id: 8,
        window: 1,
        area: Rect::new(10., 10., 100., 100.),
        kind: Kind::Popup,
    });
    assert_eq!(
        overlays.dismiss_request(1, &outside),
        Some((8, DismissReason::OutsidePress))
    );
}
#[test]
fn tiny_docks_do_not_panic_and_number_drag_respects_modifier_and_range() {
    let small = Rect::new(0., 0., 12., 10.);
    assert!(dock_ratio(Edge::Top, small).is_finite());
    assert!(dock_ratio(Edge::Left, small).is_finite());
    let mut editor = NumberEditor::default();
    let area = Rect::new(0., 0., 100., 30.);
    let options = NumberOptions {
        speed: 1.,
        min: 0.,
        max: 26.,
        decimals: 2,
    };
    editor.update(
        25.,
        options,
        &Input {
            mouse: [10., 10.],
            pressed: Some((MouseButton::Left, [10., 10.])),
            ..Default::default()
        },
        area,
    );
    let events = editor.update(
        25.,
        options,
        &Input {
            mouse: [30., 10.],
            held: Some(MouseButton::Left),
            modifiers: motion_ui::input::Modifiers {
                shift: true,
                ..Default::default()
            },
            ..Default::default()
        },
        area,
    );
    assert_eq!(
        events,
        vec![
            NumberEvent::Begin { value: 25. },
            NumberEvent::Change { value: 26. }
        ]
    );
    assert_eq!(
        editor.update(
            26.,
            options,
            &Input {
                focus_lost: true,
                ..Default::default()
            },
            area
        ),
        vec![NumberEvent::Cancel { value: 25. }]
    );
}
fn dock_ratio(edge: Edge, area: Rect) -> f32 {
    motion_ui::dock::clamp_split(edge, 0., area)
}
