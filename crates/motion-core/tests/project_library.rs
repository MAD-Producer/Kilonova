use motion_core::{
    project_library::{self, ItemPlacement, MAX_FOLDERS, MAX_FOLDER_DEPTH},
    Asset, Command, CompositionAction, CompositionSettings, Engine, Error, LibraryAction, Project,
    ProjectFolder, ProjectItem, MAIN_COMPOSITION,
};
use std::error::Error as _;

fn fixture() -> Project {
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    p.assets.push(Asset {
        id: 7,
        path: "assets/unused.png".into(),
        width: 4,
        height: 4,
    });
    p.edit_composition(CompositionAction::Create {
        settings: CompositionSettings {
            name: "第二个合成".into(),
            width: 32,
            height: 32,
            fps: 24,
            frames: 96,
            timing: "preserve_seconds".into(),
            shorten: "reject".into(),
        },
    })
    .unwrap();
    p.validate().unwrap();
    p
}

fn edit(e: &mut Engine, action: LibraryAction) {
    e.apply(Command::ProjectLibrary { action }).unwrap();
}
fn create(id: u64, parent: Option<u64>) -> LibraryAction {
    LibraryAction::CreateFolder {
        id,
        parent,
        name: format!("文件夹 {id}"),
    }
}
fn image() -> ProjectItem {
    ProjectItem::Image { id: 7 }
}

// Independently generated 4x4 opaque red PNG; no product or third-party asset.
const IMAGE_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4, 8, 6, 0,
    0, 0, 169, 241, 158, 126, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31,
    25, 51, 144, 46, 0, 0, 60, 64, 31, 225, 224, 129, 119, 180, 0, 0, 0, 0, 73, 69, 78, 68, 174,
    66, 96, 130,
];

#[test]
fn project_library_is_shared_by_compositions_and_keeps_orphan_assets_after_save_reopen() {
    let initial = fixture();
    let child = initial.compositions[0].id.clone();
    let mut e = Engine::new(initial.clone()).unwrap();
    edit(&mut e, create(1, None));
    edit(&mut e, create(2, Some(1)));
    edit(
        &mut e,
        LibraryAction::MoveItems {
            items: vec![image(), ProjectItem::Composition { id: child.clone() }],
            folder: Some(2),
        },
    );
    let organized = e.snapshot();
    assert_eq!(project_library::items(&organized).len(), 3);
    assert!(project_library::items(&organized).contains(&image()));
    assert!(
        organized.layers.is_empty(),
        "inventory must not rely on layer usages"
    );
    e.activate_composition(&child).unwrap();
    assert_eq!(e.project().library, organized.library);
    assert_eq!(project_library::items(e.project()).len(), 3);
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    std::fs::write(root.path().join("assets/unused.png"), IMAGE_PNG).unwrap();
    motion_core::storage::save(root.path(), e.project()).unwrap();
    let reopened = motion_core::storage::load(root.path()).unwrap();
    assert_eq!(reopened, organized);
    assert_eq!(reopened.library.folder_of(&image()), Some(2));
    let package = root.path().join("organized.msproj");
    motion_core::storage::export_package(root.path(), e.project(), &package).unwrap();
    let imported = root.path().join("imported");
    assert_eq!(
        motion_core::storage::import_package(&package, &imported).unwrap(),
        organized
    );
    assert_eq!(
        std::fs::read(imported.join("assets/unused.png")).unwrap(),
        IMAGE_PNG
    );
    assert!(e.undo().unwrap());
    assert!(e.project().library.placements.is_empty());
    assert!(e.redo().unwrap());
    assert_eq!(e.project().library, organized.library);
    e.activate_composition(MAIN_COMPOSITION).unwrap();
    assert_eq!(e.snapshot(), organized);
}

#[test]
fn project_library_rejects_stale_targets_cycles_and_invalid_names_without_losing_redo() {
    let mut e = Engine::new(fixture()).unwrap();
    edit(&mut e, create(1, None));
    edit(&mut e, create(2, Some(1)));
    edit(
        &mut e,
        LibraryAction::RenameFolder {
            id: 1,
            name: "rename".into(),
        },
    );
    e.undo().unwrap();
    let before = e.snapshot();
    let revision = e.revision();
    for action in [
        create(1, None),
        create(0, None),
        create(3, Some(999)),
        LibraryAction::MoveFolder {
            id: 1,
            parent: Some(2),
        },
        LibraryAction::MoveFolder {
            id: 1,
            parent: Some(1),
        },
        LibraryAction::MoveFolder {
            id: 999,
            parent: None,
        },
        LibraryAction::RenameFolder {
            id: 1,
            name: " \t".into(),
        },
        LibraryAction::RenameFolder {
            id: 1,
            name: "bad\nname".into(),
        },
        LibraryAction::RenameFolder {
            id: 1,
            name: "中".repeat(86),
        },
        LibraryAction::RemoveFolder { id: 999 },
        LibraryAction::MoveItems {
            items: vec![image()],
            folder: Some(999),
        },
        LibraryAction::MoveItems {
            items: vec![image(), ProjectItem::Video { id: 999 }],
            folder: Some(1),
        },
        LibraryAction::MoveItems {
            items: vec![image(), image()],
            folder: None,
        },
        LibraryAction::MoveItems {
            items: vec![],
            folder: None,
        },
    ] {
        let error = e.apply(Command::ProjectLibrary { action }).unwrap_err();
        assert!(matches!(error, Error::ProjectLibrary { .. }), "{error}");
        assert!(
            error.source().is_some(),
            "original cause must remain inspectable"
        );
        assert_eq!(e.snapshot(), before);
        assert_eq!(e.revision(), revision);
        assert!(e.can_redo());
    }
    assert!(e.redo().unwrap());
    assert_eq!(e.project().library.folders[0].name, "rename");
}

#[test]
fn project_library_folder_removal_reparents_without_deleting_items_and_undoes_once() {
    let mut e = Engine::new(fixture()).unwrap();
    for (id, parent) in [(1, None), (2, Some(1)), (3, Some(2))] {
        edit(&mut e, create(id, parent));
    }
    edit(
        &mut e,
        LibraryAction::MoveItems {
            items: vec![image()],
            folder: Some(2),
        },
    );
    let before = e.snapshot();
    edit(&mut e, LibraryAction::RemoveFolder { id: 2 });
    let after = e.snapshot();
    assert_eq!(
        after
            .library
            .folders
            .iter()
            .find(|f| f.id == 3)
            .unwrap()
            .parent,
        Some(1)
    );
    assert_eq!(after.library.folder_of(&image()), Some(1));
    assert_eq!(after.assets, before.assets);
    assert_eq!(after.compositions, before.compositions);
    assert!(e.undo().unwrap());
    assert_eq!(e.snapshot(), before);
    assert!(e.redo().unwrap());
    assert_eq!(e.snapshot(), after);
    edit(&mut e, LibraryAction::RemoveFolder { id: 1 });
    assert_eq!(e.project().library.folder_of(&image()), None);
    assert_eq!(e.project().library.folders[0].parent, None);
    assert_eq!(e.project().assets, before.assets);
}

#[test]
fn project_library_moves_are_atomic_and_no_op_moves_do_not_create_history() {
    let mut e = Engine::new(fixture()).unwrap();
    edit(&mut e, create(1, None));
    let targets = vec![
        image(),
        ProjectItem::Composition {
            id: MAIN_COMPOSITION.into(),
        },
    ];
    edit(
        &mut e,
        LibraryAction::MoveItems {
            items: targets.clone(),
            folder: Some(1),
        },
    );
    let before = e.snapshot();
    let revision = e.revision();
    edit(
        &mut e,
        LibraryAction::MoveItems {
            items: targets.into_iter().rev().collect(),
            folder: Some(1),
        },
    );
    assert_eq!(e.snapshot(), before);
    assert_eq!(e.revision(), revision);
    assert!(e
        .apply_batch(vec![
            Command::ProjectLibrary {
                action: create(2, None)
            },
            Command::ProjectLibrary {
                action: LibraryAction::MoveItems {
                    items: vec![ProjectItem::Image { id: 999 }],
                    folder: Some(2),
                }
            },
        ])
        .is_err());
    assert_eq!(e.snapshot(), before);
    assert_eq!(e.revision(), revision);
    e.begin_gesture().unwrap();
    assert!(matches!(
        e.apply(Command::ProjectLibrary {
            action: create(2, None)
        }),
        Err(Error::Gesture)
    ));
    e.end_gesture(false).unwrap();
    assert_eq!(e.snapshot(), before);
}

#[test]
fn project_library_resource_and_composition_removal_clean_only_their_placements() {
    let mut e = Engine::new(fixture()).unwrap();
    let child = e.project().compositions[0].id.clone();
    edit(&mut e, create(1, None));
    edit(
        &mut e,
        LibraryAction::MoveItems {
            items: vec![image(), ProjectItem::Composition { id: child.clone() }],
            folder: Some(1),
        },
    );
    let organized = e.snapshot();
    e.apply(Command::MediaAsset {
        action: motion_core::media_assets::AssetAction::Remove {
            target: motion_core::media_assets::AssetRef {
                kind: motion_core::media_assets::AssetKind::Image,
                id: 7,
            },
        },
    })
    .unwrap();
    assert_eq!(e.project().library.placements.len(), 1);
    assert!(e.project().assets.is_empty());
    e.undo().unwrap();
    assert_eq!(e.snapshot(), organized);
    e.apply(Command::Composition {
        action: CompositionAction::Delete { target: child },
    })
    .unwrap();
    assert_eq!(e.project().library.placements.len(), 1);
    assert_eq!(e.project().library.folder_of(&image()), Some(1));
    e.undo().unwrap();
    assert_eq!(e.snapshot(), organized);
}

#[test]
fn project_library_required_current_schema_rejects_corrupt_organization_without_pruning() {
    let mut p = fixture();
    p.library.folders.push(ProjectFolder {
        id: 1,
        parent: None,
        name: "sources".into(),
    });
    let clean = p.clone();
    for placement in [
        ItemPlacement {
            item: image(),
            folder: 99,
        },
        ItemPlacement {
            item: ProjectItem::Image { id: 99 },
            folder: 1,
        },
    ] {
        p.library.placements = vec![placement];
        let before = p.clone();
        assert!(p.validate().is_err());
        assert_eq!(p, before);
    }
    p = clean;
    p.library.placements = vec![
        ItemPlacement {
            item: image(),
            folder: 1
        };
        2
    ];
    assert!(p.validate().is_err());
    let mut value = serde_json::to_value(fixture()).unwrap();
    value.as_object_mut().unwrap().remove("library");
    assert!(serde_json::from_value::<Project>(value)
        .unwrap_err()
        .to_string()
        .contains("library"));
    let mut old = fixture();
    old.version -= 1;
    assert!(old.validate().is_err());
}

#[test]
fn project_library_enforces_folder_and_depth_budgets_at_the_boundary() {
    let mut p = fixture();
    p.library.folders = (1..=MAX_FOLDER_DEPTH as u64)
        .map(|id| ProjectFolder {
            id,
            parent: (id > 1).then_some(id - 1),
            name: format!("folder {id}"),
        })
        .collect();
    p.validate().unwrap();
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    assert!(e
        .apply(Command::ProjectLibrary {
            action: create(99, Some(MAX_FOLDER_DEPTH as u64))
        })
        .is_err());
    assert_eq!(e.snapshot(), before);
    let mut p = fixture();
    p.library.folders = (1..=MAX_FOLDERS as u64)
        .map(|id| ProjectFolder {
            id,
            parent: None,
            name: format!("folder {id}"),
        })
        .collect();
    p.validate().unwrap();
    let mut e = Engine::new(p).unwrap();
    let before = e.snapshot();
    assert!(e
        .apply(Command::ProjectLibrary {
            action: create(MAX_FOLDERS as u64 + 1, None)
        })
        .is_err());
    assert_eq!(e.snapshot(), before);
}

#[test]
fn project_library_command_transport_uses_shared_scope_and_required_schema() {
    let mut e = Engine::new(fixture()).unwrap();
    let child = e.project().compositions[0].id.clone();
    let requests = serde_json::json!([
        {"op":"project_library","composition":child,"action":{
            "op":"create_folder","id":8,"parent":null,"name":"素材"}},
        {"op":"project_library","action":{
            "op":"move_items","items":[{"kind":"image","id":7}],"folder":8}},
    ]);
    let commands = motion_core::CommandRegistry::parse(&requests.to_string()).unwrap();
    motion_core::CommandRegistry::run(&mut e, commands).unwrap();
    assert_eq!(e.project().composition_id, MAIN_COMPOSITION);
    assert_eq!(e.project().library.folder_of(&image()), Some(8));
    assert!(e.undo().unwrap());
    assert!(e.project().library.folders.is_empty());
    assert!(!e.can_undo());
    let catalogue = motion_core::CommandRegistry::catalogue();
    let schema = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "project_library")
        .unwrap();
    assert_eq!(schema["undoable"], true);
    assert!(schema["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k == "action"));
    for text in [
        r#"{"op":"project_library","action":{"op":"create_folder","id":1,"name":"Folder","unexpected":true}}"#,
        r#"{"op":"project_library","action":{"op":"move_items","folder":null}}"#,
    ] {
        assert!(motion_core::CommandRegistry::parse(text).is_err(), "{text}");
    }
}
