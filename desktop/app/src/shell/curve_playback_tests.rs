use crate::*;

fn finish_actions(shell: &mut Shell) -> usize {
    let mut frames = 0;
    while let Some(action) = shell::actions::pop_action(&mut shell.actions) {
        shell.action(action, None).unwrap();
        if let Some((_, receive)) = shell.editing.take() {
            shell.accept_edit_state(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap(),
            );
            frames += usize::from(shell.present_due);
        }
    }
    frames
}

#[test]
fn graph_ruler_scrubbing_cannot_feed_back_through_acknowledged_frames_or_starve_space() {
    for lower_ruler in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        shell.home = None;
        let mut layer = Layer::solid(7, "Animated", [100.; 2], [100., 200., 0.], [1.; 4]);
        layer.transform.position.set_animated(27, true).unwrap();
        layer
            .transform
            .position
            .set_at(67, [300., 200., 0.])
            .unwrap();
        shell.state = shell
            .engine
            .edit(serde_json::to_string(&Command::Add { layer }).unwrap())
            .unwrap();
        let before = shell.state["project"].clone();
        let id = dock::Panel::Timeline.dock();
        shell.workspace.root = dock::Node::Dock {
            id,
            tabs: vec![id],
            active: 0,
        };
        shell.panels.select_only(Some(7));
        shell.panels.focused = Some(dock::Panel::Timeline);
        shell.panels.timeline.set_graph_mode(true);
        shell.panels.timeline.graph_fitted = true;
        shell.panels.timeline.zoom = 1.;
        let size = [1200., 900.];
        assert!(shell.draw_commands(size).1.is_empty());
        let area = panels::frames(&shell.workspace.root, size)[0].content;
        let track_left = area.min[0] + 640_f32.min(area.width() * 0.52);
        let ppf = (area.max[0] - track_left - 8.) / 300.;
        let point = [
            track_left + 25. * ppf,
            if lower_ruler {
                area.max[1] - 48.
            } else {
                area.min[1] + 50.
            },
        ];
        shell.translate(Event::MousePressed {
            position: point,
            button: MouseButton::Left,
        });
        let (_, actions) = shell.draw_commands(size);
        let expected = if lower_ruler { 25. } else { 27. };
        assert!(
            matches!(actions.as_slice(), [panels::Action::Seek(frame)] if *frame == expected),
            "lower_ruler={lower_ruler}, seeks={:?}",
            actions
                .iter()
                .filter_map(|a| match a {
                    panels::Action::Seek(f) => Some(f),
                    _ => None,
                })
                .collect::<Vec<_>>()
        );
        shell.actions.extend(actions);
        assert_eq!(finish_actions(&mut shell), 1);
        assert!(
            shell.draw_acknowledged(size).1.is_empty(),
            "acknowledgement replayed the drag"
        );
        // Crossing the other ruler must preserve the original snapping policy.
        let other_y = if lower_ruler {
            area.min[1] + 50.
        } else {
            area.max[1] - 48.
        };
        shell.translate(Event::MouseMoved {
            position: [point[0], other_y],
        });
        assert!(shell.draw_commands(size).1.is_empty());
        shell.translate(Event::MouseMoved {
            position: [track_left + 40. * ppf, other_y],
        });
        let (_, actions) = shell.draw_commands(size);
        assert!(matches!(actions.as_slice(), [panels::Action::Seek(frame)] if *frame == 40.));
        shell.actions.extend(actions);
        assert_eq!(finish_actions(&mut shell), 1);
        assert!(shell.draw_acknowledged(size).1.is_empty());
        let release = [track_left + 50. * ppf, other_y];
        shell.translate(Event::MouseReleased {
            position: release,
            button: MouseButton::Left,
        });
        for _ in 0..8 {
            assert!(
                shell.draw_acknowledged(size).1.is_empty(),
                "release was starved"
            );
            assert!(shell.input.pending && shell.input.released.is_some());
        }
        let (_, actions) = shell.draw_commands(size);
        assert!(matches!(actions.as_slice(), [panels::Action::Seek(frame)] if *frame == 50.));
        assert!(!shell.panels.timeline.scrubbing);
        shell.actions.extend(actions);
        finish_actions(&mut shell);
        assert_eq!(shell.state["frame"], 50.);
        assert!(shell.draw_acknowledged(size).1.is_empty());
        shell.translate(Event::KeyPressed {
            key: Key::Space,
            modifiers: Modifiers::default(),
        });
        let (_, actions) = shell.draw_commands(size);
        assert!(matches!(actions.as_slice(), [panels::Action::TogglePlay]));
        shell.actions.extend(actions);
        finish_actions(&mut shell);
        assert!(shell.playback.is_some() && shell.panels.playing);
        assert!(shell.draw_acknowledged(size).1.is_empty());
        // Complete the presentation boundary, then exercise the real worker tick.
        shell.present_due = false;
        shell.attached = true;
        shell.playback = Some((Instant::now() - Duration::from_millis(100), 50.));
        shell.next_composition = Instant::now();
        assert!(shell.playback_ready(Instant::now()));
        shell.advance_playback().unwrap();
        assert!(!shell.playback_ready(Instant::now()));
        let (_, receive) = shell.editing.take().unwrap();
        shell.accept_edit_state(
            receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap(),
        );
        assert!(shell.state["frame"].as_f64().unwrap() >= 53.);
        assert!(shell.draw_acknowledged(size).1.is_empty());
        assert_eq!(shell.state["project"], before);
        shell.stop();

        for cancel in [
            Event::FocusLost,
            Event::KeyPressed {
                key: Key::Escape,
                modifiers: Modifiers::default(),
            },
        ] {
            shell.translate(Event::MousePressed {
                position: point,
                button: MouseButton::Left,
            });
            let (_, actions) = shell.draw_commands(size);
            shell.actions.extend(actions);
            finish_actions(&mut shell);
            assert!(shell.panels.timeline.scrubbing);
            shell.translate(cancel);
            assert!(shell.draw_commands(size).1.is_empty());
            assert!(!shell.panels.timeline.scrubbing);
            assert!(shell.draw_acknowledged(size).1.is_empty());
            shell.translate(Event::MouseReleased {
                position: release,
                button: MouseButton::Left,
            });
            assert!(shell.draw_commands(size).1.is_empty());
        }
    }
}
