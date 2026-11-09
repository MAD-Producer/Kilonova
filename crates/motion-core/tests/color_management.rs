use motion_core::{color_management::*, Command, Engine, Error, Project};
use motion_effects::{
    color_config::*,
    image_contract::{DataChannels, ImageMeaning},
};
use std::{error::Error as _, sync::Arc};
#[path = "support/color_project.rs"]
mod fixture;

#[test]
fn explicit_default_and_current_format_never_invent_missing_color_settings() {
    let original = Project::new(64, 64, 30, 120).unwrap();
    assert_eq!(
        *original.color_management,
        ProjectColorManagement::Unmanaged {}
    );
    let mut json = serde_json::to_value(&original).unwrap();
    assert_eq!(
        json["color_management"],
        serde_json::json!({"kind":"unmanaged"})
    );
    json.as_object_mut().unwrap().remove("color_management");
    assert!(serde_json::from_value::<Project>(json)
        .unwrap_err()
        .to_string()
        .contains("color_management"));
    let mut old = original.clone();
    old.version -= 1;
    assert!(old
        .validate()
        .unwrap_err()
        .to_string()
        .contains("unsupported project format"));
    for text in [
        r#"{"op":"set_color_management"}"#,
        r#"{"op":"set_color_management","settings":{"kind":"unmanaged","ignored":true}}"#,
        r#"{"op":"set_color_management","settings":{"kind":"ocio"}}"#,
    ] {
        assert!(motion_core::parse_commands(text).is_err(), "{text}");
    }
}

#[test]
fn declared_and_verified_identities_agree_without_claiming_files_exist() {
    let settings = fixture::settings();
    let expected = settings
        .manifest
        .resolve_declared(&settings.selection)
        .unwrap();
    let config = VerifiedColorConfig::new(
        settings.manifest.clone(),
        Arc::from(fixture::CONFIG),
        std::collections::BTreeMap::from([("looks/example.cube".into(), Arc::from(fixture::LUT))]),
    )
    .unwrap();
    assert_eq!(settings.manifest.declared_key().unwrap(), config.key());
    assert_eq!(config.resolve(&settings.selection).unwrap(), expected);
    assert!(matches!(
        VerifiedColorConfig::new(
            settings.manifest,
            Arc::from(fixture::CONFIG),
            Default::default()
        ),
        Err(ColorConfigError::Resource {
            reason: "missing resource",
            ..
        })
    ));
}

#[test]
fn full_project_roundtrip_history_and_atomic_failure_keep_color_identity() {
    let before = Project::new(64, 64, 30, 120).unwrap();
    let mut engine = Engine::new(before.clone()).unwrap();
    let settings = fixture::managed(fixture::settings());
    let text = serde_json::to_string(&Command::SetColorManagement {
        settings: settings.clone(),
    })
    .unwrap();
    engine
        .apply_batch(motion_core::parse_commands(&text).unwrap())
        .unwrap();
    let mut expected = before.clone();
    expected.color_management = settings;
    assert_eq!(engine.snapshot(), expected);
    let folder = tempfile::tempdir().unwrap();
    fixture::write_resources(folder.path(), &expected);
    motion_core::storage::save(folder.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(folder.path()).unwrap(), expected);
    let diagnostics = engine.history_diagnostics();
    let revision = engine.revision();
    let rejected = engine
        .apply_batch(vec![
            Command::SetColorManagement {
                settings: before.color_management.clone(),
            },
            Command::SetWorkArea {
                in_frame: 120,
                out_frame: 120,
            },
        ])
        .unwrap_err();
    assert!(rejected.to_string().contains("invalid work area"));
    assert_eq!(engine.snapshot(), expected);
    assert_eq!(engine.history_diagnostics(), diagnostics);
    assert_eq!(engine.revision(), revision);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), before);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.snapshot(), expected);
}

#[test]
fn composition_scopes_share_color_identity_and_snapshots_share_the_payload() {
    use motion_core::{CompositionAction, CompositionSettings};
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    let result = project
        .edit_composition(CompositionAction::Create {
            settings: CompositionSettings {
                name: "Child".into(),
                width: 32,
                height: 32,
                fps: motion_core::FrameRate::integer(24).unwrap(),
                frames: 48,
                timing: "preserve_seconds".into(),
                shorten: "reject".into(),
            },
        })
        .unwrap();
    let child = result["composition"].as_str().unwrap().to_owned();
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::InComposition {
            composition: child,
            command: Box::new(Command::SetColorManagement {
                settings: fixture::managed(fixture::settings()),
            }),
        })
        .unwrap();
    assert_eq!(
        engine.project().composition_id,
        motion_core::MAIN_COMPOSITION
    );
    for id in engine.project().composition_ids() {
        let view = engine.project().composition_frame_view(&id).unwrap();
        assert_eq!(view.color_management, engine.project().color_management);
        assert!(Arc::ptr_eq(
            &view.color_management,
            &engine.project().color_management
        ));
    }
    assert!(Arc::ptr_eq(
        &engine.snapshot().color_management,
        &engine.project().color_management
    ));
    let catalogue = motion_core::CommandRegistry::catalogue();
    let descriptor = catalogue["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "set_color_management")
        .unwrap();
    assert_eq!(descriptor["undoable"], true);
    assert!(descriptor["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("settings")));
}

#[test]
fn config_roles_hash_aces_and_signal_mismatches_remain_typed_and_atomic() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    let before = engine.snapshot();
    for case in 0..7 {
        let mut settings = fixture::settings();
        match case {
            0 => settings.selection.config_key[0] ^= 1,
            1 => {
                settings.selection.working = SpaceReference::Role {
                    name: "missing".into(),
                }
            }
            2 => settings.selection.input = fixture::space("raw"),
            3 => {
                settings.selection.aces = Some(ColorVersion {
                    major: 2,
                    minor: 1,
                    patch: 0,
                })
            }
            4 => settings.working.reference = ReferenceSpace::Display,
            5 => settings.default_input.encoding = "log".into(),
            6 => settings.selection.display.view = "missing".into(),
            _ => unreachable!(),
        }
        let error = engine
            .apply(Command::SetColorManagement {
                settings: fixture::managed(settings),
            })
            .unwrap_err();
        if case < 4 || case == 6 {
            let Error::ProjectColor(ProjectColorError::Config(native)) = &error else {
                panic!("{error:?}")
            };
            assert!(
                error
                    .source()
                    .unwrap()
                    .downcast_ref::<ColorConfigError>()
                    .is_some()
                    || error
                        .source()
                        .unwrap()
                        .downcast_ref::<ProjectColorError>()
                        .is_some()
            );
            match case {
                0 => assert!(matches!(native, ColorConfigError::ConfigMismatch)),
                1 | 6 => assert!(matches!(native, ColorConfigError::Missing { .. })),
                2 => assert!(matches!(native, ColorConfigError::DataSpace { .. })),
                3 => assert!(matches!(native, ColorConfigError::AcesMismatch)),
                _ => unreachable!(),
            }
        }
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.revision(), 0);
        assert!(!engine.can_undo());
    }
    for version in [
        ColorVersion {
            major: 1,
            minor: 3,
            patch: 0,
        },
        ColorVersion {
            major: 2,
            minor: 1,
            patch: 0,
        },
    ] {
        let mut settings = fixture::settings();
        settings.manifest.origin = ConfigOrigin::Builtin {
            id: "declared-schema-fixture".into(),
            aces: version,
        };
        settings.selection.aces = Some(version);
        fixture::rekey(&mut settings);
        settings.validate(&before).unwrap();
    }
}

#[test]
fn source_color_data_and_range_declarations_reject_ambiguity() {
    let mut project = fixture::project();
    project.assets = vec![motion_core::Asset {
        id: 7,
        path: "assets/source.png".into(),
        width: 1,
        height: 1,
    }];
    let mut settings = fixture::settings();
    let input = settings.authored_input.clone();
    settings.sources.push(SourceColorAssignment {
        asset: 7,
        interpretation: SourceInterpretation::Color { input },
    });
    settings.validate(&project).unwrap();
    settings.sources[0].interpretation = SourceInterpretation::Data {
        meaning: ImageMeaning::Data {
            channels: DataChannels::Vector4,
        },
    };
    settings.validate(&project).unwrap();
    settings.sources.push(settings.sources[0].clone());
    assert!(settings
        .validate(&project)
        .unwrap_err()
        .to_string()
        .contains("asset=Some(7)"));
    settings.sources.pop();
    settings.sources[0].asset = 8;
    assert!(settings.validate(&project).is_err());
    settings.sources[0].asset = 7;
    settings.sources[0].interpretation = SourceInterpretation::Data {
        meaning: ImageMeaning::Color {
            space: motion_effects::WorkingSpace::Linear,
            alpha: motion_effects::AlphaMode::Straight,
        },
    };
    assert!(settings.validate(&project).is_err());
    let mut signal = settings.working.clone();
    // ACES AP0 imaginary primary is legal; this is validation, not a gamut conversion.
    signal.primaries = [[0.7347, 0.2653], [0., 1.], [0.0001, -0.077]];
    signal.white_point = [0.32168, 0.33767];
    signal.validate().unwrap();
    for range in [
        SignalRange::Limited {
            black: 0.5,
            white: 0.1,
        },
        SignalRange::Limited {
            black: f64::NAN,
            white: 1.,
        },
    ] {
        signal.range = range;
        assert!(signal.validate().is_err());
    }
    signal.range = SignalRange::Limited {
        black: 64. / 1023.,
        white: 940. / 1023.,
    };
    signal.validate().unwrap();
    signal.primaries = [[0.5, 0.5]; 3];
    assert!(signal.validate().is_err());
    signal = settings.working.clone();
    signal.white_point = [f64::INFINITY, 0.5];
    assert!(signal.validate().is_err());
    settings.sources = vec![
        SourceColorAssignment {
            asset: 7,
            interpretation: SourceInterpretation::Data {
                meaning: ImageMeaning::Normal {}
            }
        };
        MAX_SOURCE_COLOR_ASSIGNMENTS + 1
    ];
    assert!(settings
        .validate(&project)
        .unwrap_err()
        .to_string()
        .contains("too many source"));
}

#[test]
fn managed_bundle_paths_reject_windows_collisions_and_file_directory_aliases() {
    for names in [
        vec!["config.ocio"],
        vec!["LOOK.cube", "look.cube"],
        vec!["look", "look/a.cube"],
        vec!["CON.cube"],
        vec!["trailing./look.cube"],
        vec!["bad?.cube"],
        vec!["config.ocio/child"],
    ] {
        let mut settings = fixture::settings();
        settings.manifest.resources.clear();
        for name in names {
            settings
                .manifest
                .resources
                .insert(name.into(), ContentIdentity::of(fixture::LUT));
        }
        fixture::rekey(&mut settings);
        assert!(settings.resources().is_err());
    }
}

#[test]
fn resource_load_keeps_native_missing_cause_and_rejects_same_size_tampering() {
    let project = fixture::project();
    let root = tempfile::tempdir().unwrap();
    fixture::write_resources(root.path(), &project);
    motion_core::storage::save(root.path(), &project).unwrap();
    std::fs::remove_file(
        root.path()
            .join(&project.color_management.resources().unwrap()[0].path),
    )
    .unwrap();
    let error = motion_core::storage::load(root.path()).unwrap_err();
    let Error::ProjectColor(ProjectColorError::Io {
        phase: "resolve",
        path,
        source,
    }) = &error
    else {
        panic!("{error:?}")
    };
    assert!(path.starts_with("assets/color/"));
    assert!(source.raw_os_error().is_some());
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
    fixture::write_resources(root.path(), &project);
    let ProjectColorManagement::Ocio { settings } = project.color_management.as_ref() else {
        unreachable!()
    };
    let config = motion_core::storage::read_color_config(root.path(), settings).unwrap();
    assert_eq!(config.key(), settings.selection.config_key);
    for resource in settings.resources().unwrap() {
        let path = root.path().join(resource.path);
        let original = std::fs::read(&path).unwrap();
        let mut modified = original.clone();
        modified[0] ^= 1;
        std::fs::write(&path, modified).unwrap();
        let error = motion_core::storage::load(root.path()).unwrap_err();
        assert!(
            matches!(
                error,
                Error::ProjectColor(ProjectColorError::Resource {
                    phase: "verify",
                    ..
                })
            ),
            "{error:?}"
        );
        std::fs::write(path, original).unwrap();
    }
    assert_eq!(motion_core::storage::load(root.path()).unwrap(), project);
    assert_eq!(config.config_bytes(), fixture::CONFIG);
}

#[test]
fn package_roundtrip_preserves_closed_config_bytes_and_rejects_late_changes() {
    let project = fixture::project();
    let root = tempfile::tempdir().unwrap();
    fixture::write_resources(root.path(), &project);
    motion_core::storage::save(root.path(), &project).unwrap();
    let snapshot = motion_core::storage::PackageSnapshot::new(root.path(), &project).unwrap();
    assert_eq!(snapshot.source_count(), 2);
    let destination = tempfile::tempdir().unwrap();
    let archive = destination.path().join("color.msproj");
    motion_core::storage::export_package(root.path(), &project, &archive).unwrap();
    let restored =
        motion_core::storage::import_package(&archive, &destination.path().join("opened")).unwrap();
    assert_eq!(restored, project);
    for resource in project.color_management.resources().unwrap() {
        assert_eq!(
            std::fs::read(destination.path().join("opened").join(&resource.path)).unwrap(),
            std::fs::read(root.path().join(resource.path)).unwrap()
        );
    }
    let resource = &project.color_management.resources().unwrap()[0];
    let file = root.path().join(&resource.path);
    let original = std::fs::read(&file).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    std::fs::write(&file, changed).unwrap();
    let prior = std::fs::read(&archive).unwrap();
    let error = snapshot
        .prepare(&archive, &mut |_, _| Ok(()))
        .err()
        .expect("tampered source must reject publication");
    assert!(error.to_string().contains("changed"), "{error}");
    assert_eq!(std::fs::read(&archive).unwrap(), prior);
    std::fs::write(file, original).unwrap();
}

#[test]
fn history_budget_counts_color_payload_through_background_edits_and_undo_redo() {
    let initial = Project::new(64, 64, 30, 120).unwrap();
    let mut engine = Engine::new(initial.clone()).unwrap();
    let mut settings = fixture::settings();
    settings
        .manifest
        .context
        .insert("EXPLICIT_SHOT".into(), "x".repeat(256));
    fixture::rekey(&mut settings);
    engine
        .apply(Command::SetColorManagement {
            settings: fixture::managed(settings),
        })
        .unwrap();
    let managed = engine.snapshot();
    engine
        .apply(Command::SetColor {
            object: 0,
            value: [0.1, 0.2, 0.3, 1.],
        })
        .unwrap();
    let expected =
        serde_json::to_vec(&initial).unwrap().len() + serde_json::to_vec(&managed).unwrap().len();
    assert_eq!(engine.history_diagnostics()["modelBytes"], expected);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), managed);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), initial);
    assert_eq!(
        engine.history_diagnostics()["modelBytes"],
        serde_json::to_vec(&managed).unwrap().len()
            + serde_json::to_vec(&{
                let mut changed = managed.clone();
                changed.background = [0.1, 0.2, 0.3, 1.];
                changed
            })
            .unwrap()
            .len()
    );
}

#[test]
fn color_configuration_is_a_structural_transaction_outside_active_gestures() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    let initial = engine.snapshot();
    engine.begin_gesture().unwrap();
    let command = Command::SetColorManagement {
        settings: fixture::managed(fixture::settings()),
    };
    assert!(command.composition_transaction());
    assert!(command.changes_resources());
    assert!(matches!(engine.apply(command), Err(Error::Gesture)));
    assert_eq!(engine.snapshot(), initial);
    engine.end_gesture(false).unwrap();
    assert!(!engine.can_undo());
}
