use motion_ui::{
    dock::{DockId, Node},
    tabs::{self, Event, Tab},
    workspace::Workspace,
    Input, MouseButton, PaintList, Rect, TextAtlas,
};
fn specs() -> [Tab<'static>; 2] {
    [
        Tab {
            title: "项目",
            context: None,
            badge: None,
            locked: false,
            closable: false,
        },
        Tab {
            title: "效果控件",
            context: Some("<空文本图层>"),
            badge: None,
            locked: true,
            closable: true,
        },
    ]
}
#[test]
fn header_actions_do_not_activate_tabs_and_overflow_stays_inside_the_group() {
    let mut atlas = TextAtlas::from_system_language(12., true).unwrap();
    for width in [100., 304., 800.] {
        let area = Rect::new(0., 0., width, 26.);
        let tabs = specs();
        let regions = tabs::layouts(&mut atlas, area, &tabs, 1);
        assert!(regions.iter().all(|r| area.contains_rect(r.area)));
        let menu = regions[1].menu.unwrap();
        let point = [menu.min[0] + 8., menu.min[1] + 8.];
        let result = tabs::draw(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &tabs,
            1,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            true,
        );
        assert_eq!(result, Some(Event::Menu(1)));
        let close = regions[1].close.unwrap();
        let point = [close.min[0] + 6., close.min[1] + 8.];
        assert_eq!(
            tabs::draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &tabs,
                1,
                &Input {
                    mouse: point,
                    pressed: Some((MouseButton::Left, point)),
                    ..Default::default()
                },
                true
            ),
            Some(Event::Close(1))
        );
    }
}
#[test]
fn dragging_an_inactive_tab_detaches_that_tab_without_switching_its_owner() {
    let mut workspace = Workspace::default();
    workspace.root = Node::Dock {
        id: DockId(1),
        tabs: vec![DockId(1), DockId(2)],
        active: 0,
    };
    let area = Rect::new(0., 0., 300., 300.);
    let point = [80., 12.];
    workspace.interact_with_header_actions(
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        area,
        0.,
    );
    workspace.choose_dragged_tab(DockId(2));
    // A repeated pending press must not reset the drag origin.
    workspace.interact_with_header_actions(
        &Input {
            mouse: [180., 12.],
            pressed: Some((MouseButton::Left, [180., 12.])),
            ..Default::default()
        },
        area,
        0.,
    );
    let moved = [350., 12.];
    let result = workspace.interact(
        &Input {
            mouse: moved,
            released: Some((MouseButton::Left, moved)),
            ..Default::default()
        },
        area,
    );
    assert!(matches!(
        result,
        Some(motion_ui::workspace::Change::Float(DockId(2)))
    ));
    assert!(workspace.float(DockId(2)));
    assert!(workspace.root.find(DockId(1)).is_some());
    assert!(workspace.valid());
}
