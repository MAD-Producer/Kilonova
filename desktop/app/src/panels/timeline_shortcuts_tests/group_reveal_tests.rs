use super::*;
#[cfg(target_os = "windows")]
mod native;
mod supported_content;

fn fixture() -> (Engine, Value) {
    let (engine, catalogue) = effect_timeline::tests::fixture();
    let mut project = engine.project().clone();
    let layer = &mut project.layers[0];
    let path = motion_core::vector::VectorPath {
        tangents: Default::default(), id: 1,
        closed: true,
        nodes: [[10., 10.], [90., 10.], [90., 90.], [10., 90.]]
            .into_iter()
            .enumerate()
            .map(|(index, [x, y])| motion_core::vector::PathNode {
                id: index as u64 + 1,
                geometry: motion_core::Track::constant([x, y, 0., 0., 0., 0.]),
            })
            .collect(),
    };
    let mut mask = motion_core::masks::LayerMask::new(1, path);
    mask.opacity.set_animated(0, true).unwrap();
    mask.opacity.set_at(60, 0.5).unwrap();
    layer.masks.push(mask);
    (Engine::new(project).unwrap(), catalogue)
}

fn draw(p: &mut Panels, state: &Value, key: Option<Key>, floating: bool) {
    let input = Input {
        keys: key
            .into_iter()
            .map(|key| (key, Modifiers::default()))
            .collect(),
        ..Default::default()
    };
    let mut list = PaintList::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let actions = if floating {
        super::super::floating_chrome(
            &mut list,
            &mut atlas,
            Panel::Timeline,
            [1000., 760.],
            p,
            state,
            &input,
        )
    } else {
        super::super::chrome(
            &mut list,
            &mut atlas,
            [1000., 760.],
            &dock::Node::dock(Panel::Timeline.dock()),
            p,
            state,
            &input,
        )
    };
    assert!(
        actions.is_empty(),
        "disclosure must not edit or seek: {actions:?}"
    );
}

#[test]
fn effect_and_mask_keys_reveal_filtered_collapsed_items_and_toggle_in_both_hosts_and_locales() {
    let (engine, catalogue) = fixture();
    let original = engine.project().clone();
    let history = (engine.revision(), engine.can_undo(), engine.can_redo());
    let state = state(&engine, 30.);
    for locale in [Locale::En, Locale::Zh] {
        for floating in [false, true] {
            for key in [Key::E, Key::M] {
                let mut p = panels();
                p.locale = locale;
                p.effect_controls.catalogue = catalogue.clone().into();
                p.timeline
                    .filters
                    .insert(7, types::PropertyFilter::Properties(1));
                p.timeline.collapsed_effect_items.insert((7, 1));
                p.timeline.collapsed_masks.insert(7);
                p.timeline.collapsed_mask_items.insert((7, 1));
                draw(&mut p, &state, Some(key), floating);
                draw(&mut p, &state, None, floating);
                let rows = timeline::rows(&state);
                let row = rows.iter().find(|row| row.id == 7).unwrap();
                let layout = timeline_layout::LayerRows::new(row, &p.timeline, 30., locale);
                assert!(layout.expanded);
                assert!(!p.timeline.filters.contains_key(&7));
                if key == Key::E {
                    assert!(layout.effects_open && !layout.effect_rows.is_empty());
                    assert!(!p.timeline.collapsed_effect_items.contains(&(7, 1)));
                    assert!(
                        p.timeline.collapsed_masks.contains(&7),
                        "keep other group state"
                    );
                } else {
                    assert!(layout.mask_rows.len() > 2);
                    assert!(!p.timeline.collapsed_masks.contains(&7));
                    assert!(!p.timeline.collapsed_mask_items.contains(&(7, 1)));
                    assert!(
                        !p.timeline.expanded_effects.contains(&7),
                        "keep other group state"
                    );
                }
                draw(&mut p, &state, Some(key), floating);
                if key == Key::E {
                    assert!(!p.timeline.expanded_effects.contains(&7));
                } else {
                    assert!(p.timeline.collapsed_masks.contains(&7));
                }
                draw(&mut p, &state, Some(key), floating);
                assert!(p.timeline.expanded.contains(&7));
            }
        }
    }
    assert_eq!(engine.project(), &original);
    assert_eq!(
        (engine.revision(), engine.can_undo(), engine.can_redo()),
        history
    );
}

#[test]
fn group_keys_defer_to_text_ime_lost_focus_repeat_menus_and_active_gestures() {
    let (engine, _) = fixture();
    let state = state(&engine, 30.);
    for key in [Key::E, Key::M] {
        for reason in 0..9 {
            let mut p = panels();
            p.timeline.collapsed_masks.insert(7);
            let mut input = Input {
                keys: vec![(key, Modifiers::default())],
                ..Default::default()
            };
            match reason {
                0 => p.project.search = true,
                1 => input.composing = true,
                2 => input.ime_consumed = true,
                3 => input.focus_lost = true,
                4 => input.key_repeats = vec![true],
                5 => p.timeline.scrubbing = true,
                6 => p.focused = Some(Panel::Nodes),
                7 => p.focused = Some(Panel::Properties),
                _ => p.panel_menu = Some((Panel::Timeline.dock(), [10., 10.])),
            }
            let mut out = vec![];
            keyboard::route(&input, &mut p, &state, reason == 6, None, &mut out);
            assert!(
                out.is_empty(),
                "key={key:?} reason={reason} actions={out:?}"
            );
            assert!(
                !p.timeline.expanded.contains(&7),
                "key={key:?} reason={reason}"
            );
            assert!(p.timeline.collapsed_masks.contains(&7));
            assert!(!p.timeline.expanded_effects.contains(&7));
        }
    }
}

#[test]
fn mixed_selections_only_reveal_real_groups_without_changing_keys_or_document_history() {
    let (engine, _) = fixture();
    let mut project = engine.project().clone();
    let mut second = project.layers[0].clone();
    second.id = 9;
    second.locked = true;
    project.layers.push(second);
    project.layers.push(Layer::camera(10, "Camera", 640, 360));
    let engine = Engine::new(project).unwrap();
    let state = state(&engine, 30.);
    let mut p = panels();
    p.selected_layers = vec![7, 8, 9, 10, 999];
    p.selected_keys
        .apply(
            &state,
            [
                key_selection::Key {
                    target: motion_core::KeyframeTarget::Effect {
                        object: 7,
                        effect: 1,
                        param: "speed".into(),
                    },
                    frame: 20,
                },
                key_selection::Key {
                    target: motion_core::KeyframeTarget::Mask {
                        object: 7,
                        mask: 1,
                        property: motion_core::masks::MaskProperty::Opacity,
                    },
                    frame: 10,
                },
            ],
            key_selection::Mode::Replace,
        )
        .unwrap();
    let selected_keys = p.selected_keys.clone();
    p.timeline.collapsed_masks.extend([7, 9]);
    assert!(invoke(&mut p, &state, Key::E, Modifiers::default()).is_empty());
    assert_eq!(p.timeline.expanded_effects, [7, 9].into_iter().collect());
    assert!(!p.timeline.expanded.contains(&8));
    assert!(!p.timeline.expanded.contains(&10));
    assert!(!p.timeline.expanded.contains(&999));
    assert!(invoke(&mut p, &state, Key::E, Modifiers::default()).is_empty());
    assert!(p.timeline.expanded_effects.is_empty());
    assert!(invoke(&mut p, &state, Key::M, Modifiers::default()).is_empty());
    assert!(!p.timeline.collapsed_masks.contains(&7) && !p.timeline.collapsed_masks.contains(&9));
    assert_eq!(p.selected_layers, [7, 8, 9, 10, 999]);
    assert!(p.selected_keys.same_selection(&selected_keys));
    assert!(!engine.can_undo());
}

#[test]
fn group_shortcuts_can_be_remapped_persisted_and_suppressed_without_stealing_other_chords() {
    use crate::shortcuts::{bindings::Bindings, Command as Shortcut, Scope};
    let (engine, _) = fixture();
    let state = state(&engine, 30.);
    let mut bindings = Bindings::default();
    let chord = motion_ui::input::KeyChord::new(Key::E, true, false, true);
    bindings
        .bind(Scope::Animation, Shortcut::ToggleEffectGroup, Some(chord))
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shortcuts.json");
    std::fs::write(&path, bindings.export_bytes().unwrap()).unwrap();
    let mut p = panels();
    p.shortcut_bindings = std::sync::Arc::new(Bindings::import(&path).unwrap());
    assert!(invoke(&mut p, &state, Key::E, Modifiers::default()).is_empty());
    assert!(!p.timeline.expanded.contains(&7));
    assert!(invoke(&mut p, &state, chord.key, chord.modifiers).is_empty());
    assert!(p.timeline.expanded_effects.contains(&7));
    assert_eq!(
        crate::shortcuts::resolve(
            Scope::Animation,
            Key::M,
            Modifiers {
                control: true,
                ..Default::default()
            },
            false
        ),
        None
    );
}
