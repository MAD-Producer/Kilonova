use super::*;

fn stamp(rows: Vec<Frame>) -> Vec<(u32, Vec<u32>)> {
    rows.into_iter()
        .map(|row| {
            (
                row.panel.dock().0,
                row.tabs
                    .min
                    .into_iter()
                    .chain(row.tabs.max)
                    .chain(row.content.min)
                    .chain(row.content.max)
                    .map(|value| {
                        if value.is_nan() {
                            f32::NAN.to_bits()
                        } else {
                            value.to_bits()
                        }
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn cached_frames_match_the_original_geometry_for_tabs_sizes_and_invalid_inputs() {
    let layouts = vec![
        dock::editor_layout(),
        Node::Dock {
            id: Panel::Project.dock(),
            tabs: vec![Panel::Project.dock(), Panel::Composition.dock()],
            active: 0,
        },
        Node::Dock {
            id: Panel::Project.dock(),
            tabs: vec![Panel::Project.dock(), Panel::Composition.dock()],
            active: 1,
        },
        Node::Dock {
            id: Panel::Project.dock(),
            tabs: vec![],
            active: 99,
        },
        Node::dock(dock::DockId(9999)),
        Node::Fixed {
            edge: dock::Edge::Top,
            size: 0.,
        },
        Node::Fixed {
            edge: dock::Edge::Top,
            size: f32::NAN,
        },
        Node::Split {
            edge: dock::Edge::Bottom,
            split: 0.3,
            first: Box::new(Node::dock(Panel::Timeline.dock())),
            second: Box::new(Node::dock(Panel::Properties.dock())),
        },
        Node::Split {
            edge: dock::Edge::Left,
            split: f32::INFINITY,
            first: Box::new(Node::dock(Panel::Timeline.dock())),
            second: Box::new(Node::dock(Panel::Properties.dock())),
        },
    ];
    for layout in layouts {
        for size in [[900., 600.], [1600., 1000.], [0., -0.], [f32::NAN, 600.]] {
            let expected = stamp(calculate_frames(&layout, size));
            for _ in 0..3 {
                assert_eq!(stamp(frames(&layout, size)), expected);
            }
        }
    }
}

#[test]
fn returned_frame_edits_and_in_place_layout_changes_do_not_poison_later_queries() {
    let mut layout = Node::Dock {
        id: Panel::Project.dock(),
        tabs: vec![Panel::Project.dock(), Panel::Composition.dock()],
        active: 0,
    };
    let expected = stamp(calculate_frames(&layout, [900., 600.]));
    let mut owned = frames(&layout, [900., 600.]);
    owned[0].panel = Panel::Info;
    owned[0].content.min[0] = -500.;
    assert_eq!(stamp(frames(&layout, [900., 600.])), expected);
    if let Node::Dock { active, .. } = &mut layout {
        *active = 1;
    }
    let changed = frames(&layout, [900., 600.]);
    assert_eq!(changed[0].panel, Panel::Composition);
    assert_eq!(
        stamp(changed),
        stamp(calculate_frames(&layout, [900., 600.]))
    );
    assert_eq!(
        stamp(frames(&layout, [1600., 1000.])),
        stamp(calculate_frames(&layout, [1600., 1000.]))
    );
    assert_eq!(owned[0].panel, Panel::Info);
}

#[test]
fn a_borrowed_optional_cache_slot_falls_back_without_panicking_or_changing_geometry() {
    let layout = dock::editor_layout();
    let expected = stamp(calculate_frames(&layout, [900., 600.]));
    FRAME_CACHE.with(|slot| {
        let _busy = slot.borrow_mut();
        assert_eq!(stamp(frames(&layout, [900., 600.])), expected);
    });
    assert_eq!(stamp(frames(&layout, [900., 600.])), expected);
}

#[test]
fn geometry_is_still_available_after_the_optional_thread_cache_is_destroyed() {
    struct Late(std::sync::mpsc::Sender<(bool, Vec<(u32, Vec<u32>)>)>);
    impl Drop for Late {
        fn drop(&mut self) {
            let unavailable = FRAME_CACHE.try_with(|_| ()).is_err();
            let result = stamp(frames(&Node::dock(Panel::Info.dock()), [900., 600.]));
            let _ = self.0.send((unavailable, result));
        }
    }
    thread_local! { static LATE: std::cell::RefCell<Option<Late>> = const {std::cell::RefCell::new(None)}; }
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        LATE.with(|slot| *slot.borrow_mut() = Some(Late(send)));
        frames(&Node::dock(Panel::Info.dock()), [900., 600.]);
    })
    .join()
    .unwrap();
    let (unavailable, actual) = receive.recv().unwrap();
    assert!(unavailable, "cache destruction was not exercised");
    assert_eq!(
        actual,
        stamp(calculate_frames(
            &Node::dock(Panel::Info.dock()),
            [900., 600.]
        ))
    );
}
