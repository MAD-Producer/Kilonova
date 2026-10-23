use super::*;
use motion_ui::{dock::Panel, paint::PaintList};
use serde_json::json;

fn fixture(root: &std::path::Path, locale: Locale) -> Shell {
    let mut project = motion_core::Project::new(320, 180, 30, 90).unwrap();
    for (id, name, color) in [
        (1, "Blue", [0.2, 0.5, 0.8, 1.]),
        (2, "Orange", [0.9, 0.5, 0.1, 1.]),
        (3, "Locked", [0.4, 0.4, 0.4, 1.]),
    ] {
        let mut layer = Layer::solid(id, name, [75., 60.], [id as f32 * 75., 90., 0.], color);
        layer.locked = id == 3;
        project.layers.push(layer);
    }
    motion_core::storage::save(root, &project).unwrap();
    let mut shell = Shell::new(root.into(), None, Some(locale)).unwrap();
    shell.home = None;
    shell.workspace = workspace::Workspace::default();
    shell.workspace.locale = locale;
    shell.panels.locale = locale;
    shell.panels.select_only(Some(1));
    shell.panels.add_selection(2);
    shell.panels.selected = Some(1);
    shell.panels.browser.catalogue = shell.panels.effect_controls.catalogue.clone();
    shell.panels.browser.preview_categories(locale);
    shell.atlas = motion_ui::text::TextAtlas::from_family(
        motion_ui::style::default_stylesheet_arc().font_family(),
        18.,
        true,
    )
    .unwrap();
    shell
}
fn draw(
    shell: &mut Shell,
    owner: Option<Panel>,
    size: [f32; 2],
    input: &Input,
) -> (PaintList, Vec<panels::Action>) {
    let mut list = PaintList::default();
    let out = if let Some(panel) = owner {
        panels::floating_chrome(
            &mut list,
            &mut shell.atlas,
            panel,
            size,
            &mut shell.panels,
            &shell.state,
            input,
        )
    } else {
        panels::chrome(
            &mut list,
            &mut shell.atlas,
            size,
            shell.workspace.display_root(),
            &mut shell.panels,
            &shell.state,
            input,
        )
    };
    (list, out)
}
fn center(r: motion_ui::Rect) -> [f32; 2] {
    [(r.min[0] + r.max[0]) * 0.5, (r.min[1] + r.max[1]) * 0.5]
}
fn begin(shell: &mut Shell, owner: Option<Panel>, size: [f32; 2]) -> [f32; 2] {
    assert!(draw(shell, owner, size, &Input::default()).1.is_empty());
    let entry = panels::effect_controls::entries(
        &shell.panels.effect_controls.catalogue,
        shell.panels.locale,
    )
    .into_iter()
    .find(|e| e.id == "glow")
    .unwrap();
    let rect = shell
        .panels
        .browser
        .library_row(&entry.key)
        .expect("visible actual effect row");
    let point = [rect.min[0] + 50., (rect.min[1] + rect.max[1]) * 0.5];
    assert!(draw(
        shell,
        owner,
        size,
        &Input {
            mouse: point,
            held: Some(MouseButton::Left),
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        }
    )
    .1
    .is_empty());
    assert!(shell.panels.effect_drag.pending());
    point
}
#[test]
fn effect_drag_real_panel_routes_batch_once_roundtrip_undo_and_preserve_shortcuts() {
    for locale in [Locale::Zh, Locale::En] {
        for target in [None, Some(Panel::Timeline), Some(Panel::Composition)] {
            let folder = tempfile::tempdir().unwrap();
            let root = folder.path().join("project");
            let mut shell = fixture(&root, locale);
            let before = shell.state["project"].clone();
            begin(&mut shell, Some(Panel::Effects), [320., 500.]);
            let size = [900., 600.];
            // A normal repaint collects the real current layout geometry without starting edits.
            assert!(draw(&mut shell, target, size, &Input::default())
                .1
                .is_empty());
            let rect = shell
                .panels
                .effect_drag
                .target_rect(if target == Some(Panel::Composition) {
                    None
                } else {
                    Some(1)
                })
                .unwrap();
            let point = center(rect);
            shell.panels.effect_drag.pointer(target, point, false, 3.);
            let selected = shell.panels.selection_ids();
            let input = Input {
                keys: vec![
                    (Key::Space, Default::default()),
                    (Key::Delete, Default::default()),
                    (
                        Key::A,
                        motion_ui::input::Modifiers {
                            control: true,
                            ..Default::default()
                        },
                    ),
                ],
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                mouse: point,
                ..Default::default()
            };
            assert!(draw(&mut shell, target, size, &input).1.is_empty());
            assert!(
                shell
                    .panels
                    .effect_drag
                    .status()
                    .is_some_and(|s| s.contains("Blue") && s.contains('2')),
                "active drag status describes current targets instead of a previous success"
            );
            assert_eq!(shell.panels.selection_ids(), selected);
            assert_eq!(shell.engine.state().unwrap()["project"], before);
            shell.panels.effect_drag.pointer(target, point, true, 3.);
            let actions = draw(&mut shell, target, size, &Input::default()).1;
            assert!(
                matches!(actions.as_slice(),[panels::Action::Plugin(v)]if v["objects"]==json!([1,2])&&v["composition"]==shell.state["composition"])
            );
            for action in actions {
                shell.action(action, None).unwrap();
            }
            let after = shell.state["project"].clone();
            assert_eq!(after["layers"][0]["effects"].as_array().unwrap().len(), 1);
            assert_eq!(after["layers"][1]["effects"].as_array().unwrap().len(), 1);
            assert_eq!(after["layers"][2]["effects"].as_array().unwrap().len(), 0);
            assert!(draw(&mut shell, target, size, &Input::default())
                .1
                .is_empty());
            shell.engine.save().unwrap();
            assert_eq!(
                serde_json::to_value(motion_core::storage::load(&root).unwrap()).unwrap(),
                after
            );
            shell.state = shell.engine.history(0).unwrap();
            assert_eq!(shell.state["project"], before);
            shell.state = shell.engine.history(1).unwrap();
            assert_eq!(shell.state["project"], after);
        }
    }
}
#[test]
fn effect_drag_rejects_locked_row_and_cancels_escape_without_history_or_delayed_drop() {
    let folder = tempfile::tempdir().unwrap();
    let mut shell = fixture(&folder.path().join("project"), Locale::Zh);
    let before = shell.state["project"].clone();
    begin(&mut shell, Some(Panel::Effects), [320., 500.]);
    draw(
        &mut shell,
        Some(Panel::Timeline),
        [900., 600.],
        &Input::default(),
    );
    let rect = shell.panels.effect_drag.target_rect(Some(3)).unwrap();
    let point = center(rect);
    shell
        .panels
        .effect_drag
        .pointer(Some(Panel::Timeline), point, true, 3.);
    assert!(draw(
        &mut shell,
        Some(Panel::Timeline),
        [900., 600.],
        &Input::default()
    )
    .1
    .is_empty());
    assert!(shell.panels.message.as_deref().unwrap().contains("锁定"));
    begin(&mut shell, Some(Panel::Effects), [320., 500.]);
    shell
        .panels
        .effect_drag
        .pointer(None, [400., 300.], false, 3.);
    assert!(draw(
        &mut shell,
        None,
        [900., 600.],
        &Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        }
    )
    .1
    .is_empty());
    assert!(!shell.panels.effect_drag.pending());
    shell
        .panels
        .effect_drag
        .pointer(None, [100., 500.], true, 3.);
    assert!(draw(&mut shell, None, [900., 600.], &Input::default())
        .1
        .is_empty());
    assert_eq!(shell.engine.state().unwrap()["project"], before);
    assert_eq!(shell.engine.history(0).unwrap()["project"], before);
}

#[cfg(target_os = "windows")]
mod native;
