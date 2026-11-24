//! Fresh files/oracles produced by the native qualification executable.
use motion_effects::color_config::*;
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Clone, Deserialize)]
pub struct Case {
    pub config: String,
    pub name: String,
    pub cube: String,
    pub input: Vec<[f32; 4]>,
    pub expected: Vec<[f32; 4]>,
    pub composite: Vec<[f32; 4]>,
}
#[derive(Deserialize)]
struct Fixture {
    version: u32,
    runtime: String,
    cases: Vec<Case>,
}
pub fn cases() -> Vec<Case> {
    let directory = PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_FAILURE_LIBRARY_DIR")
            .expect("explicit fresh native resource qualification directory"),
    );
    let fixture: Fixture =
        serde_json::from_slice(&std::fs::read(directory.join("native-resources.json")).unwrap())
            .unwrap();
    assert_eq!(fixture.version, 1);
    assert_eq!(fixture.runtime, "2.6.0");
    assert_eq!(fixture.cases.len(), 5);
    for case in &fixture.cases {
        assert_eq!(case.input.len(), 12);
        assert_eq!(case.expected.len(), 12);
        assert_eq!(case.composite.len(), 2);
        assert!(case
            .input
            .iter()
            .chain(&case.expected)
            .chain(&case.composite)
            .flatten()
            .all(|value| value.is_finite()));
    }
    assert!(
        !directory.join("resource-oracle-files").exists(),
        "native oracle files must already be removed"
    );
    fixture.cases
}
pub fn config(case: &Case) -> VerifiedColorConfig {
    let manifest = ColorConfigManifest {
        version: CONFIG_MANIFEST_VERSION,
        ocio_version: ColorVersion {
            major: 2,
            minor: 6,
            patch: 0,
        },
        origin: ConfigOrigin::Authored {
            id: "native-resource-qualification".into(),
        },
        config: ContentIdentity::of(case.config.as_bytes()),
        resources: BTreeMap::from([(case.name.clone(), ContentIdentity::of(case.cube.as_bytes()))]),
        context: BTreeMap::new(),
        spaces: ["Input", "Working", "Output"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    ConfigColorSpace {
                        encoding: "scene-linear".into(),
                        reference: ReferenceSpace::Scene,
                        is_data: false,
                    },
                )
            })
            .collect(),
        roles: BTreeMap::from([
            ("default".into(), "Working".into()),
            ("scene_linear".into(), "Working".into()),
            ("color_picking".into(), "Input".into()),
        ]),
        displays: BTreeMap::from([("Monitor".into(), vec!["View".into()])]),
    };
    VerifiedColorConfig::new(
        manifest,
        Arc::from(case.config.as_bytes()),
        BTreeMap::from([(case.name.clone(), Arc::from(case.cube.as_bytes()))]),
    )
    .unwrap()
}
pub fn selection(config: &VerifiedColorConfig) -> ColorSelection {
    ColorSelection {
        config_key: config.key(),
        aces: None,
        input: SpaceReference::Space {
            name: "Input".into(),
        },
        working: SpaceReference::Role {
            name: "scene_linear".into(),
        },
        display: DisplayView {
            display: "Monitor".into(),
            view: "View".into(),
        },
        output: OutputSelection::Space {
            space: SpaceReference::Space {
                name: "Output".into(),
            },
        },
    }
}
