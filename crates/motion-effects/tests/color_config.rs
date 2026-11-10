use motion_effects::color_config::*;
use std::{collections::BTreeMap, error::Error as _, sync::Arc};

#[path = "color_config/image_identity.rs"]
mod image_identity;

const CONFIG: &[u8] = b"independently authored closed config bytes\n";
const LUT: &[u8] = b"LUT_1D_SIZE 2\n0 0 0\n1 1 1\n";
fn role(value: &str) -> SpaceReference {
    SpaceReference::Role { name: value.into() }
}
fn space(value: &str) -> SpaceReference {
    SpaceReference::Space { name: value.into() }
}
fn manifest() -> ColorConfigManifest {
    let color = |encoding: &str, reference| ConfigColorSpace {
        encoding: encoding.into(),
        reference,
        is_data: false,
    };
    ColorConfigManifest {
        version: CONFIG_MANIFEST_VERSION,
        ocio_version: ColorVersion {
            major: 2,
            minor: 6,
            patch: 0,
        },
        origin: ConfigOrigin::Authored {
            id: "synthetic-contract".into(),
        },
        config: ContentIdentity::of(CONFIG),
        resources: BTreeMap::from([("looks/example.cube".into(), ContentIdentity::of(LUT))]),
        context: BTreeMap::from([("SHOT".into(), "".into())]),
        spaces: BTreeMap::from([
            (
                "linear".into(),
                color("scene-linear", ReferenceSpace::Scene),
            ),
            (
                "encoded".into(),
                color("sdr-video", ReferenceSpace::Display),
            ),
            ("log".into(), color("log", ReferenceSpace::Scene)),
            (
                "data".into(),
                ConfigColorSpace {
                    encoding: "data".into(),
                    reference: ReferenceSpace::Scene,
                    is_data: true,
                },
            ),
        ]),
        roles: BTreeMap::from([
            ("scene_linear".into(), "linear".into()),
            ("input".into(), "encoded".into()),
            ("data".into(), "data".into()),
        ]),
        displays: BTreeMap::from([("monitor".into(), vec!["normal".into(), "alternate".into()])]),
    }
}
fn files() -> BTreeMap<String, Arc<[u8]>> {
    BTreeMap::from([("looks/example.cube".into(), Arc::from(LUT))])
}
fn verified(value: ColorConfigManifest) -> VerifiedColorConfig {
    VerifiedColorConfig::new(value, Arc::from(CONFIG), files()).unwrap()
}
fn selection(config: &VerifiedColorConfig) -> ColorSelection {
    ColorSelection {
        config_key: config.key(),
        aces: None,
        input: role("input"),
        working: role("scene_linear"),
        display: DisplayView {
            display: "monitor".into(),
            view: "normal".into(),
        },
        output: OutputSelection::Space {
            space: space("encoded"),
        },
    }
}

#[test]
fn immutable_content_and_explicit_resolution_roundtrip() {
    let value = manifest();
    let json = serde_json::to_vec(&value).unwrap();
    assert_eq!(ColorConfigManifest::from_json(&json).unwrap(), value);
    let config = verified(value);
    let selected = selection(&config);
    let result = config.resolve(&selected).unwrap();
    assert_eq!(result.input, "encoded");
    assert_eq!(result.working, "linear");
    assert_eq!(result.output, ResolvedColorOutput::Space("encoded".into()));
    let selected: ColorSelection =
        serde_json::from_slice(&serde_json::to_vec(&selected).unwrap()).unwrap();
    assert_eq!(config.resolve(&selected).unwrap(), result);
    assert_eq!(config.config_bytes(), CONFIG);
    assert_eq!(config.resource_bytes("looks/example.cube"), Some(LUT));
    assert!(config.resource_bytes("normal-cache-miss").is_none());
    let mut log = selected.clone();
    log.working = space("log");
    assert_eq!(config.resolve(&log).unwrap().working, "log");
    assert_ne!(result.key, config.resolve(&log).unwrap().key);
    let mut displayed = selected;
    displayed.output = OutputSelection::Display {
        display_view: DisplayView {
            display: "monitor".into(),
            view: "alternate".into(),
        },
    };
    assert_eq!(
        config.resolve(&displayed).unwrap().output,
        ResolvedColorOutput::Display(DisplayView {
            display: "monitor".into(),
            view: "alternate".into()
        })
    );
}

#[test]
fn closed_resource_set_rejects_missing_modified_and_unbounded_input() {
    let missing = VerifiedColorConfig::new(manifest(), Arc::from(CONFIG), BTreeMap::new())
        .err()
        .unwrap();
    assert!(matches!(
        missing,
        ColorConfigError::Resource {
            reason: "missing resource",
            ..
        }
    ));
    let mut changed = files();
    changed.insert("looks/example.cube".into(), Arc::from(&b"different"[..]));
    assert!(matches!(
        VerifiedColorConfig::new(manifest(), Arc::from(CONFIG), changed),
        Err(ColorConfigError::Resource {
            reason: "content identity mismatch",
            ..
        })
    ));
    let mut extra = files();
    extra.insert("extra.cube".into(), Arc::from(LUT));
    assert!(matches!(
        VerifiedColorConfig::new(manifest(), Arc::from(CONFIG), extra),
        Err(ColorConfigError::Resource {
            reason: "undeclared resource",
            ..
        })
    ));
    assert!(
        matches!(VerifiedColorConfig::new(manifest(), Arc::from(&b"different config"[..]), files()), Err(ColorConfigError::Resource { resource, .. }) if resource == "config")
    );
    assert!(matches!(
        VerifiedColorConfig::new(
            manifest(),
            Arc::from(vec![0; MAX_CONFIG_BYTES + 1]),
            files()
        ),
        Err(ColorConfigError::Capacity {
            field: "actual config bytes",
            ..
        })
    ));
    let mut huge = files();
    huge.insert(
        "looks/example.cube".into(),
        Arc::from(vec![0; MAX_RESOURCE_BYTES + 1]),
    );
    assert!(matches!(
        VerifiedColorConfig::new(manifest(), Arc::from(CONFIG), huge),
        Err(ColorConfigError::Capacity {
            field: "actual resource bytes",
            ..
        })
    ));
    // A failed replacement never touches the existing verified catalogue.
    let config = verified(manifest());
    let previous = config.resolve(&selection(&config)).unwrap();
    assert!(VerifiedColorConfig::new(manifest(), Arc::from(CONFIG), BTreeMap::new()).is_err());
    assert_eq!(config.resolve(&selection(&config)).unwrap(), previous);
}

#[test]
fn names_roles_views_and_version_fail_without_fallback_or_mutation() {
    let config = verified(manifest());
    let good = selection(&config);
    let expected = config.resolve(&good).unwrap();
    for (field, value) in [("role", role("missing")), ("space", space("missing"))] {
        let mut selected = good.clone();
        selected.input = value;
        assert!(
            matches!(config.resolve(&selected), Err(ColorConfigError::Missing { kind, name }) if kind == field && name == "missing")
        );
    }
    for selected in [
        ColorSelection {
            config_key: [0; 32],
            ..good.clone()
        },
        ColorSelection {
            aces: Some(ColorVersion {
                major: 2,
                minor: 0,
                patch: 0,
            }),
            ..good.clone()
        },
        ColorSelection {
            input: role("data"),
            ..good.clone()
        },
        ColorSelection {
            working: space("data"),
            ..good.clone()
        },
        ColorSelection {
            display: DisplayView {
                display: "missing".into(),
                view: "normal".into(),
            },
            ..good.clone()
        },
        ColorSelection {
            display: DisplayView {
                display: "monitor".into(),
                view: "missing".into(),
            },
            ..good.clone()
        },
        ColorSelection {
            output: OutputSelection::Space {
                space: space("data"),
            },
            ..good.clone()
        },
        ColorSelection {
            output: OutputSelection::Display {
                display_view: DisplayView {
                    display: "monitor".into(),
                    view: "missing".into(),
                },
            },
            ..good.clone()
        },
    ] {
        assert!(config.resolve(&selected).is_err());
        assert_eq!(config.resolve(&good).unwrap(), expected);
    }
    let mut selected = good;
    selected.input = role(&"x".repeat(10000));
    let error = config.resolve(&selected).unwrap_err().to_string();
    assert!(error.len() < 256 && !error.contains(&"x".repeat(256)));
}

#[test]
fn hashes_cover_runtime_config_resources_context_catalogue_and_selection() {
    let base = manifest();
    let config = verified(base.clone());
    let expected = config.key();
    let mut variants = Vec::new();
    let mut item = base.clone();
    item.ocio_version.patch += 1;
    variants.push(item);
    let mut item = base.clone();
    item.origin = ConfigOrigin::Authored {
        id: "another".into(),
    };
    variants.push(item);
    let mut item = base.clone();
    item.context.insert("SHOT".into(), "different".into());
    variants.push(item);
    let mut item = base.clone();
    item.roles.insert("scene_linear".into(), "log".into());
    variants.push(item);
    let mut item = base.clone();
    item.spaces.get_mut("linear").unwrap().reference = ReferenceSpace::Display;
    variants.push(item);
    let mut item = base.clone();
    item.spaces.get_mut("linear").unwrap().encoding = "unknown".into();
    variants.push(item);
    let mut item = base.clone();
    item.displays
        .get_mut("monitor")
        .unwrap()
        .push("new view".into());
    variants.push(item);
    for item in variants {
        assert_ne!(verified(item).key(), expected);
    }
    let mut new_config = base.clone();
    new_config.config = ContentIdentity::of(b"changed");
    assert_ne!(
        VerifiedColorConfig::new(new_config, Arc::from(&b"changed"[..]), files())
            .unwrap()
            .key(),
        expected
    );
    let mut new_file = base;
    new_file.resources.insert(
        "looks/example.cube".into(),
        ContentIdentity::of(b"changed LUT"),
    );
    assert_ne!(
        VerifiedColorConfig::new(
            new_file,
            Arc::from(CONFIG),
            BTreeMap::from([("looks/example.cube".into(), Arc::from(&b"changed LUT"[..]))])
        )
        .unwrap()
        .key(),
        expected
    );
    let selected = selection(&config);
    let resolved = config.resolve(&selected).unwrap();
    let mut new_selected = selected.clone();
    new_selected.display.view = "alternate".into();
    assert_ne!(config.resolve(&new_selected).unwrap().key, resolved.key);
    let mut new_selected = selected;
    new_selected.input = space("encoded");
    assert_ne!(config.resolve(&new_selected).unwrap().key, resolved.key);
}

#[test]
fn aces_families_are_explicit_and_never_rebound() {
    let mut keys = Vec::new();
    for (major, minor) in [(1, 3), (2, 0), (2, 1)] {
        let version = ColorVersion {
            major,
            minor,
            patch: 0,
        };
        let mut item = manifest();
        item.origin = ConfigOrigin::Builtin {
            id: format!("synthetic-{major}-{minor}"),
            aces: version,
        };
        let config = verified(item);
        let mut selected = selection(&config);
        assert!(matches!(
            config.resolve(&selected),
            Err(ColorConfigError::AcesMismatch)
        ));
        selected.aces = Some(version);
        assert_eq!(config.resolve(&selected).unwrap().working, "linear");
        for (old_key, old_version) in &keys {
            assert_ne!(*old_key, config.key());
            selected.aces = Some(*old_version);
            assert!(matches!(
                config.resolve(&selected),
                Err(ColorConfigError::AcesMismatch)
            ));
            selected.aces = Some(version);
            selected.config_key = *old_key;
            assert!(matches!(
                config.resolve(&selected),
                Err(ColorConfigError::ConfigMismatch)
            ));
            selected.config_key = config.key();
        }
        keys.push((config.key(), version));
    }
}

#[test]
fn malformed_manifests_keep_original_json_causes_and_reject_ambiguous_maps() {
    let error = ColorConfigManifest::from_json(b"{\ninvalid").unwrap_err();
    let original = error
        .source()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .unwrap();
    assert_eq!(original.line(), 2);
    let long_key = "private-unknown-field".repeat(8192);
    let mut long = serde_json::to_value(manifest()).unwrap();
    long[&long_key] = true.into();
    let bounded_error =
        ColorConfigManifest::from_json(&serde_json::to_vec(&long).unwrap()).unwrap_err();
    assert!(bounded_error.to_string().len() < 128);
    assert!(!bounded_error.to_string().contains("private-unknown-field"));
    assert!(bounded_error.source().unwrap().is::<serde_json::Error>());
    let mut value = serde_json::to_value(manifest()).unwrap();
    value["unknown"] = true.into();
    assert!(matches!(
        ColorConfigManifest::from_json(&serde_json::to_vec(&value).unwrap()),
        Err(ColorConfigError::Json(_))
    ));
    let text = String::from_utf8(serde_json::to_vec(&manifest()).unwrap()).unwrap();
    let duplicate = text.replace(
        "\"scene_linear\":\"linear\"",
        "\"scene_linear\":\"linear\",\"scene_linear\":\"log\"",
    );
    assert_ne!(text, duplicate);
    assert!(matches!(
        ColorConfigManifest::from_json(duplicate.as_bytes()),
        Err(ColorConfigError::Json(_))
    ));
    assert!(matches!(
        ColorConfigManifest::from_json(&vec![b' '; MAX_MANIFEST_BYTES + 1]),
        Err(ColorConfigError::Capacity {
            field: "manifest bytes",
            ..
        })
    ));
    let mut bad = manifest();
    bad.version += 1;
    assert!(bad.validate().is_err());
    let mut bad = manifest();
    bad.ocio_version.major = 1;
    assert!(bad.validate().is_err());
    let mut bad = manifest();
    bad.roles.insert("missing".into(), "absent".into());
    assert!(bad.validate().is_err());
    let mut bad = manifest();
    bad.displays
        .get_mut("monitor")
        .unwrap()
        .push("normal".into());
    assert!(bad.validate().is_err());
    for path in [
        "/absolute",
        "../escape",
        "foo//bar",
        "a/./b",
        "C:/file",
        "a\\b",
    ] {
        let mut bad = manifest();
        bad.resources = BTreeMap::from([(path.into(), ContentIdentity::of(LUT))]);
        assert!(bad.validate().is_err(), "{path}");
    }
    let mut bad = manifest();
    bad.config.sha256 = "A".repeat(64);
    assert!(bad.validate().is_err());
    let mut bad = manifest();
    bad.config.bytes = usize::MAX;
    assert!(bad.validate().is_err());
    let mut bad = manifest();
    bad.spaces.clear();
    assert!(bad.validate().is_err());

    let mut actual = files();
    for index in 0..MAX_CONFIG_RESOURCES {
        actual.insert(format!("extra-{index}"), Arc::from(LUT));
    }
    assert!(matches!(
        VerifiedColorConfig::new(manifest(), Arc::from(CONFIG), actual),
        Err(ColorConfigError::Capacity {
            field: "actual resources",
            ..
        })
    ));
    let shared: Arc<[u8]> = Arc::from(vec![0; MAX_RESOURCE_BYTES]);
    let mut total = manifest();
    total.resources.clear();
    let mut resources = BTreeMap::new();
    for index in 0..4 {
        let path = format!("table-{index}");
        total
            .resources
            .insert(path.clone(), ContentIdentity::of(b"x"));
        resources.insert(path, shared.clone());
    }
    assert!(matches!(
        VerifiedColorConfig::new(total, Arc::from(CONFIG), resources),
        Err(ColorConfigError::Capacity {
            field: "actual bundle bytes",
            ..
        })
    ));
}
