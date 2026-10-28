use super::*;

#[test]
fn shared_menu_missing_styles_are_rejected_before_a_sheet_can_reach_the_painter() {
    for class in ["context_menu", "context_menu.row"] {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../styles/dark.json")).unwrap();
        value["styles"].as_object_mut().unwrap().remove(class);
        let result = StyleSheet::from_json(&serde_json::to_vec(&value).unwrap());
        assert!(matches!(result,Err(reason) if reason==format!("missing framework style {class}")));
    }
}

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
#[test]
fn shared_menu_geometry_keeps_root_child_and_clipped_hits_inside_small_windows() {
    let metrics = Metrics::context(&PaintList::default().styles());
    for bounds in [
        Rect::new(0., 0., 800., 500.),
        Rect::new(0., 24., 240., 156.),
        Rect::new(7., 11., 9., 5.),
    ] {
        let card = popup(
            [bounds.max[0], bounds.max[1]],
            bounds,
            220.,
            metrics.height(30, bounds),
            true,
        );
        let child = child(
            card,
            metrics.row_rect(card, 0),
            bounds,
            270.,
            metrics.height(15, bounds),
        );
        for rect in [card, child, metrics.row_rect(card, 0)] {
            assert!(rect.min[0] >= bounds.min[0] && rect.min[1] >= bounds.min[1]);
            assert!(rect.max[0] <= bounds.max[0] && rect.max[1] <= bounds.max[1]);
        }
        assert!(
            !metrics
                .row_rect(card, 0)
                .contains([bounds.max[0] + 1., bounds.max[1]])
        );
    }
    let bounds = Rect::new(0., 0., 800., 500.);
    let root = popup([790., 490.], bounds, 220., 200., true);
    assert_eq!(
        child(root, metrics.row_rect(root, 0), bounds, 270., 100.).max[0],
        root.min[0]
    );
}
#[test]
fn shared_menu_keyboard_skips_disabled_scrolls_and_does_not_repeat_actions_or_consume_ime() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let metrics = Metrics::context(&PaintList::default().styles());
    let entries: Vec<_> = (0..20)
        .map(|i| Entry::new(format!("Entry {i}"), i, i % 3 != 0))
        .collect();
    let mut highlighted = 0;
    let mut scroll = 0;
    let mut run = |input: &Input| {
        draw(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 240., 86.),
            metrics,
            &entries,
            input,
            true,
            false,
            &mut highlighted,
            &mut scroll,
        )
    };
    assert_eq!(run(&key(Key::Home)).chosen, None);
    assert_eq!(run(&key(Key::Enter)).chosen, Some(1));
    run(&key(Key::End));
    assert_eq!(run(&key(Key::Enter)).chosen, Some(19));
    run(&key(Key::PageUp));
    assert_eq!(run(&key(Key::Enter)).chosen, Some(14));
    let mut repeat = key(Key::Enter);
    repeat.key_repeats = vec![true];
    assert_eq!(run(&repeat).chosen, None);
    let mut ime = key(Key::Enter);
    ime.ime_consumed = true;
    assert_eq!(run(&ime).chosen, None);
    let mut modifier = key(Key::Enter);
    modifier.keys[0].1.control = true;
    assert_eq!(run(&modifier).chosen, None);
    drop(run);
    assert!(scroll > 0 && scroll <= highlighted);
}
#[test]
fn shared_menu_clicks_only_enabled_visible_rows_and_empty_is_a_normal_noop() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut paint = PaintList::default();
    let metrics = Metrics::context(&paint.styles());
    let card = Rect::new(10., 10., 220., 46.);
    let entries = [
        Entry::new("Disabled".into(), (), false),
        Entry::new("Enabled".into(), (), true),
        Entry::new("Offscreen".into(), (), true),
    ];
    let (mut selected, mut scroll) = (0, 0);
    for (point, expected) in [
        ([20., 20.], None),
        ([20., 40.], Some(1)),
        ([20., 60.], None),
    ] {
        let input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        };
        assert_eq!(
            draw(
                &mut paint,
                &mut atlas,
                card,
                metrics,
                &entries,
                &input,
                false,
                true,
                &mut selected,
                &mut scroll
            )
            .chosen,
            expected
        );
    }
    let empty: [Entry<()>; 0] = [];
    assert_eq!(
        draw(
            &mut paint,
            &mut atlas,
            card,
            metrics,
            &empty,
            &key(Key::Enter),
            true,
            true,
            &mut selected,
            &mut scroll
        )
        .chosen,
        None
    );
    assert_eq!((selected, scroll), (0, 0));
}
#[test]
fn shared_menu_hint_is_an_explicit_catalogue_column_not_a_user_text_parser() {
    let mut entry = Entry::new("Duplicate    Ctrl+B".into(), (), true);
    entry.hint = "Ctrl+B".into();
    assert_eq!(entry.caption(), "Duplicate");
    entry.label = "User    name".into();
    assert_eq!(entry.caption(), "User    name");
    entry.hint.clear();
    entry.label = "User    Ctrl+B".into();
    assert_eq!(entry.caption(), "User    Ctrl+B");
}
