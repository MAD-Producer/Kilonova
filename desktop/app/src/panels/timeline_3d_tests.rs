//! Exercise native input, the shell action queue and asynchronous model replies.
use super::*;
use motion_ui::input::Event;

#[test]
fn three_d_switch_is_editable_in_observation_views_without_changing_the_view() {
    for locale in [Locale::Zh, Locale::En] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            7, "Observed", [100.; 2], [0.; 3], [1.; 4],
        ));
        motion_core::storage::save(&root, &project).unwrap();
        let mut shell = crate::Shell::new(root.clone(), None, Some(locale)).unwrap();
        shell.home = None;
        shell.workspace.root = Node::dock(Panel::Timeline.dock());
        // This fixture calculates its click from the default column plan.
        // User workspace preferences may hide columns and shift the switch.
        shell.panels.timeline.column_visibility = Default::default();
        let styles = PaintList::default().styles();
        for kind in [1, 2, 3] {
            shell.action(Action::ViewKind(kind), None).unwrap();
            let (_, reply) = shell.editing.take().unwrap();
            shell.accept_edit_state(
                reply
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap(),
            );
            assert_eq!(shell.state["observing"], true);
            let view = shell.state["observationView"].clone();
            for floating in [false, true] {
                for width in [360_f32, 1280.] {
                    let top = if floating {
                        30.
                    } else {
                        frames(&shell.workspace.root, [width, 700.])[0].content.min[1]
                    };
                    let gutter = GUTTER.min(width * 0.52);
                    let slot = timeline_columns::ColumnRects::styled(
                        &styles,
                        0.,
                        gutter,
                        Default::default(),
                    )
                    .three_d
                    .unwrap();
                    let point = [
                        slot.x + slot.width * 0.5,
                        top + crate::panels::timeline::header_height(&styles) + styles.metric("row.height", 22.) * 0.5,
                    ];
                    let render = |shell: &mut crate::Shell, input: Input| {
                        shell.panels.focused = Some(Panel::Timeline);
                        if floating {
                            floating_chrome(
                                &mut PaintList::default(),
                                &mut shell.atlas,
                                Panel::Timeline,
                                [width, 700.],
                                &mut shell.panels,
                                &shell.state,
                                &input,
                            )
                        } else {
                            shell.input = input;
                            shell.draw_commands([width, 700.]).1
                        }
                    };
                    for enabled in [true, false] {
                        let actions = render(
                            &mut shell,
                            Input {
                                mouse: point,
                                pressed: Some((MouseButton::Left, point)),
                                ..Default::default()
                            },
                        );
                        assert!(
                            matches!(actions.as_slice(),[Action::Edit(Command::SetLayer3d {object:7,enabled:value})] if *value==enabled),
                            "kind={kind} floating={floating} width={width} actions={actions:?}"
                        );
                        shell
                            .action(actions.into_iter().next().unwrap(), None)
                            .unwrap();
                        let (_, reply) = shell.editing.take().unwrap();
                        shell.accept_edit_state(
                            reply
                                .recv_timeout(std::time::Duration::from_secs(5))
                                .unwrap()
                                .unwrap(),
                        );
                        assert_eq!(shell.state["project"]["layers"][0]["three_d"], enabled);
                        assert_eq!(shell.state["observing"], true);
                        assert_eq!(shell.state["observationView"], view);
                        shell.engine.save().unwrap();
                        assert_eq!(
                            motion_core::storage::load(&root).unwrap().layers[0].three_d,
                            enabled
                        );
                        let before_undo = shell.state["project"].clone();
                        shell.state = shell.engine.history(0).unwrap();
                        assert_eq!(shell.state["project"]["layers"][0]["three_d"], !enabled);
                        shell.state = shell.engine.history(1).unwrap();
                        assert_eq!(shell.state["project"], before_undo);
                        assert_eq!(shell.state["observationView"], view);
                    }
                    shell.state = shell
                        .engine
                        .edit(
                            serde_json::to_string(&Command::Flags {
                                object: 7,
                                visible: true,
                                locked: true,
                            })
                            .unwrap(),
                        )
                        .unwrap();
                    let before = shell.state["project"].clone();
                    assert!(render(
                        &mut shell,
                        Input {
                            mouse: point,
                            pressed: Some((MouseButton::Left, point)),
                            ..Default::default()
                        }
                    )
                    .is_empty());
                    assert_eq!(shell.engine.state().unwrap()["project"], before);
                    shell.state = shell.engine.history(0).unwrap();
                }
            }
        }
    }
}

#[test]
fn three_d_click_updates_the_shell_after_scrolling_and_switching_panel_focus() {
    for locale in [Locale::Zh, Locale::En] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("project");
        let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
        for object in 1..=32 {
            project.layers.push(motion_core::Layer::solid(
                object,
                &format!("Layer {object}"),
                [100.; 2],
                [0.; 3],
                [1.; 4],
            ));
        }
        motion_core::storage::save(&root, &project).unwrap();
        let mut shell = crate::Shell::new(root.clone(), None, Some(locale)).unwrap();
        shell.home = None;
        let size = [1280., 900.];
        // Keep the inspector and composition present; the first click must also
        // move focus from Properties to Timeline without consuming the switch.
        let layout = shell.workspace.root.clone();
        for full_workspace in [true, false] {
            shell.workspace.root = if full_workspace {
                layout.clone()
            } else {
                Node::dock(Panel::Timeline.dock())
            };
            for graph in [false, true] {
                shell.panels.timeline.set_graph_mode(graph);
                shell.panels.timeline.scroll = 0.;
                shell.input = Input::default();
                let styles = shell.draw_commands(size).0.styles();
                let area = frames(&shell.workspace.root, size)
                    .into_iter()
                    .find(|f| f.panel == Panel::Timeline)
                    .unwrap()
                    .content;
                shell.translate(Event::MouseMoved {
                    position: [area.min[0] + 10., area.min[1] + 90.],
                });
                shell.translate(Event::MouseWheel { delta: [0., 110.] });
                assert!(shell.draw_commands(size).1.is_empty());
                assert!(shell.panels.timeline.scroll > 0.);
                let row_height = styles.metric("row.height", 22.);
                let index = (shell.panels.timeline.scroll / row_height).ceil() as usize;
                let rows = timeline::rows(&shell.state);
                let object = rows[index].id;
                let columns = timeline_columns::ColumnRects::with_output_switches(
                    &styles,
                    area.min[0],
                    GUTTER.min(area.width() * 0.52),
                    shell.panels.timeline.column_visibility,
                    shell.panels.timeline.sampling_visibility,
                    super::layer_sampling::available(&shell.state),
                    super::playback_switches::available(&shell.state),
                );
                let switch = columns.three_d.unwrap();
                let point = [
                    switch.x + switch.width * 0.5,
                    area.min[1] + crate::panels::timeline::header_height(&styles) + index as f32 * row_height - shell.panels.timeline.scroll
                        + row_height * 0.5,
                ];
                for enabled in [true, false] {
                    shell.panels.focused = Some(Panel::Properties);
                    shell.translate(Event::MousePressed {
                        position: point,
                        button: MouseButton::Left,
                    });
                    let actions = shell.draw_commands(size).1;
                    assert!(
                        matches!(actions.as_slice(), [Action::Edit(Command::SetLayer3d {object: id, enabled: value})]
                        if *id == object && *value == enabled),
                        "{actions:?}"
                    );
                    shell.actions.extend(actions);
                    while let Some(action) = crate::shell::actions::pop_action(&mut shell.actions) {
                        shell.action(action, None).unwrap();
                        if let Some((_, reply)) = shell.editing.take() {
                            shell.accept_edit_state(
                                reply
                                    .recv_timeout(std::time::Duration::from_secs(5))
                                    .unwrap()
                                    .unwrap(),
                            );
                        }
                    }
                    let layer = shell.state["project"]["layers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|layer| layer["id"] == object)
                        .unwrap();
                    assert_eq!(layer["three_d"], enabled);
                    assert!(!shell.panels.timeline_columns.editing());
                    shell.translate(Event::MouseReleased {
                        position: point,
                        button: MouseButton::Left,
                    });
                    assert!(shell.draw_commands(size).1.is_empty());
                    shell.engine.save().unwrap();
                    let saved = motion_core::storage::load(&root).unwrap();
                    assert_eq!(
                        saved
                            .layers
                            .iter()
                            .find(|layer| layer.id == object)
                            .unwrap()
                            .three_d,
                        enabled
                    );
                }
            }
        }
    }
}
