use motion_ui::{
    cursor::Cursor,
    motion::{Easing, Motion},
    style::{default_stylesheet, States},
    widgets::*,
    Color, Input, Key, MouseButton, PaintList, Rect, TextAtlas,
};
use std::sync::Arc;
fn click(x: f32, y: f32) -> Input {
    Input {
        mouse: [x, y],
        pressed: Some((MouseButton::Left, [x, y])),
        ..Default::default()
    }
}
fn key(k: Key) -> Input {
    Input {
        keys: vec![(k, Default::default())],
        ..Default::default()
    }
}

#[test]
fn checkbox_row_hit_keyboard_and_disabled_states_work_in_both_languages() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let area = Rect::new(0., 0., 160., 22.);
    for label in ["启用 3D", "Enable 3D"] {
        let mut list = PaintList::default();
        assert!(
            Checkbox::draw(
                &mut list,
                &mut atlas,
                area,
                label,
                false,
                &click(140., 11.),
                true,
                false,
                None
            )
            .clicked
        );
        assert!(
            Checkbox::draw(
                &mut list,
                &mut atlas,
                area,
                label,
                false,
                &key(Key::Space),
                true,
                true,
                None
            )
            .clicked
        );
        list.clear();
        assert!(
            !Checkbox::draw(
                &mut list,
                &mut atlas,
                area,
                label,
                true,
                &key(Key::Space),
                false,
                true,
                None
            )
            .clicked
        );
        assert!(list
            .glyphs
            .iter()
            .all(|glyph| glyph.color == default_stylesheet().color("disabled")));
        assert!(
            !Checkbox::draw(
                &mut list,
                &mut atlas,
                area,
                label,
                true,
                &key(Key::Space),
                true,
                false,
                None
            )
            .clicked
        );
        list.clear();
        Checkbox::draw_with_states(
            &mut list,
            &mut atlas,
            area,
            label,
            true,
            &Input::default(),
            true,
            false,
            None,
            States::default().with(motion_ui::style::State::Invalid),
        );
        assert!(list.shapes.iter().any(|shape|matches!(shape,motion_ui::paint::Shape::Procedural {kind:motion_ui::paint::Pattern::Box {border,..},..} if *border==default_stylesheet().color("error"))));
    }
}
#[test]
fn search_unicode_selection_clear_and_enter_do_not_leak_to_other_fields() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut list = PaintList::default();
    let area = Rect::new(0., 0., 180., 24.);
    for placeholder in ["搜索", "Search"] {
        let mut field = SearchField::default();
        let mut query = "原文字".to_string();
        field.draw(
            &mut list,
            &mut atlas,
            area,
            &mut query,
            placeholder,
            &click(80., 12.),
            true,
        );
        let response = field.draw(
            &mut list,
            &mut atlas,
            area,
            &mut query,
            placeholder,
            &Input {
                text: "字母 A".into(),
                ime_consumed: true,
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
            true,
        );
        assert!(response.changed);
        assert!(!response.submitted);
        assert!(response.active);
        assert_eq!(query, "字母 A");
        let response = field.draw(
            &mut list,
            &mut atlas,
            area,
            &mut query,
            placeholder,
            &key(Key::Enter),
            true,
        );
        assert!(response.submitted);
        assert!(!response.active);
        let response = field.draw(
            &mut list,
            &mut atlas,
            area,
            &mut query,
            placeholder,
            &click(166., 12.),
            true,
        );
        assert!(response.changed);
        assert!(query.is_empty());
        let response = field.draw(
            &mut list,
            &mut atlas,
            area,
            &mut query,
            placeholder,
            &Input {
                text: "blocked".into(),
                ..Default::default()
            },
            false,
        );
        assert!(!response.changed);
        assert!(query.is_empty());
    }
}
#[test]
fn dropdown_flips_scrolls_skips_disabled_items_and_commits_only_after_opening() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut list = PaintList::default();
    let area = Rect::new(0., 150., 160., 24.);
    let bounds = Rect::new(0., 0., 160., 180.);
    let items = [
        DropdownItem {
            label: "第一 / First",
            enabled: true,
        },
        DropdownItem {
            label: "不可选 / Disabled",
            enabled: false,
        },
        DropdownItem {
            label: "第三 / Third",
            enabled: true,
        },
    ];
    let mut dropdown = Dropdown::default();
    assert!(dropdown
        .draw(
            &mut list,
            &mut atlas,
            area,
            bounds,
            &items,
            0,
            &key(Key::Enter),
            true,
            true
        )
        .is_none());
    assert!(dropdown.open);
    dropdown.draw(
        &mut list,
        &mut atlas,
        area,
        bounds,
        &items,
        0,
        &key(Key::Down),
        true,
        true,
    );
    assert_eq!(
        dropdown.draw(
            &mut list,
            &mut atlas,
            area,
            bounds,
            &items,
            0,
            &key(Key::Enter),
            true,
            true
        ),
        Some(2)
    );
    assert!(!dropdown.open);
    dropdown.draw(
        &mut list,
        &mut atlas,
        area,
        bounds,
        &items,
        2,
        &click(40., 162.),
        true,
        true,
    );
    assert!(dropdown.open);
    dropdown.draw(
        &mut list,
        &mut atlas,
        area,
        bounds,
        &items,
        2,
        &Input {
            focus_lost: true,
            ..Default::default()
        },
        true,
        true,
    );
    assert!(!dropdown.open);
    dropdown.draw(
        &mut list,
        &mut atlas,
        area,
        bounds,
        &items,
        2,
        &click(40., 162.),
        true,
        true,
    );
    assert_eq!(
        dropdown.draw(
            &mut list,
            &mut atlas,
            area,
            bounds,
            &items[..1],
            0,
            &key(Key::Enter),
            true,
            true
        ),
        Some(0)
    );
    dropdown.draw(
        &mut list,
        &mut atlas,
        area,
        bounds,
        &[],
        0,
        &click(40., 162.),
        true,
        true,
    );
    assert!(!dropdown.open);

    assert!(list.shapes.iter().any(|s| matches!(
        s,
        motion_ui::paint::Shape::Procedural {
            kind: motion_ui::paint::Pattern::Shadow { .. },
            ..
        }
    )));
}
#[test]
fn long_tree_list_reaches_last_item_and_clips_pointer_hits_with_shared_row_density() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let sheet = Arc::new(
        default_stylesheet()
            .with_overrides(br#"{"tokens":{"sizes":{"row.height":30}}}"#)
            .unwrap(),
    );
    let mut list = PaintList::with_styles(sheet.clone());
    let area = Rect::new(0., 0., 180., 90.);
    let labels: Vec<_> = (0..100).map(|i| format!("媒体 / Media {i}")).collect();
    let items: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| ListItem {
            label,
            depth: i % 2,
            expanded: Some(false),
            enabled: true,
        })
        .collect();
    let mut view = ListView::default();
    view.draw(&mut list, &mut atlas, area, &items, &key(Key::End), true);
    assert_eq!(view.scroll.offset, 2910.);
    let response = view.draw(&mut list, &mut atlas, area, &items, &click(60., 75.), true);
    assert_eq!(response.selected, Some(99));
    let response = view.draw(&mut list, &mut atlas, area, &items, &key(Key::Enter), true);
    assert_eq!(response.activated, Some(99));
    let response = view.draw(&mut list, &mut atlas, area, &items, &click(60., 110.), true);
    assert!(response.selected.is_none());
    assert!(Arc::ptr_eq(&list.child().styles(), &sheet));
}
#[test]
fn stars_have_keyboard_alternative_and_do_not_change_disabled_values() {
    let mut list = PaintList::default();
    let area = Rect::new(0., 0., 100., 22.);
    assert_eq!(
        StarRating::draw(&mut list, area, 2, 5, &click(70., 11.), true, false),
        Some(4)
    );
    assert_eq!(
        StarRating::draw(&mut list, area, 2, 5, &key(Key::Right), true, true),
        Some(3)
    );
    assert_eq!(
        StarRating::draw(&mut list, area, 2, 5, &key(Key::Right), false, true),
        None
    );
}
#[test]
fn numeric_threshold_modifier_cursor_and_easing_are_configurable() {
    assert!(numeric_behavior::started(0., 40., 12.));
    assert!(!numeric_behavior::started(5., 5., 12.));
    assert!(numeric_behavior::started(0., 0., 0.));
    assert_eq!(
        numeric_behavior::cursor(&Input::default(), true),
        Cursor::Grabbing
    );
    assert_eq!(
        numeric_behavior::cursor(
            &Input {
                modifiers: motion_ui::input::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            false
        ),
        Cursor::ScrubFine
    );
    let mut a = Motion::default();
    let mut b = Motion::default();
    a.set_easing(Easing::Linear);
    b.set_easing(Easing::CubicOut);
    a.value_at((1, 1), 0., 1., 100., false, 0.);
    b.value_at((1, 1), 0., 1., 100., false, 0.);
    assert_eq!(a.value_at((1, 1), 0., 1., 100., false, 0.05), 0.5);
    assert_eq!(b.value_at((1, 1), 0., 1., 100., false, 0.05), 0.875);
    let style = default_stylesheet()
        .with_overrides(
            br#"{"tokens":{"motion":{"easing":"linear"}},"styles":{"button":{"radius":6}}}"#,
        )
        .unwrap();
    assert_eq!(style.easing(), Easing::Linear);
    assert_eq!(
        style.resolve("button", States::default()).unwrap().radius,
        6.
    );
    assert!(default_stylesheet()
        .with_overrides(br#"{"tokens":{"motion":{"easing":"nonsense"}}}"#)
        .is_err());
}
#[test]
fn property_columns_stack_on_narrow_panels_without_overlap_and_keep_numeric_width() {
    let sheet = default_stylesheet();
    for width in [160., 220., 286., 400.] {
        let columns = PropertyColumns::new(sheet, Rect::new(0., 0., width, 500.), 0., 72.);
        assert!(columns.value.width() >= 72.);
        assert!(columns.axis.max[0] <= columns.value.min[0]);
        assert!(
            columns.label.max[0] <= columns.axis.min[0]
                || columns.label.max[1] <= columns.value.min[1]
        );
        assert!(columns.value.max[0] <= width);
        assert_eq!(sheet.color("focus"), Color::rgb(55, 142, 240));
    }
}
