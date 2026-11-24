//! Check builtin provenance independently of the chosen pixel transform.
use super::*;
use motion_effects::color_config::ConfigOrigin;

#[test]
#[ignore = "requires the pinned native OCIO runtime"]
fn builtin_origin_and_aces_version_match_the_entire_native_configuration() {
    let runtime = runtime();
    let reference = reference();
    let sample = [0.2, 0.4, 1.5];
    let mut refused = 0;
    for (index, item) in reference.configs.iter().enumerate() {
        let original = config(item);
        let prepare = |config: &VerifiedColorConfig| {
            let mut selection = item.selection.clone();
            selection.config_key = config.key();
            selection.aces = match &config.manifest().origin {
                ConfigOrigin::Builtin { aces, .. } => Some(*aces),
                ConfigOrigin::Authored { .. } => None,
            };
            runtime.prepare(
                config,
                &selection,
                ColorProcessorPhase::InputToWorking,
                AlphaMode::Straight,
                AlphaMode::Straight,
            )
        };
        let expected = prepare(&original).unwrap().apply_rgb(sample).unwrap();
        let ConfigOrigin::Builtin { id, aces } = &item.manifest.origin else {
            unreachable!()
        };
        let ConfigOrigin::Builtin { id: other, .. } = &reference.configs
            [(index + 1) % reference.configs.len()]
        .manifest
        .origin
        else {
            unreachable!()
        };
        let mut cases = Vec::new();
        for declared in [
            other.as_str(),
            "cg-config-latest",
            "cg-config-v999.0.0_aces-v2.1_ocio-v2.6",
        ] {
            let mut manifest = item.manifest.clone();
            manifest.origin = ConfigOrigin::Builtin {
                id: declared.into(),
                aces: *aces,
            };
            cases.push(("builtin_id", manifest, item.config.clone()));
        }
        for changed in [
            motion_effects::color_config::ColorVersion {
                minor: aces.minor + 1,
                ..*aces
            },
            motion_effects::color_config::ColorVersion {
                patch: aces.patch + 1,
                ..*aces
            },
        ] {
            let mut manifest = item.manifest.clone();
            manifest.origin = ConfigOrigin::Builtin {
                id: id.clone(),
                aces: changed,
            };
            cases.push(("aces_version", manifest, item.config.clone()));
        }
        // Luma is not exercised by this RGB processor: comparing only its
        // output or the catalogue names would miss this configuration change.
        let changed = item.config.replace(
            "luma: [0.2126, 0.7152, 0.0722]",
            "luma: [0.2127, 0.7151, 0.0722]",
        );
        assert_ne!(changed, item.config);
        let mut manifest = item.manifest.clone();
        manifest.config = ContentIdentity::of(changed.as_bytes());
        cases.push(("builtin_id", manifest.clone(), changed.clone()));
        for (field, manifest, bytes) in cases {
            let config =
                VerifiedColorConfig::new(manifest, Arc::from(bytes.as_bytes()), BTreeMap::new())
                    .unwrap();
            let key = config.key();
            let error = prepare(&config).err().unwrap();
            assert!(
                matches!(&error, Error::Catalogue { field: actual, .. } if *actual == field),
                "{error}"
            );
            assert_eq!(config.key(), key);
            assert_eq!(config.config_bytes(), bytes.as_bytes());
            assert_eq!(
                prepare(&original).unwrap().apply_rgb(sample).unwrap(),
                expected
            );
            refused += 1;
        }
        // Independent authored configurations must not acquire an ACES claim,
        // nor be refused merely because their name resembles a builtin name.
        manifest.origin = ConfigOrigin::Authored {
            id: "independent-luma-config".into(),
        };
        let authored =
            VerifiedColorConfig::new(manifest, Arc::from(changed.as_bytes()), BTreeMap::new())
                .unwrap();
        assert_eq!(
            prepare(&authored).unwrap().apply_rgb(sample).unwrap(),
            expected
        );
        // Comments change the project's byte identity, but not OCIO's complete
        // canonical configuration or the explicit builtin version it represents.
        let commented = format!("# independently added layout comment\n{}", item.config);
        let mut manifest = item.manifest.clone();
        manifest.config = ContentIdentity::of(commented.as_bytes());
        let commented =
            VerifiedColorConfig::new(manifest, Arc::from(commented.as_bytes()), BTreeMap::new())
                .unwrap();
        assert_ne!(commented.key(), original.key());
        assert_eq!(
            prepare(&commented).unwrap().apply_rgb(sample).unwrap(),
            expected
        );
    }
    assert_eq!(refused, 18);
    println!("native builtin identity: ACES 1.3/2.0/2.1; 18 wrong-ID/version/content refusals; healthy recovery, authored configs and canonical comment controls passed");
}
