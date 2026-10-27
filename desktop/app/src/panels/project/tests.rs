use super::*;
use motion_core::project_library::{ItemPlacement, ProjectFolder};
use motion_core::{Asset, Composition, Project};
use motion_ui::input::Modifiers;

fn fixture() -> Value {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    p.name = "主合成 Main".into();
    p.compositions.push(Composition {
        id: "comp-detail".into(),
        name: "细节 Detail".into(),
        width: 640,
        height: 360,
        fps: 30,
        frames: 120,
        work_area: p.work_area.clone(),
        guides: vec![],
        background: p.background,
        camera: p.camera.clone(),
        layers: vec![],
        expressions: vec![],
    });
    for id in 1..=3 {
        p.assets.push(Asset {
            id,
            path: format!("assets/source-{id}.png"),
            width: 4,
            height: 4,
        });
    }
    p.library.folders = vec![
        ProjectFolder {
            id: 7,
            parent: None,
            name: "素材 Assets".into(),
        },
        ProjectFolder {
            id: 8,
            parent: Some(7),
            name: "图片 Images".into(),
        },
    ];
    p.library.placements = vec![
        ItemPlacement {
            item: ProjectItem::Image { id: 2 },
            folder: 7,
        },
        ItemPlacement {
            item: ProjectItem::Image { id: 3 },
            folder: 8,
        },
        ItemPlacement {
            item: ProjectItem::Composition {
                id: "comp-detail".into(),
            },
            folder: 7,
        },
    ];
    p.validate().unwrap();
    serde_json::json!({"root":"project-inventory-fixture","document_generation":17,"revision":3,"project":p,"frame":0})
}
fn point(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn click(rect: Rect, button: MouseButton) -> Input {
    let point = point(rect);
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
fn draw(panels: &mut Panels, state: &Value, owner: Option<Panel>, input: Input) -> Vec<Action> {
    let mut list = PaintList::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    if let Some(panel) = owner {
        super::super::floating_chrome(
            &mut list,
            &mut atlas,
            panel,
            [600., 420.],
            panels,
            state,
            &input,
        )
    } else {
        super::super::chrome(
            &mut list,
            &mut atlas,
            [900., 600.],
            &Node::dock(Panel::Project.dock()),
            panels,
            state,
            &input,
        )
    }
}
fn rect(panels: &Panels, key: &RowKey) -> Rect {
    panels
        .project
        .browser
        .rows
        .iter()
        .find(|(k, _)| k == key)
        .unwrap()
        .1
}

#[test]
fn project_inventory_lists_unreferenced_items_and_all_compositions_without_decoding_animation_payloads(
) {
    let mut state = fixture();
    let catalogue = Catalogue::read(&state).unwrap();
    assert_eq!(catalogue.entries.len(), 7);
    let view = ViewPreferences {
        folder: Some(7),
        ..Default::default()
    };
    let visible = catalogue.visible(&view, "");
    assert!(visible.iter().any(|entry| entry.key
        == RowKey::Item(ProjectItem::Composition {
            id: "comp-detail".into()
        })));
    assert!(visible
        .iter()
        .any(|entry| entry.key == RowKey::Item(ProjectItem::Image { id: 2 })));
    assert!(!visible
        .iter()
        .any(|entry| entry.key == RowKey::Item(ProjectItem::Image { id: 3 })));
    assert_eq!(catalogue.visible(&view, "source").len(), 1);
    assert_eq!(
        catalogue
            .visible(
                &ViewPreferences {
                    search_all: true,
                    ..view
                },
                "source"
            )
            .len(),
        3
    );
    assert!(!catalogue.allows_move(&RowKey::Folder(7), Some(7)));
    assert!(!catalogue.allows_move(&RowKey::Folder(7), Some(8)));
    assert!(catalogue.allows_move(&RowKey::Folder(8), None));
    // A projection must not parse or clone the untouched keyframe/shape graph.
    state["project"]["layers"] = serde_json::json!({"not-a-layer-array":"irrelevant to inventory"});
    state["project"]["compositions"][0]["layers"] = serde_json::json!([{"tracks":"irrelevant"}]);
    assert_eq!(Catalogue::read(&state).unwrap().entries.len(), 7);
}

#[test]
fn project_folder_menus_names_and_navigation_are_stamped_and_identical_in_all_window_owners_and_languages(
) {
    for locale in [Locale::Zh, Locale::En] {
        for owner in [None, Some(Panel::Project)] {
            let state = fixture();
            let mut panels = Panels::default();
            panels.locale = locale;
            panels.focused = Some(Panel::Project);
            assert!(draw(&mut panels, &state, owner, Input::default()).is_empty());
            let folder = rect(&panels, &RowKey::Folder(7));
            assert!(draw(
                &mut panels,
                &state,
                owner,
                click(folder, MouseButton::Right)
            )
            .is_empty());
            let rename = panels.project.browser.buttons[1];
            assert!(draw(&mut panels, &state, owner, click(rename, MouseButton::Left)).is_empty());
            assert!(panels.project.browser.modal());
            draw(&mut panels, &state, owner, Input::default());
            // Preedit is not a second committed Input.text payload.
            let composing = Input {
                composing: true,
                keys: vec![(Key::Enter, Modifiers::default())],
                ..Default::default()
            };
            assert!(draw(&mut panels, &state, owner, composing).is_empty());
            assert!(panels.project.browser.modal());
            assert!(draw(
                &mut panels,
                &state,
                owner,
                Input {
                    text: "目录 New name".into(),
                    ..Default::default()
                }
            )
            .is_empty());
            let actions = draw(&mut panels, &state, owner, key(Key::Enter));
            assert!(
                matches!(actions.as_slice(),[Action::EditorAction{source,commands}] if *source==crate::layer_clipboard::KeyframeSource::of(&state)
            && matches!(commands.as_slice(),[Command::ProjectLibrary{action:LibraryAction::RenameFolder{id:7,name}}] if name=="目录 New name")),
                "{actions:?}"
            );
            assert!(!panels.project.browser.modal());
            assert!(draw(&mut panels, &state, owner, click(folder, MouseButton::Left)).is_empty());
            let actions = draw(&mut panels, &state, owner, key(Key::Enter));
            assert!(
                matches!(actions.as_slice(),[Action::ProjectViewChanged(view)] if view.folder==Some(7)),
                "{actions:?}"
            );
            draw(&mut panels, &state, owner, Input::default());
            let detail = rect(
                &panels,
                &RowKey::Item(ProjectItem::Composition {
                    id: "comp-detail".into(),
                }),
            );
            draw(&mut panels, &state, owner, click(detail, MouseButton::Left));
            let actions = draw(&mut panels, &state, owner, key(Key::Enter));
            assert!(
                matches!(actions.as_slice(),[Action::ProjectOpen{composition,..}] if composition=="comp-detail"),
                "{actions:?}"
            );
            assert_eq!(
                panels.selected, None,
                "selecting a source must not select a layer"
            );
        }
    }
}

#[test]
fn project_folder_popups_cancel_stale_sources_outside_clicks_and_cross_window_input_without_leaking_actions(
) {
    for owner in [None, Some(Panel::Project)] {
        for change in 0..5 {
            let mut state = fixture();
            let mut panels = Panels::default();
            panels.focused = Some(Panel::Project);
            draw(&mut panels, &state, owner, Input::default());
            let add = panels.project.browser.buttons[1];
            assert!(draw(&mut panels, &state, owner, click(add, MouseButton::Left)).is_empty());
            assert!(
                panels.project.browser.modal(),
                "opening click must not cancel the popup"
            );
            let input = match change {
                0 => {
                    state["revision"] = 4.into();
                    key(Key::Enter)
                }
                1 => {
                    state["document_generation"] = 18.into();
                    key(Key::Enter)
                }
                2 => {
                    state["root"] = "another-project".into();
                    key(Key::Enter)
                }
                3 => Input {
                    focus_lost: true,
                    ..Default::default()
                },
                _ => Input {
                    mouse: [0., 0.],
                    pressed: Some((MouseButton::Left, [0., 0.])),
                    ..Default::default()
                },
            };
            assert!(draw(&mut panels, &state, owner, input).is_empty());
            assert!(!panels.project.browser.modal());
            draw(&mut panels, &state, owner, Input::default());
            let add = panels.project.browser.buttons[1];
            draw(&mut panels, &state, owner, click(add, MouseButton::Left));
            let other = if owner.is_some() {
                None
            } else {
                Some(Panel::Project)
            };
            assert!(draw(&mut panels, &state, other, key(Key::Enter)).is_empty());
            assert!(!panels.project.browser.modal());
        }
    }
}

#[test]
fn project_view_preferences_are_bounded_and_saved_scope_never_applies_to_a_different_project() {
    let state = fixture();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let view = ViewPreferences {
        root: "project-inventory-fixture".into(),
        folder: Some(8),
        query: "source".into(),
        search_all: true,
    };
    let mut workspace = crate::workspace::Workspace::default();
    workspace.project_view = view.clone();
    workspace.save(&path).unwrap();
    let reopened = crate::workspace::Workspace::load(&path);
    assert_eq!(reopened.project_view, view);
    let mut panels = Panels::default();
    panels.project.browser.view = reopened.project_view;
    panels.project.query = view.query.clone();
    draw(&mut panels, &state, None, Input::default());
    assert_eq!(panels.project.browser.view, view);
    let mut other = state.clone();
    other["root"] = "another-project".into();
    draw(&mut panels, &other, None, Input::default());
    assert_eq!(panels.project.browser.view.folder, None);
    assert!(panels.project.query.is_empty());
    let mut invalid = view;
    invalid.query = "x".repeat(257);
    workspace.project_view = invalid;
    assert!(!workspace.valid());
}

#[test]
fn project_move_popup_ignores_repeat_and_ime_and_never_accepts_its_own_descendant() {
    for owner in [None, Some(Panel::Project)] {
        let state = fixture();
        let mut panels = Panels::default();
        panels.focused = Some(Panel::Project);
        draw(&mut panels, &state, owner, Input::default());
        let row = rect(&panels, &RowKey::Folder(7));
        draw(&mut panels, &state, owner, click(row, MouseButton::Left));
        let mv = panels.project.browser.buttons[2];
        assert!(draw(&mut panels, &state, owner, click(mv, MouseButton::Left)).is_empty());
        assert!(panels.project.browser.modal());
        for input in [
            Input {
                keys: vec![(Key::Enter, Modifiers::default())],
                key_repeats: vec![true],
                ..Default::default()
            },
            Input {
                keys: vec![(Key::Enter, Modifiers::default())],
                composing: true,
                ..Default::default()
            },
            Input {
                keys: vec![(Key::Enter, Modifiers::default())],
                ime_consumed: true,
                ..Default::default()
            },
        ] {
            assert!(draw(&mut panels, &state, owner, input).is_empty());
            assert!(panels.project.browser.modal());
        }
        // Disabled destinations are skipped by keyboard navigation.
        assert!(draw(&mut panels, &state, owner, key(Key::Down)).is_empty());
        let actions = draw(&mut panels, &state, owner, key(Key::Enter));
        assert!(
            matches!(actions.as_slice(),[Action::EditorAction{commands,..}] if matches!(commands.as_slice(),[Command::ProjectLibrary {action:LibraryAction::MoveFolder{id:7,parent:None}}])),
            "{actions:?}"
        );
    }
}

#[test]
fn project_folder_context_keyboard_uses_plain_nonrepeated_keys_and_consumes_its_closing_frame() {
    let state = fixture();
    let mut panels = Panels::default();
    panels.focused = Some(Panel::Project);
    draw(&mut panels, &state, None, Input::default());
    let folder = rect(&panels, &RowKey::Folder(7));
    draw(&mut panels, &state, None, click(folder, MouseButton::Right));
    for input in [
        Input {
            keys: vec![(Key::Enter, Modifiers::default())],
            key_repeats: vec![true],
            ..Default::default()
        },
        Input {
            keys: vec![(Key::Enter, Modifiers::default())],
            composing: true,
            ..Default::default()
        },
    ] {
        assert!(draw(&mut panels, &state, None, input).is_empty());
        assert!(panels.project.context_menu.is_some());
    }
    draw(&mut panels, &state, None, key(Key::Down));
    assert!(draw(&mut panels, &state, None, key(Key::Enter)).is_empty());
    assert!(panels.project.browser.modal());
    assert!(draw(&mut panels, &state, None, key(Key::Escape)).is_empty());
    assert!(!panels.project.browser.modal());
}

#[cfg(target_os = "windows")]
mod native;

#[test]
fn project_inventory_decode_failure_is_bounded_observed_once_per_snapshot_and_recovers_at_default_level(
) {
    const CHILD: &str = "MOTION_PROJECT_INVENTORY_ERROR_CHILD";
    const NAME:&str="panels::project::tests::project_inventory_decode_failure_is_bounded_observed_once_per_snapshot_and_recovers_at_default_level";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut panels = Panels::default();
        panels.project.browser.view.root = "project-inventory-fixture".into();
        panels.project.query = "query-must-not-be-logged".into();
        let mut broken = fixture();
        broken["project"]["library"] =
            serde_json::json!({"folders":"bad".repeat(4096),"placements":[]});
        for _ in 0..60 {
            assert!(draw(&mut panels, &broken, None, Input::default()).is_empty());
        }
        let mut good = fixture();
        panels.project.query.clear();
        good["revision"] = 4.into();
        draw(&mut panels, &good, None, Input::default());
        assert_eq!(panels.project.browser.rows.len(), 3);
        broken["revision"] = 5.into();
        draw(&mut panels, &broken, None, Input::default());
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PROJECT_INVENTORY_ERROR_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<_> = file
        .lines()
        .filter(|line| line.contains("project inventory"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.len() < 1024
        && line.contains(" ERROR ")
        && line.contains("phase=decode")));
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.contains("project inventory"))
            .count(),
        2
    );
    assert!(!stdout.contains("project inventory"));
    assert!(!file.contains("query-must-not-be-logged"));
}

#[test]
fn project_folder_creation_uses_a_free_bounded_id_even_when_existing_ids_are_maximal() {
    let mut state = fixture();
    state["project"]["library"]["folders"][0]["id"] = u64::MAX.into();
    state["project"]["library"]["folders"][1]["parent"] = u64::MAX.into();
    for placement in state["project"]["library"]["placements"]
        .as_array_mut()
        .unwrap()
    {
        if placement["folder"] == 7 {
            placement["folder"] = u64::MAX.into();
        }
    }
    serde_json::from_value::<Project>(state["project"].clone())
        .unwrap()
        .validate()
        .unwrap();
    let mut panels = Panels::default();
    draw(&mut panels, &state, None, Input::default());
    let add = panels.project.browser.buttons[1];
    draw(&mut panels, &state, None, click(add, MouseButton::Left));
    let actions = draw(&mut panels, &state, None, key(Key::Enter));
    assert!(
        matches!(actions.as_slice(),[Action::EditorAction{commands,..}] if matches!(commands.as_slice(),[Command::ProjectLibrary{action:LibraryAction::CreateFolder{id:1,..}}])),
        "{actions:?}"
    );
}
