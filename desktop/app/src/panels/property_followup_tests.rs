//! Real desktop commands, persisted projects and both native window routes.
use super::*;
use motion_ui::input::Modifiers;

fn draw(
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    panel: Panel,
    floating: bool,
    atlas: &mut TextAtlas,
) -> Vec<Action> {
    if floating {
        floating_chrome(
            &mut PaintList::default(),
            atlas,
            panel,
            [1000., 800.],
            panels,
            state,
            input,
        )
    } else {
        chrome(
            &mut PaintList::default(),
            atlas,
            [1000., 800.],
            &dock::Node::dock(panel.dock()),
            panels,
            state,
            input,
        )
    }
}

fn click(rect: Rect, button: MouseButton) -> Input {
    let point = [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    Input {
        mouse: point,
        pressed: Some((button, point)),
        ..Default::default()
    }
}

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Modifiers::default())],
        ..Default::default()
    }
}

fn command(actions: Vec<Action>) -> Command {
    let mut edits = actions.into_iter().filter_map(|action| match action {
        Action::Edit(command) => Some(command),
        _ => None,
    });
    let command = edits.next().expect("one command from the actual UI");
    assert!(edits.next().is_none());
    command
}

fn project() -> motion_core::Project {
    let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        7, "Property", [100.; 2], [0.; 3], [1.; 4],
    ));
    project
}

#[test]
fn narrow_timeline_3d_click_does_not_open_parent_menu() {
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &project()).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let state = host.state().unwrap();
    let mut panels = Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut render = |panels: &mut Panels, input: &Input| {
        floating_chrome(
            &mut PaintList::default(),
            &mut atlas,
            Panel::Timeline,
            [360., 700.],
            panels,
            &state,
            input,
        )
    };
    render(&mut panels, &Input::default());
    let styles = PaintList::default().styles();
    let gutter = GUTTER.min(360. * 0.52);
    let columns = layer_switches::columns(&styles, Rect::new(0., 30. + 64., gutter, 22.), gutter);
    let actions = render(&mut panels, &click(columns.three_d, MouseButton::Left));
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Edit(Command::SetLayer3d {
            object: 7,
            enabled: true
        })
    )));
    assert!(
        !panels.timeline_columns.editing(),
        "the 3D checkbox click must not also open Parent & Link"
    );
}

#[test]
fn timeline_switch_columns_stay_disjoint_and_route_edits_through_resize() {
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &project()).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let initial = host.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for height in [22., 32.] {
                let styles = std::sync::Arc::new(
                    PaintList::default()
                        .styles()
                        .with_overrides(
                            serde_json::json!({"tokens":{"sizes":{"row.height":height}}})
                                .to_string()
                                .as_bytes(),
                        )
                        .unwrap(),
                );
                for width in [360., 500., 720., 1280.] {
                    let mut panels = Panels {
                        selected: Some(7),
                        focused: Some(Panel::Timeline),
                        locale,
                        styles: Some(styles.clone()),
                        ..Default::default()
                    };
                    let mut render =
                        |panels: &mut Panels, state: &Value, input: &Input, width: f32| {
                            if floating {
                                floating_chrome(
                                    &mut PaintList::default(),
                                    &mut atlas,
                                    Panel::Timeline,
                                    [width, 700.],
                                    panels,
                                    state,
                                    input,
                                )
                            } else {
                                chrome(
                                    &mut PaintList::default(),
                                    &mut atlas,
                                    [width, 700.],
                                    &dock::Node::dock(Panel::Timeline.dock()),
                                    panels,
                                    state,
                                    input,
                                )
                            }
                        };
                    render(&mut panels, &initial, &Input::default(), width);
                    let top = if floating {
                        30.
                    } else {
                        frames(&dock::Node::dock(Panel::Timeline.dock()), [width, 700.])[0]
                            .content
                            .min[1]
                    };
                    let gutter = GUTTER.min(width * 0.52);
                    let columns = layer_switches::columns(
                        &styles,
                        Rect::new(0., top + 64., gutter, height),
                        gutter,
                    );
                    assert!(columns.three_d_visible);
                    assert!(columns.name.width() > 0.);
                    assert!(columns.name.max[0] <= columns.three_d.min[0]);
                    assert!(columns.three_d.max[0] <= gutter);
                    assert_eq!(columns.three_d.width(), styles.metric("icon.slot", 20.));
                    if columns.parent_visible {
                        assert!(columns.three_d.max[0] <= gutter - 114.);
                    }
                    if columns.blend_visible {
                        assert!(columns.three_d.max[0] <= gutter - 196.);
                    }
                    let action = command(render(
                        &mut panels,
                        &initial,
                        &click(columns.three_d, MouseButton::Left),
                        width,
                    ));
                    assert!(matches!(
                        action,
                        Command::SetLayer3d {
                            object: 7,
                            enabled: true
                        }
                    ));
                    assert!(
                        !panels.timeline_columns.editing(),
                        "3D must not open a second control"
                    );
                    let changed = host.edit(serde_json::to_string(&action).unwrap()).unwrap();
                    host.save().unwrap();
                    assert!(motion_core::storage::load(root.path()).unwrap().layers[0].three_d);
                    assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
                    assert_eq!(host.history(1).unwrap()["project"], changed["project"]);
                    host.history(0).unwrap();
                    for observing in [false, true] {
                        // Lock through the real engine so the revision-keyed row cache
                        // sees an actual model change, not a mutated stale snapshot.
                        // A preview camera no longer disables the dimension switch;
                        // locked layers must remain protected in every view.
                        let mut blocked = host.edit(
                                serde_json::to_string(&Command::Flags {
                                    object: 7,
                                    visible: true,
                                    locked: true,
                                })
                                .unwrap(),
                            )
                            .unwrap();
                        blocked["observing"] = observing.into();
                        let actions = render(
                            &mut panels,
                            &blocked,
                            &click(columns.three_d, MouseButton::Left),
                            width,
                        );
                        assert!(!actions.iter().any(|a| matches!(a,Action::Edit(_) | Action::EditBatch(_))),
                            "blocked click: floating={floating} locale={locale:?} height={height} width={width} observing={observing}: {actions:?}");
                        assert!(!panels.timeline_columns.editing());
                        host.history(0).unwrap();
                    }
                    let parent = Rect::new(gutter - 94., top + 66., 86., height - 4.);
                    let actions = render(
                        &mut panels,
                        &initial,
                        &click(parent, MouseButton::Left),
                        width,
                    );
                    assert!(!actions
                        .iter()
                        .any(|a| matches!(a, Action::Edit(_) | Action::EditBatch(_))));
                    assert_eq!(panels.timeline_columns.editing(), columns.parent_visible);
                    if columns.parent_visible {
                        let resized = render(&mut panels, &initial, &Input::default(), 360.);
                        assert!(!resized
                            .iter()
                            .any(|a| matches!(a, Action::Edit(_) | Action::EditBatch(_))));
                        assert!(
                            !panels.timeline_columns.editing(),
                            "a hidden parent cell must close its popup"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(host.state().unwrap()["project"], initial["project"]);
}

#[test]
fn timeline_mask_header_actions_and_add_menu_preserve_targets_locking_and_undo() {
    use motion_core::{
        masks::{LayerMask, MaskAction},
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut project = project();
    let mut mask = LayerMask::new(
        11,
        VectorPath {
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
        },
    );
    mask.name = "Long mask name / 很长的蒙版名称，不得覆盖操作区".into();
    project.layers[0].masks.push(mask);
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &project).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for width in [360., 720., 1000.] {
                for height in [22., 32.] {
                    let before = host.state().unwrap();
                    let styles = std::sync::Arc::new(
                        PaintList::default()
                            .styles()
                            .with_overrides(
                                serde_json::json!({"tokens":{"sizes":{"row.height":height}}})
                                    .to_string()
                                    .as_bytes(),
                            )
                            .unwrap(),
                    );
                    let mut panels = Panels {
                        focused: Some(Panel::Timeline),
                        locale,
                        styles: Some(styles.clone()),
                        ..Default::default()
                    };
                    panels.timeline.expanded.insert(7);
                    panels.timeline.collapsed_transforms.insert(7);
                    let layout = dock::Node::dock(Panel::Timeline.dock());
                    let top = if floating {
                        30.
                    } else {
                        frames(&layout, [width, 800.])[0].content.min[1]
                    };
                    let mut render = |panels: &mut Panels, state: &Value, input: &Input| {
                        let mut paint = PaintList::default();
                        if floating {
                            floating_chrome(
                                &mut paint,
                                &mut atlas,
                                Panel::Timeline,
                                [width, 800.],
                                panels,
                                state,
                                input,
                            )
                        } else {
                            chrome(
                                &mut paint,
                                &mut atlas,
                                [width, 800.],
                                &layout,
                                panels,
                                state,
                                input,
                            )
                        }
                    };
                    assert!(render(&mut panels, &before, &Input::default()).is_empty());
                    let gutter = GUTTER.min(width * 0.52);
                    let area = Rect::new(0., top, gutter, 800. - top);
                    let rects = path_controls::mask_header::action_rects(
                        &styles,
                        area,
                        top + 64. + height * 2.,
                        height,
                    );
                    assert!(rects
                        .windows(2)
                        .all(|pair| pair[0].max[0] <= pair[1].min[0]));
                    assert!(rects[3].max[0] < gutter);
                    let heading = timeline::property_group_reserved(
                        &mut PaintList::default(),
                        &mut TextAtlas::from_system(12.).unwrap(),
                        Rect::new(
                            styles.metric("tree.indent", 14.),
                            top,
                            gutter - styles.metric("tree.indent", 14.),
                            800. - top,
                        ),
                        rects[0].min[1],
                        height,
                        "Long mask name",
                        true,
                        styles.metric("icon.slot", 20.) * 4. + styles.metric("padding.panel", 8.),
                    );
                    assert!(heading.max[0] <= rects[0].min[0]);
                    let add = panels.path_controls.mask_add_rect(Panel::Timeline).unwrap();

                    let edit = command(render(
                        &mut panels,
                        &before,
                        &click(rects[0], MouseButton::Left),
                    ));
                    assert!(matches!(
                        edit,
                        Command::Mask {
                            object: 7,
                            action: MaskAction::Options {
                                mask: 11,
                                enabled: Some(false),
                                ..
                            }
                        }
                    ));
                    let disabled = host.edit(serde_json::to_string(&edit).unwrap()).unwrap();
                    assert_eq!(
                        disabled["project"]["layers"][0]["masks"][0]["enabled"],
                        false
                    );
                    assert!(!panels.timeline.collapsed_mask_items.contains(&(7, 11)));
                    host.save().unwrap();
                    assert!(
                        !motion_core::storage::load(root.path()).unwrap().layers[0].masks[0]
                            .enabled
                    );
                    assert_eq!(host.history(0).unwrap()["project"], before["project"]);

                    assert!(
                        render(&mut panels, &before, &click(rects[1], MouseButton::Left))
                            .is_empty()
                    );
                    assert_eq!(panels.selected, Some(7));
                    assert_eq!(panels.path_controls.tool, path_controls::Tool::Mask);
                    assert_eq!(panels.path_controls.mask, Some(11));
                    let actions = render(&mut panels, &before, &click(rects[2], MouseButton::Left));
                    assert!(matches!(
                        actions.as_slice(),
                        [Action::MaskTimeline {
                            object: 7,
                            mask: 11
                        }]
                    ));
                    assert_eq!(host.state().unwrap()["project"], before["project"]);

                    let edit = command(render(
                        &mut panels,
                        &before,
                        &click(rects[3], MouseButton::Left),
                    ));
                    assert!(matches!(
                        edit,
                        Command::Mask {
                            object: 7,
                            action: MaskAction::Remove { mask: 11 }
                        }
                    ));
                    let removed = host.edit(serde_json::to_string(&edit).unwrap()).unwrap();
                    let removed_project: motion_core::Project =
                        serde_json::from_value(removed["project"].clone()).unwrap();
                    assert!(removed_project.layers[0].masks.is_empty());
                    host.save().unwrap();
                    assert!(motion_core::storage::load(root.path()).unwrap().layers[0]
                        .masks
                        .is_empty());
                    assert_eq!(host.history(0).unwrap()["project"], before["project"]);

                    for observing in [false, true] {
                        let mut blocked = before.clone();
                        blocked["observing"] = Value::Bool(observing);
                        blocked["project"]["layers"][0]["locked"] = Value::Bool(!observing);
                        blocked["revision"] = Value::from(
                            before["revision"].as_u64().unwrap() + 1 + u64::from(observing),
                        );
                        panels.path_controls.tool = path_controls::Tool::Select;
                        for rect in rects.into_iter().chain([add]) {
                            assert!(
                                render(&mut panels, &blocked, &click(rect, MouseButton::Left))
                                    .is_empty()
                            );
                        }
                        assert!(!panels.path_controls.mask_menu());
                        assert_eq!(panels.path_controls.tool, path_controls::Tool::Select);
                    }

                    assert!(render(&mut panels, &before, &click(add, MouseButton::Left)).is_empty());
                    assert!(panels.path_controls.mask_menu());
                    // Dismiss over a stopwatch: the obscured row must not consume the click.
                    assert!(render(
                        &mut panels,
                        &before,
                        &click(
                            Rect::new(126., add.max[1] + height, 14., height),
                            MouseButton::Left
                        )
                    )
                    .is_empty());
                    assert!(!panels.path_controls.mask_menu());
                    assert!(render(&mut panels, &before, &click(add, MouseButton::Left)).is_empty());
                    assert!(render(&mut panels, &before, &key(Key::Escape)).is_empty());
                    assert!(!panels.path_controls.mask_menu());

                    assert!(render(&mut panels, &before, &click(add, MouseButton::Left)).is_empty());
                    assert!(render(&mut panels, &before, &key(Key::Down)).is_empty());
                    let edit = command(render(&mut panels, &before, &key(Key::Enter)));
                    assert!(
                        matches!(&edit,Command::Mask { object:7,action:MaskAction::Add { mask } } if mask.id==12)
                    );
                    let added = host.edit(serde_json::to_string(&edit).unwrap()).unwrap();
                    assert_eq!(
                        added["project"]["layers"][0]["masks"]
                            .as_array()
                            .unwrap()
                            .len(),
                        2
                    );
                    host.save().unwrap();
                    assert_eq!(
                        motion_core::storage::load(root.path()).unwrap().layers[0]
                            .masks
                            .len(),
                        2
                    );
                    assert_eq!(host.history(0).unwrap()["project"], before["project"]);

                    assert!(render(&mut panels, &before, &click(add, MouseButton::Left)).is_empty());
                    assert!(render(&mut panels, &before, &key(Key::Enter)).is_empty());
                    assert_eq!(panels.path_controls.tool, path_controls::Tool::Mask);
                    assert_eq!(panels.path_controls.mask, None);
                    assert!(!panels.path_controls.mask_menu());
                    assert!(!panels.timeline.collapsed_mask_items.contains(&(7, 11)));

                    assert!(render(&mut panels, &before, &click(add, MouseButton::Left)).is_empty());
                    if floating {
                        // A dismissing click must not redock the floating timeline.
                        assert!(render(
                            &mut panels,
                            &before,
                            &click(Rect::new(width - 78., 4., 72., 22.), MouseButton::Left,)
                        )
                        .is_empty());
                    } else {
                        let split = dock::Node::Split {
                            edge: dock::Edge::Left,
                            split: 0.7,
                            first: Box::new(dock::Node::dock(Panel::Timeline.dock())),
                            second: Box::new(dock::Node::dock(Panel::Properties.dock())),
                        };
                        let other = frames(&split, [width, 800.])
                            .into_iter()
                            .find(|f| f.panel == Panel::Properties)
                            .unwrap();
                        let obscured_toggle = path_controls::mask_header::action_rects(
                            &styles,
                            other.content,
                            other.content.min[1] + styles.metric("padding.panel", 8.) + height * 2.,
                            height,
                        )[0];
                        let mut split_atlas = TextAtlas::from_system(12.).unwrap();
                        assert!(chrome(
                            &mut PaintList::default(),
                            &mut split_atlas,
                            [width, 800.],
                            &split,
                            &mut panels,
                            &before,
                            &Input::default()
                        )
                        .is_empty());
                        assert!(
                            panels.path_controls.mask_menu(),
                            "inspector initialization must preserve the timeline-owned menu"
                        );
                        // Keep the closing frame modal even when the other panel is drawn later.
                        assert!(chrome(
                            &mut PaintList::default(),
                            &mut split_atlas,
                            [width, 800.],
                            &split,
                            &mut panels,
                            &before,
                            &click(obscured_toggle, MouseButton::Left)
                        )
                        .is_empty());
                        assert_eq!(panels.focused, Some(Panel::Timeline));
                    }
                    assert!(!panels.path_controls.mask_menu(), "dismissal: floating={floating} width={width} height={height} locale={locale:?}");
                    assert_eq!(host.state().unwrap()["project"], before["project"]);
                }
            }
        }
    }
}

#[test]
fn masks_precede_transforms_and_layer_row_3d_is_undoable_in_both_windows() {
    use motion_core::{
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut project = project();
    project.layers[0].masks.push(LayerMask::new(
        1,
        VectorPath {
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
        },
    ));
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &project).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for height in [22., 32.] {
                let before = host.state().unwrap();
                let styles = PaintList::default()
                    .styles()
                    .with_overrides(
                        serde_json::json!({"tokens":{"sizes":{"row.height":height}}})
                            .to_string()
                            .as_bytes(),
                    )
                    .unwrap();
                let styles = std::sync::Arc::new(styles);
                let mut panels = Panels {
                    selected: Some(7),
                    focused: Some(Panel::Properties),
                    locale,
                    styles: Some(styles.clone()),
                    ..Default::default()
                };
                panels.properties.transform_collapsed = true;
                panels.path_controls.masks_open = true;
                assert!(draw(
                    &mut panels,
                    &before,
                    &Input::default(),
                    Panel::Properties,
                    floating,
                    &mut atlas
                )
                .is_empty());
                let top = |panel: Panel| {
                    if floating {
                        30.
                    } else {
                        frames(&dock::Node::dock(panel.dock()), [1000., 800.])[0]
                            .content
                            .min[1]
                    }
                };
                let heading = panels
                    .path_controls
                    .mask_add_rect(Panel::Properties)
                    .expect("Masks is a layer group");
                assert_eq!(
                    heading.min[1],
                    top(Panel::Properties) + styles.metric("padding.panel", 8.) + height,
                    "Masks must be immediately below the layer title, before Transform"
                );
                assert!(panels.path_controls.inspector_modes.contains_key(&(7, 1)));
                let toggle = click(
                    Rect::new(40., heading.min[1], 20., height),
                    MouseButton::Left,
                );
                assert!(draw(
                    &mut panels,
                    &before,
                    &toggle,
                    Panel::Properties,
                    floating,
                    &mut atlas
                )
                .is_empty());
                assert!(!panels.path_controls.masks_open && panels.properties.transform_collapsed);
                panels.focused = Some(Panel::Timeline);
                panels.timeline.expanded.insert(7);
                panels.timeline.collapsed_transforms.insert(7);
                let rows = timeline::rows(&before);
                let plan =
                    super::timeline_layout::LayerRows::new(&rows[0], &panels.timeline, 0., locale);
                assert_eq!(plan.masks.start, 1);
                assert!(plan.masks.len() > 1);
                assert_eq!(plan.masks.end, plan.transform.start);
                let gutter = GUTTER.min(1000. * 0.52);
                let columns = super::layer_switches::columns(
                    &styles,
                    Rect::new(0., top(Panel::Timeline) + 64., gutter, height),
                    gutter,
                );
                let edit = command(draw(
                    &mut panels,
                    &before,
                    &click(columns.three_d, MouseButton::Left),
                    Panel::Timeline,
                    floating,
                    &mut atlas,
                ));
                assert!(matches!(
                    edit,
                    Command::SetLayer3d {
                        object: 7,
                        enabled: true
                    }
                ));
                let after = host.edit(serde_json::to_string(&edit).unwrap()).unwrap();
                assert_eq!(after["project"]["layers"][0]["three_d"], true);
                host.save().unwrap();
                assert!(motion_core::storage::load(root.path()).unwrap().layers[0].three_d);
                let locked = host.edit(serde_json::to_string(&Command::Flags {
                    object: 7, visible: true, locked: true,
                }).unwrap()).unwrap();
                for observing in [false, true] {
                    let mut blocked = locked.clone();
                    blocked["observing"] = Value::Bool(observing);
                    assert!(draw(
                        &mut panels,
                        &blocked,
                        &click(columns.three_d, MouseButton::Left),
                        Panel::Timeline,
                        floating,
                        &mut atlas
                    )
                    .is_empty());
                }
                assert_eq!(host.history(0).unwrap()["project"], after["project"]);
                assert_eq!(host.history(0).unwrap()["project"], before["project"]);
            }
        }
    }
}

#[test]
fn dimension_context_merges_and_rejects_without_losing_axes_in_both_windows() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for panel in [Panel::Properties, Panel::Timeline] {
                let root = tempfile::tempdir().unwrap();
                let mut editor = motion_core::Engine::new(project()).unwrap();
                editor
                    .apply(Command::Animate {
                        object: 7,
                        property: Property::Position,
                        axis: None,
                        frame: 0,
                        enabled: true,
                    })
                    .unwrap();
                editor
                    .apply(Command::SetVector {
                        object: 7,
                        property: Property::Position,
                        frame: 12,
                        value: [120., 240., 360.],
                    })
                    .unwrap();
                editor
                    .apply(Command::SeparateDimensions {
                        object: 7,
                        property: Property::Position,
                    })
                    .unwrap();
                motion_core::storage::save(root.path(), editor.project()).unwrap();
                let host = crate::engine::Engine::start(
                    root.path().into(),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let initial = host.state().unwrap();
                let mut panels = Panels {
                    selected: Some(7),
                    focused: Some(panel),
                    locale,
                    ..Default::default()
                };
                panels.timeline.expanded.insert(7);
                let mut choose_merge = |state: &Value, panels: &mut Panels| {
                    draw(
                        panels,
                        state,
                        &Input::default(),
                        panel,
                        floating,
                        &mut atlas,
                    );
                    let rect = panels
                        .expressions
                        .triggers
                        .iter()
                        .find(|(target, _)| {
                            matches!(
                                target,
                                motion_core::ExpressionTarget::Property {
                                    property: Property::Position,
                                    ..
                                }
                            )
                        })
                        .unwrap()
                        .1;
                    assert!(draw(
                        panels,
                        state,
                        &click(rect, MouseButton::Right),
                        panel,
                        floating,
                        &mut atlas
                    )
                    .is_empty());
                    draw(panels, state, &key(Key::Down), panel, floating, &mut atlas);
                    command(draw(
                        panels,
                        state,
                        &key(Key::Enter),
                        panel,
                        floating,
                        &mut atlas,
                    ))
                };
                let merge = choose_merge(&initial, &mut panels);
                assert!(matches!(
                    merge,
                    Command::MergeDimensions {
                        object: 7,
                        property: Property::Position
                    }
                ));
                let merged = host.edit(serde_json::to_string(&merge).unwrap()).unwrap();
                assert_eq!(
                    merged["timeline_layers"][0]["properties"]["position"]["separated"],
                    false
                );
                host.save().unwrap();
                assert!(motion_core::storage::load(root.path()).unwrap().layers[0]
                    .transform
                    .position
                    .axes
                    .is_none());
                assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
                assert_eq!(host.history(1).unwrap()["project"], merged["project"]);
                host.history(0).unwrap();
                let mismatched = host
                    .edit(
                        serde_json::to_string(&Command::SetComponent {
                            object: 7,
                            property: Property::Position,
                            axis: motion_core::Axis::X,
                            frame: 9,
                            value: 90.,
                        })
                        .unwrap(),
                    )
                    .unwrap();
                let merge = choose_merge(&mismatched, &mut panels);
                let reason = host
                    .edit(serde_json::to_string(&merge).unwrap())
                    .unwrap_err();
                assert!(reason.contains("matching key times"), "{reason}");
                let after = host.state().unwrap();
                assert_eq!(after["project"], mismatched["project"]);
                assert_eq!(after["revision"], mismatched["revision"]);
                assert_eq!(
                    host.history(0).unwrap()["project"],
                    initial["project"],
                    "rejection adds no undo step"
                );
            }
        }
    }
}

#[test]
fn separated_axis_inline_editor_preserves_other_axes_and_round_trips() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for panel in [Panel::Properties, Panel::Timeline] {
                let root = tempfile::tempdir().unwrap();
                let mut editor = motion_core::Engine::new(project()).unwrap();
                editor
                    .apply(Command::SeparateDimensions {
                        object: 7,
                        property: Property::Position,
                    })
                    .unwrap();
                let y_expression = motion_core::PropertyExpression {
                    target: motion_core::ExpressionTarget::Property {
                        object: 7,
                        property: Property::Position,
                        axis: Some(motion_core::Axis::Y),
                    },
                    source: "20".into(),
                    enabled: true,
                    seed: 71,
                    profile: motion_core::EXPRESSION_PROFILE.into(),
                };
                editor
                    .apply(Command::SetExpression {
                        expression: y_expression.clone(),
                        frame: 0,
                    })
                    .unwrap();
                motion_core::storage::save(root.path(), editor.project()).unwrap();
                let host = crate::engine::Engine::start(
                    root.path().into(),
                    motion_desktop_media::platform(),
                    8 << 30,
                )
                .unwrap();
                let initial = host.state().unwrap();
                let mut panels = Panels {
                    selected: Some(7),
                    focused: Some(panel),
                    locale,
                    ..Default::default()
                };
                panels.timeline.expanded.insert(7);
                draw(
                    &mut panels,
                    &initial,
                    &Input::default(),
                    panel,
                    floating,
                    &mut atlas,
                );
                assert!(panels.expressions.rects.is_empty());
                let target = motion_core::ExpressionTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: Some(motion_core::Axis::X),
                };
                let rect = panels
                    .expressions
                    .triggers
                    .iter()
                    .find(|(t, _)| t == &target)
                    .expect("the real row has an axis target")
                    .1;
                draw(
                    &mut panels,
                    &initial,
                    &click(rect, MouseButton::Right),
                    panel,
                    floating,
                    &mut atlas,
                );
                assert!(draw(
                    &mut panels,
                    &initial,
                    &key(Key::Enter),
                    panel,
                    floating,
                    &mut atlas
                )
                .is_empty());
                assert_eq!(panels.expressions.draft, Some(target.clone()));
                draw(
                    &mut panels,
                    &initial,
                    &Input {
                        keys: vec![(
                            Key::A,
                            Modifiers {
                                control: true,
                                ..Default::default()
                            },
                        )],
                        ..Default::default()
                    },
                    panel,
                    floating,
                    &mut atlas,
                );
                draw(
                    &mut panels,
                    &initial,
                    &Input {
                        text: "123".into(),
                        ..Default::default()
                    },
                    panel,
                    floating,
                    &mut atlas,
                );
                let edit = command(draw(
                    &mut panels,
                    &initial,
                    &key(Key::Enter),
                    panel,
                    floating,
                    &mut atlas,
                ));
                assert!(
                    matches!(&edit, Command::SetExpression { expression, .. } if expression.target == target && expression.source == "123")
                );
                let after = host.edit(serde_json::to_string(&edit).unwrap()).unwrap();
                assert_eq!(
                    after["sampledLayers"][0]["position"],
                    serde_json::json!([123., 20., 0.])
                );
                assert_eq!(
                    super::expressions::find(&after, &y_expression.target),
                    Some(y_expression)
                );
                assert_eq!(
                    after["project"]["layers"], initial["project"]["layers"],
                    "expressions must not rewrite authored tracks"
                );
                host.save().unwrap();
                let saved = motion_core::storage::load(root.path()).unwrap();
                assert_eq!(serde_json::to_value(saved).unwrap(), after["project"]);
                assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
                assert_eq!(host.history(1).unwrap()["project"], after["project"]);
                draw(
                    &mut panels,
                    &after,
                    &key(Key::Escape),
                    panel,
                    floating,
                    &mut atlas,
                );
                assert!(panels.expressions.draft.is_none());
                assert_eq!(host.state().unwrap()["project"], after["project"]);
            }
        }
    }
}

#[test]
fn layer_menu_lock_and_unlock_all_are_atomic_and_preserve_visibility() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let root = tempfile::tempdir().unwrap();
        let mut project = project();
        let mut second = motion_core::Layer::solid(9, "Hidden", [100.; 2], [0.; 3], [1.; 4]);
        second.visible = false;
        project.layers.push(second);
        motion_core::storage::save(root.path(), &project).unwrap();
        let host = crate::engine::Engine::start(
            root.path().into(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let initial = host.state().unwrap();
        let mut panels = Panels {
            selected: Some(7),
            selected_layers: vec![7, 9, 999],
            locale,
            ..Default::default()
        };
        // Exercise the actual Layer menu, including batch construction and disabled no-ops.
        let mut choose = |panels: &mut Panels, state: &Value, index: usize| {
            panels.menu = Some(3);
            let point = [
                280.,
                metrics::TITLE_BAR + metrics::MENU_BAR + 4. + index as f32 * 32. + 15.,
            ];
            let mut out = vec![];
            super::menus::render(
                &mut PaintList::default(),
                &mut atlas,
                [1200., 900.],
                panels,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
                state,
                &mut out,
            );
            out
        };
        let actions = choose(&mut panels, &initial, 10);
        let [Action::EditBatch(commands)] = actions.as_slice() else {
            panic!("lock menu batch");
        };
        assert_eq!(commands.len(), 2);
        let locked = host.edit(serde_json::to_string(commands).unwrap()).unwrap();
        assert!(locked["project"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l["locked"] == true));
        assert_eq!(locked["project"]["layers"][1]["visible"], false);
        assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
        host.history(1).unwrap();
        let actions = choose(&mut panels, &locked, 11);
        let [Action::EditBatch(commands)] = actions.as_slice() else {
            panic!("unlock all batch");
        };
        let unlocked = host.edit(serde_json::to_string(commands).unwrap()).unwrap();
        assert_eq!(unlocked["project"], initial["project"]);
        assert_eq!(host.history(0).unwrap()["project"], locked["project"]);
        host.history(1).unwrap();
        host.save().unwrap();
        assert_eq!(motion_core::storage::load(root.path()).unwrap(), project);
        let unchanged_revision = host.state().unwrap()["revision"].clone();
        assert!(choose(&mut panels, &unlocked, 11).is_empty());
        panels.select_only(Some(999));
        assert!(choose(&mut panels, &unlocked, 10).is_empty());
        let mut observing = locked.clone();
        observing["observing"] = true.into();
        assert!(choose(&mut panels, &observing, 11).is_empty());
        assert_eq!(host.state().unwrap()["revision"], unchanged_revision);
    }
}

#[test]
fn mask_dropdown_commits_only_the_selected_mode_and_cancel_lock_do_not_edit() {
    use motion_core::{
        masks::{LayerMask, MaskMode},
        vector::{PathNode, VectorPath},
        Track,
    };
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            let root = tempfile::tempdir().unwrap();
            let mut project = project();
            project.layers[0].masks.push(LayerMask::new(
                1,
                VectorPath {
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
                },
            ));
            motion_core::storage::save(root.path(), &project).unwrap();
            let host = crate::engine::Engine::start(
                root.path().into(),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let initial = host.state().unwrap();
            let mut panels = Panels {
                selected: Some(7),
                focused: Some(Panel::Properties),
                locale,
                ..Default::default()
            };
            panels.path_controls.masks_open = true;
            draw(
                &mut panels,
                &initial,
                &Input::default(),
                Panel::Properties,
                floating,
                &mut atlas,
            );
            let rect = panels.path_controls.inspector_modes[&(7, 1)];
            assert!(draw(
                &mut panels,
                &initial,
                &click(rect, MouseButton::Left),
                Panel::Properties,
                floating,
                &mut atlas
            )
            .is_empty());
            let mut paint = PaintList::default();
            let actions = if floating {
                floating_chrome(
                    &mut paint,
                    &mut atlas,
                    Panel::Properties,
                    [1000., 800.],
                    &mut panels,
                    &initial,
                    &Input::default(),
                )
            } else {
                chrome(
                    &mut paint,
                    &mut atlas,
                    [1000., 800.],
                    &dock::Node::dock(Panel::Properties.dock()),
                    &mut panels,
                    &initial,
                    &Input::default(),
                )
            };
            assert!(actions.is_empty());
            let row = paint.styles().metric("row.height", 22.);
            let (popup_index, popup) = paint
                .shapes
                .iter()
                .enumerate()
                .find_map(|(index, shape)| match shape {
                    motion_ui::paint::Shape::Procedural {
                        area,
                        kind: motion_ui::paint::Pattern::Box { .. },
                        ..
                    } if (area.height() - row * 7.).abs() < 0.01
                        && area.contains([rect.min[0] + 4., rect.max[1] + 4.]) =>
                    {
                        Some((index, *area))
                    }
                    _ => None,
                })
                .expect("all seven mask modes are drawn in the popup");
            let popup_order = paint
                .order
                .iter()
                .position(|op| *op == motion_ui::paint::PaintOp::Shape(popup_index))
                .unwrap();
            for op in &paint.order[popup_order + 1..] {
                let motion_ui::paint::PaintOp::Shape(index) = *op else {
                    continue;
                };
                if paint.colors[index].0[3] < 1. {
                    continue;
                }
                let bounds = match &paint.shapes[index] {
                    motion_ui::paint::Shape::Quad { points } => Rect::new(
                        points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
                        points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
                        points
                            .iter()
                            .map(|p| p[0])
                            .fold(f32::NEG_INFINITY, f32::max)
                            - points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
                        points
                            .iter()
                            .map(|p| p[1])
                            .fold(f32::NEG_INFINITY, f32::max)
                            - points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
                    ),
                    motion_ui::paint::Shape::Procedural { area, .. } => *area,
                    _ => continue,
                };
                assert!(
                    !(bounds.width() > popup.width()
                        && bounds.height() >= row
                        && bounds.min[0] < popup.max[0]
                        && bounds.max[0] > popup.min[0]
                        && bounds.min[1] < popup.max[1]
                        && bounds.max[1] > popup.min[1]),
                    "a later inspector row must not cover the mask dropdown: floating={floating} popup={popup:?} later={bounds:?}"
                );
            }
            assert!(draw(
                &mut panels,
                &initial,
                &key(Key::Down),
                Panel::Properties,
                floating,
                &mut atlas
            )
            .is_empty());
            assert!(draw(
                &mut panels,
                &initial,
                &key(Key::Down),
                Panel::Properties,
                floating,
                &mut atlas
            )
            .is_empty());
            assert_eq!(
                host.state().unwrap()["revision"],
                initial["revision"],
                "browsing is not a model edit"
            );
            let edit = command(draw(
                &mut panels,
                &initial,
                &key(Key::Enter),
                Panel::Properties,
                floating,
                &mut atlas,
            ));
            assert!(matches!(
                edit,
                Command::Mask {
                    action: motion_core::masks::MaskAction::Options {
                        mode: Some(MaskMode::Intersect),
                        ..
                    },
                    ..
                }
            ));
            let after = host.edit(serde_json::to_string(&edit).unwrap()).unwrap();
            host.save().unwrap();
            assert_eq!(
                motion_core::storage::load(root.path()).unwrap().layers[0].masks[0].mode,
                MaskMode::Intersect
            );
            assert_eq!(host.history(0).unwrap()["project"], initial["project"]);
            host.history(1).unwrap();
            draw(
                &mut panels,
                &after,
                &Input::default(),
                Panel::Properties,
                floating,
                &mut atlas,
            );
            let rect = panels.path_controls.inspector_modes[&(7, 1)];
            draw(
                &mut panels,
                &after,
                &click(rect, MouseButton::Left),
                Panel::Properties,
                floating,
                &mut atlas,
            );
            draw(
                &mut panels,
                &after,
                &key(Key::Down),
                Panel::Properties,
                floating,
                &mut atlas,
            );
            assert!(draw(
                &mut panels,
                &after,
                &key(Key::Escape),
                Panel::Properties,
                floating,
                &mut atlas
            )
            .is_empty());
            let locked = host
                .edit(
                    serde_json::to_string(&Command::Flags {
                        object: 7,
                        visible: true,
                        locked: true,
                    })
                    .unwrap(),
                )
                .unwrap();
            assert!(draw(
                &mut panels,
                &locked,
                &click(rect, MouseButton::Left),
                Panel::Properties,
                floating,
                &mut atlas
            )
            .is_empty());
            assert!(draw(
                &mut panels,
                &locked,
                &key(Key::Enter),
                Panel::Properties,
                floating,
                &mut atlas
            )
            .is_empty());
            assert_eq!(host.state().unwrap()["project"], locked["project"]);
        }
    }
}

#[test]
fn negative_time_segment_handles_are_read_only_and_authored_keys_survive_graph_views() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::En, Locale::Zh] {
            let root = tempfile::tempdir().unwrap();
            let mut project = project();
            project.layers[0]
                .transform
                .position
                .upsert(-10, [10., 0., 0.], motion_core::Ease::Linear)
                .unwrap();
            project.layers[0]
                .transform
                .position
                .upsert(10, [90., 0., 0.], motion_core::Ease::Linear)
                .unwrap();
            project.expressions.push(motion_core::PropertyExpression {
                target: motion_core::ExpressionTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                source: "[500,0,0]".into(),
                enabled: true,
                seed: 0,
                profile: motion_core::EXPRESSION_PROFILE.into(),
            });
            motion_core::storage::save(root.path(), &project).unwrap();
            let host = crate::engine::Engine::start(
                root.path().into(),
                motion_desktop_media::platform(),
                8 << 30,
            )
            .unwrap();
            let state = host.state().unwrap();
            assert_eq!(state["sampledLayers"][0]["position"][0], 500.);
            let source =
                super::animation_data::channel(&state, 7, Property::Position, None).unwrap();
            assert!(super::graph_notice::has_negative(source));
            assert_eq!(
                source["keys"][0]["value"][0], 10.,
                "graph reads authored values"
            );
            let mut panels = Panels {
                selected: Some(7),
                locale,
                ..Default::default()
            };
            panels
                .animation_controls
                .select_key(7, Property::Position, None, 10);
            let plot = super::graph_handles::Plot {
                area: Rect::new(0., 0., 200., 100.),
                offset: 0.,
                ppf: 10.,
                low: 0.,
                high: 100.,
                speed: false,
                fps: 30.,
                scale: 1.,
            };
            // The default incoming tangent of the positive endpoint is inside the
            // visible plot, but editing it would target the negative segment start.
            let point = [100. / 3., 110. / 3.];
            let mut out = vec![];
            super::graph_handles::draw(
                &mut PaintList::default(),
                &mut atlas,
                &mut panels,
                source,
                7,
                Property::Position,
                None,
                0,
                plot,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
                false,
                &mut out,
            );
            assert!(panels.timeline.graph_drag.is_none());
            assert!(out.is_empty());
            panels
                .animation_controls
                .select_key(7, Property::Position, None, -10);
            panels.timeline.set_graph_mode(true);
            for panel in [Panel::Timeline, Panel::Curves] {
                panels.focused = Some(panel);
                for input in [Input::default(), key(Key::Delete), key(Key::Escape)] {
                    let actions = draw(&mut panels, &state, &input, panel, floating, &mut atlas);
                    assert!(actions.iter().all(|a| !matches!(
                        a,
                        Action::Edit(_) | Action::EditBatch(_) | Action::History(2..=4)
                    )));
                }
            }
            host.save().unwrap();
            assert_eq!(motion_core::storage::load(root.path()).unwrap(), project);
            assert_eq!(host.state().unwrap()["revision"], state["revision"]);
        }
    }
}

#[test]
fn discrete_and_curve_effect_targets_show_a_disabled_expression_action() {
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &project()).unwrap();
    let host = crate::engine::Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let catalogue = host
        .plugin(serde_json::json!({"op":"catalogue"}).to_string())
        .unwrap();
    for name in ["brightness_contrast", "curves"] {
        let package = catalogue["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| {
                p["manifest"]["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["id"] == name)
            })
            .unwrap();
        host.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":name}).to_string()).unwrap();
    }
    let initial = host.state().unwrap();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for floating in [false, true] {
        for locale in [Locale::Zh, Locale::En] {
            for (effect_name, param) in [
                ("brightness_contrast", "p0003"),
                ("curves", "p0001"),
                ("curves", "p0002"),
            ] {
                let effect = initial["project"]["layers"][0]["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["effect"] == effect_name)
                    .unwrap()["id"]
                    .as_u64()
                    .unwrap();
                let mut panels = Panels {
                    selected: Some(7),
                    focused: Some(Panel::EffectControls),
                    locale,
                    ..Default::default()
                };
                panels.effect_controls.catalogue = catalogue.clone().into();
                draw(
                    &mut panels,
                    &initial,
                    &Input::default(),
                    Panel::EffectControls,
                    floating,
                    &mut atlas,
                );
                let rect = panels.effect_controls.test_rects[&(7, effect, param.into(), -7)];
                draw(
                    &mut panels,
                    &initial,
                    &click(rect, MouseButton::Right),
                    Panel::EffectControls,
                    floating,
                    &mut atlas,
                );
                let action = panels.effect_controls.test_rects[&(7, effect, "__menu".into(), 6)];
                assert!(draw(
                    &mut panels,
                    &initial,
                    &click(action, MouseButton::Left),
                    Panel::EffectControls,
                    floating,
                    &mut atlas
                )
                .is_empty());
                assert!(panels.effect_controls.expressions.draft.is_none());
                assert!(draw(
                    &mut panels,
                    &initial,
                    &key(Key::Escape),
                    Panel::EffectControls,
                    floating,
                    &mut atlas
                )
                .is_empty());
            }
        }
    }
    assert_eq!(host.state().unwrap()["project"], initial["project"]);
    assert_eq!(host.state().unwrap()["revision"], initial["revision"]);
}
