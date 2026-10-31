use super::*;

fn key(key: Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        mouse: [-1.; 2],
        ..Default::default()
    }
}
fn frame(panels: &mut Panels, state: &Value, atlas: &mut TextAtlas, input: &Input) -> Vec<Action> {
    let mut out = vec![];
    super::super::render(
        &mut PaintList::default(),
        atlas,
        [900., 600.],
        panels,
        input,
        state,
        &mut out,
    );
    out
}
#[test]
fn shared_menu_main_chrome_keeps_enter_away_from_timeline_rename() {
    for locale in [Locale::Zh, Locale::En] {
        let mut project = motion_core::Project::new(64, 64, 30, 60).unwrap();
        project.layers.push(motion_core::Layer::solid(
            7, "Layer", [32.; 2], [0.; 3], [1.; 4],
        ));
        let state = serde_json::json!({"root":"a","composition":project.composition_id,"frame":0.,"timeline_layers":project.timeline_layers(0.).unwrap(),"project":project});
        let mut panels = Panels {
            locale,
            selected: Some(7),
            focused: Some(Panel::Timeline),
            menu: Some(7),
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut run = |panels: &mut Panels, input: &Input| {
            crate::panels::chrome(
                &mut PaintList::default(),
                &mut atlas,
                [900., 600.],
                &dock::Node::dock(Panel::Timeline.dock()),
                panels,
                &state,
                input,
            )
        };
        assert!(run(&mut panels, &key(Key::End)).is_empty());
        assert!(run(&mut panels, &key(Key::Enter)).is_empty());
        assert_eq!(panels.menu_navigation.cards.len(), 2);
        assert!(!panels.layer_actions.text_in(Some(Panel::Timeline)));
        assert!(run(&mut panels, &key(Key::Escape)).is_empty());
        assert_eq!(panels.menu, Some(7));
        assert_eq!(panels.menu_navigation.cards.len(), 1);
        assert!(run(&mut panels, &key(Key::Escape)).is_empty());
        assert_eq!(panels.menu, None);
        assert!(!panels.layer_actions.editing());
    }
}
#[test]
fn shared_menu_application_navigation_opens_child_without_executing_and_escape_returns_to_parent() {
    for locale in [Locale::Zh, Locale::En] {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let state = serde_json::json!({"root":"a", "composition":"comp-main"});
        let mut panels = Panels {
            locale,
            menu: Some(7),
            ..Default::default()
        };
        assert!(frame(&mut panels, &state, &mut atlas, &key(Key::End)).is_empty());
        assert!(frame(&mut panels, &state, &mut atlas, &key(Key::Enter)).is_empty());
        assert!(panels.menu_navigation.child);
        assert_eq!(panels.menu_navigation.cards.len(), 2);
        assert!(frame(&mut panels, &state, &mut atlas, &key(Key::Escape)).is_empty());
        assert_eq!(panels.menu, Some(7));
        assert!(!panels.menu_navigation.child);
        frame(&mut panels, &state, &mut atlas, &key(Key::Right));
        frame(&mut panels, &state, &mut atlas, &key(Key::Down));
        let actions = frame(&mut panels, &state, &mut atlas, &key(Key::Enter));
        assert!(matches!(
            actions.as_slice(),
            [Action::Workspace(crate::workspace::Preset::Animation)]
        ));
        assert_eq!(panels.menu, None);
        assert_eq!(workspaces(&panels).iter().filter(|e| e.checked).count(), 1);
    }
}
#[test]
fn shared_menu_context_loss_ime_focus_and_cancel_never_dispatch_an_authored_edit() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut state = serde_json::json!({"root":"a", "composition":"comp-main"});
    let mut panels = Panels {
        menu: Some(0),
        ..Default::default()
    };
    frame(&mut panels, &state, &mut atlas, &Input::default());
    let mut ime = key(Key::Escape);
    ime.ime_consumed = true;
    assert!(frame(&mut panels, &state, &mut atlas, &ime).is_empty());
    assert_eq!(panels.menu, Some(0));
    state["root"] = "b".into();
    assert!(frame(&mut panels, &state, &mut atlas, &key(Key::Enter)).is_empty());
    assert_eq!(panels.menu, None);
    for input in [
        key(Key::Tab),
        Input {
            focus_lost: true,
            ..Default::default()
        },
        Input {
            mouse: [899., 599.],
            pressed: Some((MouseButton::Left, [899., 599.])),
            ..Default::default()
        },
    ] {
        panels.menu = Some(0);
        frame(&mut panels, &state, &mut atlas, &Input::default());
        assert!(frame(&mut panels, &state, &mut atlas, &input).is_empty());
        assert_eq!(panels.menu, None);
    }
}
#[test]
fn shared_menu_hints_follow_current_remapping_and_checks_follow_current_state() {
    use crate::shortcuts::{Command as S, Scope, bindings::Bindings};
    let mut panels = Panels::default();
    let mut bindings = Bindings::default();
    bindings
        .bind(
            Scope::Application,
            S::Duplicate,
            Some(motion_ui::input::KeyChord::new(Key::B, true, false, false)),
        )
        .unwrap();
    panels.shortcut_bindings = std::sync::Arc::new(bindings);
    assert_eq!(
        hint(
            &Choice::Direct(Action::Edit(Command::Duplicate { object: 7 })),
            &panels,
            &Value::Null,
            3,
            8
        ),
        "Ctrl+B"
    );
    let mut next = panels.timeline.column_visibility;
    next.matte = !next.matte;
    let choice = Choice::Direct(Action::TimelineColumnVisibility(next));
    assert_eq!(checked(&choice, &panels, &Value::Null), !next.matte);
    panels.timeline.column_visibility = next;
    let mut previous = next;
    previous.matte = !previous.matte;
    assert_eq!(
        checked(
            &Choice::Direct(Action::TimelineColumnVisibility(previous)),
            &panels,
            &Value::Null
        ),
        next.matte
    );
}

#[test]
fn layer_menu_delete_dispatches_the_current_playhead() {
    for locale in [Locale::Zh, Locale::En] {
        let mut project = motion_core::Project::new(64, 64, 30, 60).unwrap();
        project.layers.push(motion_core::Layer::solid(7, "Layer", [32.; 2], [0.; 3], [1.; 4]));
        let state = serde_json::json!({"root":"a", "composition":project.composition_id,
            "frame":21.8, "project":project});
        let mut panels = Panels { locale, selected: Some(7), menu: Some(3), ..Default::default() };
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        assert!(frame(&mut panels, &state, &mut atlas, &Input::default()).is_empty());
        panels.menu_navigation.highlighted = 9;
        let actions = frame(&mut panels, &state, &mut atlas, &key(Key::Enter));
        assert!(matches!(actions.as_slice(),
            [Action::Edit(Command::RemoveLayers { objects, frame: 21 })] if objects.as_slice() == [7]));
        assert_eq!(panels.menu, None);
    }
}
