//! Independently authored catalogue/content fixtures. These bytes test identity
//! and persistence only; they do not claim an executable OCIO configuration.
#![allow(dead_code)]
use motion_core::{color_management::*, Project};
use motion_effects::{color_config::*, AlphaMode};
use std::{collections::BTreeMap, path::Path, sync::Arc};

pub const CONFIG: &[u8] = b"motion independent color identity fixture\n";
pub const LUT: &[u8] = b"LUT_1D_SIZE 2\n0 0 0\n1 1 1\n";
pub fn space(name: &str) -> SpaceReference {
    SpaceReference::Space { name: name.into() }
}
pub fn signal(encoding: &str, reference: ReferenceSpace) -> ColorSignal {
    ColorSignal {
        encoding: encoding.into(),
        transfer: if encoding == "scene-linear" {
            ColorTransfer::Linear
        } else {
            ColorTransfer::ConfigDefined
        },
        primaries: [[0.64, 0.33], [0.3, 0.6], [0.15, 0.06]],
        white_point: [0.3127, 0.3290],
        range: SignalRange::Full {},
        reference,
        alpha: AlphaMode::Straight,
    }
}
pub fn settings() -> OcioProject {
    let working = signal("scene-linear", ReferenceSpace::Scene);
    let display = signal("sdr-video", ReferenceSpace::Display);
    let manifest = ColorConfigManifest {
        version: CONFIG_MANIFEST_VERSION,
        ocio_version: ColorVersion {
            major: 2,
            minor: 6,
            patch: 0,
        },
        origin: ConfigOrigin::Authored {
            id: "project-contract-test".into(),
        },
        config: ContentIdentity::of(CONFIG),
        resources: BTreeMap::from([("looks/example.cube".into(), ContentIdentity::of(LUT))]),
        context: BTreeMap::new(),
        spaces: BTreeMap::from([
            (
                "working".into(),
                ConfigColorSpace {
                    encoding: working.encoding.clone(),
                    reference: working.reference,
                    is_data: false,
                },
            ),
            (
                "encoded".into(),
                ConfigColorSpace {
                    encoding: display.encoding.clone(),
                    reference: display.reference,
                    is_data: false,
                },
            ),
            (
                "raw".into(),
                ConfigColorSpace {
                    encoding: "data".into(),
                    reference: ReferenceSpace::Scene,
                    is_data: true,
                },
            ),
        ]),
        roles: BTreeMap::from([("scene_linear".into(), "working".into())]),
        displays: BTreeMap::from([("monitor".into(), vec!["normal".into(), "alternate".into()])]),
    };
    let selection = ColorSelection {
        config_key: manifest.declared_key().unwrap(),
        aces: None,
        input: space("encoded"),
        working: SpaceReference::Role {
            name: "scene_linear".into(),
        },
        display: DisplayView {
            display: "monitor".into(),
            view: "normal".into(),
        },
        output: OutputSelection::Space {
            space: space("encoded"),
        },
    };
    OcioProject {
        manifest,
        selection,
        default_input: display.clone(),
        authored_input: ColorInput {
            space: space("encoded"),
            signal: display.clone(),
        },
        working,
        display: display.clone(),
        output: display,
        sources: Vec::new(),
    }
}
pub fn managed(settings: OcioProject) -> Arc<ProjectColorManagement> {
    Arc::new(ProjectColorManagement::Ocio {
        settings: Box::new(settings),
    })
}
pub fn project() -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.color_management = managed(settings());
    project.validate().unwrap();
    project
}
pub fn rekey(settings: &mut OcioProject) {
    settings.selection.config_key = settings.manifest.declared_key().unwrap();
}
pub fn write_resources(root: &Path, project: &Project) {
    for resource in project.color_management.resources().unwrap() {
        let path = root.join(resource.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = if path.file_name().unwrap() == "config.ocio" {
            CONFIG
        } else {
            LUT
        };
        assert_eq!(ContentIdentity::of(bytes), resource.identity);
        std::fs::write(path, bytes).unwrap();
    }
}
