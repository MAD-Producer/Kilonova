use motion_effects::{color_config::*, color_processor::*, AlphaMode};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error as _,
    sync::Arc,
};

fn config() -> (VerifiedColorConfig, ColorSelection) {
    let bytes = Arc::<[u8]>::from(b"synthetic shader-interface contract".as_slice());
    let manifest = ColorConfigManifest {
        version: 1,
        ocio_version: ColorVersion {
            major: 2,
            minor: 6,
            patch: 0,
        },
        origin: ConfigOrigin::Authored {
            id: "independently-authored-shader-contract".into(),
        },
        config: ContentIdentity::of(&bytes),
        resources: BTreeMap::new(),
        context: BTreeMap::new(),
        spaces: BTreeMap::from([(
            "linear".into(),
            ConfigColorSpace {
                encoding: "scene-linear".into(),
                reference: ReferenceSpace::Scene,
                is_data: false,
            },
        )]),
        roles: BTreeMap::from([("scene_linear".into(), "linear".into())]),
        displays: BTreeMap::from([("display".into(), vec!["view".into()])]),
    };
    let config = VerifiedColorConfig::new(manifest, bytes, BTreeMap::new()).unwrap();
    let selection = ColorSelection {
        config_key: config.key(),
        aces: None,
        input: SpaceReference::Role {
            name: "scene_linear".into(),
        },
        working: SpaceReference::Role {
            name: "scene_linear".into(),
        },
        display: DisplayView {
            display: "display".into(),
            view: "view".into(),
        },
        output: OutputSelection::Space {
            space: SpaceReference::Space {
                name: "linear".into(),
            },
        },
    };
    (config, selection)
}
fn source(config: &VerifiedColorConfig, selection: &ColorSelection) -> ColorProcessorSource {
    ColorProcessorSource {version:1,ocio_version:config.manifest().ocio_version,
        config_key:config.key(),selection_key:config.resolve(selection).unwrap().key,
        phase:ColorProcessorPhase::WorkingToDisplay,input_alpha:AlphaMode::Straight,output_alpha:AlphaMode::Straight,
        shader:"vec4 motion_ocio_transform(vec4 inPixel) { vec4 outColor=inPixel; outColor.rgb.r=outColor.rgb.r*2.0; return outColor; }".into(),tables:vec![]}
}
fn table() -> ColorProcessorTable {
    ColorProcessorTable {
        texture_name: "motion_ocio_table".into(),
        sampler_name: "motion_ocio_tableSampler".into(),
        binding: 1,
        width: 2,
        height: 1,
        channels: 3,
        interpolation: TableInterpolation::Linear,
        values: vec![-1., 2., 4., 3., -2., 8.],
    }
}
fn with_table(source: &mut ColorProcessorSource) {
    source.shader="layout(set=0, binding = 1) uniform sampler2D motion_ocio_tableSampler;\nvec4 motion_ocio_transform(vec4 c) { return vec4(texture(motion_ocio_tableSampler, c.rg).rgb,c.a); }".into();
    source.tables = vec![table()];
}

#[test]
fn fixed_interface_compiles_and_roundtrips_without_gpu_or_native_ocio() {
    let (config, selection) = config();
    let source = source(&config, &selection);
    let prepared = source.compile(&config, &selection).unwrap();
    assert_eq!(prepared.phase(), ColorProcessorPhase::WorkingToDisplay);
    assert!(prepared.tables().is_empty());
    assert!(prepared.wgsl().contains("@compute"));
    assert!(!prepared.wgsl().contains("textureSample"));
    let loaded = ColorProcessorSource::from_json(&serde_json::to_vec(&source).unwrap()).unwrap();
    assert_eq!(
        loaded.compile(&config, &selection).unwrap().key(),
        prepared.key()
    );
    let module = naga::front::wgsl::parse_str(prepared.wgsl()).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn tables_preserve_hdr_and_use_explicit_buffer_interpolation() {
    let (config, selection) = config();
    let mut source = source(&config, &selection);
    with_table(&mut source);
    let prepared = source.compile(&config, &selection).unwrap();
    assert_eq!(
        prepared.tables(),
        &[vec![[-1., 2., 4., 1.], [3., -2., 8., 1.]]]
    );
    assert!(!prepared.wgsl().contains("textureSample"));
    assert!(prepared.wgsl().contains("motion_sample_0"));
    let key = prepared.key();
    source.tables[0].interpolation = TableInterpolation::Nearest;
    assert_ne!(source.compile(&config, &selection).unwrap().key(), key);
    source.tables[0].channels = 1;
    source.tables[0].values = vec![-2., 4.];
    assert_eq!(
        source.compile(&config, &selection).unwrap().tables(),
        &[vec![[-2., 0., 0., 1.], [4., 0., 0., 1.]]]
    );
}

#[test]
fn identity_binds_policy_alpha_shader_and_exact_table_samples() {
    let (config, selection) = config();
    let mut original = source(&config, &selection);
    with_table(&mut original);
    let key = original.compile(&config, &selection).unwrap().key();
    let mut changed = original.clone();
    changed.input_alpha = AlphaMode::Premultiplied;
    assert_ne!(changed.compile(&config, &selection).unwrap().key(), key);
    let mut changed = original.clone();
    changed.output_alpha = AlphaMode::Premultiplied;
    assert_ne!(changed.compile(&config, &selection).unwrap().key(), key);
    let mut changed = original.clone();
    changed.phase = ColorProcessorPhase::InputToWorking;
    assert_ne!(changed.compile(&config, &selection).unwrap().key(), key);
    let mut changed = original.clone();
    changed.shader.push_str("\n// authored revision\n");
    assert_ne!(changed.compile(&config, &selection).unwrap().key(), key);
    let mut changed = original.clone();
    changed.tables[0].values[0] = -0.;
    let negative_zero = changed.compile(&config, &selection).unwrap().key();
    changed.tables[0].values[0] = 0.;
    assert_ne!(
        changed.compile(&config, &selection).unwrap().key(),
        negative_zero
    );
    for field in [0, 1] {
        let mut changed = original.clone();
        if field == 0 {
            changed.config_key[0] ^= 1;
        } else {
            changed.selection_key[0] ^= 1;
        }
        assert!(matches!(
            changed.compile(&config, &selection),
            Err(ColorProcessorError::Invalid {
                field: "identity",
                ..
            })
        ));
    }
    assert_eq!(original.compile(&config, &selection).unwrap().key(), key);
}

#[test]
fn malformed_programs_preserve_native_causes_and_reject_foreign_resources() {
    let (config, selection) = config();
    let mut value = source(&config, &selection);
    value.shader = "this is not a GLSL program".into();
    let error = value.compile(&config, &selection).err().unwrap();
    assert!(matches!(error, ColorProcessorError::Parse(_)));
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<naga::front::glsl::ParseErrors>()
        .is_some());
    assert_eq!(error.to_string(), "color processor phase=glsl-parse");
    let mut value = source(&config, &selection);
    value.shader = "vec4 motion_ocio_transform(vec4 c) { return vec4(dFdx(c.r)); }".into();
    let error = value.compile(&config, &selection).err().unwrap();
    assert!(matches!(error, ColorProcessorError::Validation(_)));
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<naga::WithSpan<naga::valid::ValidationError>>()
        .is_some());
    let mut value = source(&config, &selection);
    value.shader=format!("layout(set=3,binding=0,std430) readonly buffer Foreign {{ vec4 p[]; }} foreign_values;\n{}",value.shader);
    assert!(matches!(
        value.compile(&config, &selection),
        Err(ColorProcessorError::Invalid {
            field: "shader binding",
            ..
        })
    ));
    let mut value = source(&config, &selection);
    with_table(&mut value);
    value.shader = value.shader.replace("binding = 1", "binding = 9");
    assert!(matches!(
        value.compile(&config, &selection),
        Err(ColorProcessorError::Invalid {
            field: "table declaration",
            ..
        })
    ));
}

#[test]
fn capacities_and_invalid_tables_fail_before_compilation() {
    let (config, selection) = config();
    let mut original = source(&config, &selection);
    with_table(&mut original);
    for kind in 0..9 {
        let mut value = original.clone();
        match kind {
            0 => value.version = 2,
            1 => value.ocio_version.patch = 1,
            2 => value.shader = " ".repeat(MAX_PROCESSOR_SHADER_BYTES + 1),
            3 => value.tables[0].width = 0,
            4 => value.tables[0].width = u32::MAX,
            5 => value.tables[0].channels = 4,
            6 => value.tables[0].values.pop().map(|_| ()).unwrap(),
            7 => value.tables[0].values[0] = f32::NAN,
            _ => value.tables[0].sampler_name = "injected; source".into(),
        }
        assert!(
            matches!(
                value.compile(&config, &selection),
                Err(ColorProcessorError::Invalid { .. } | ColorProcessorError::Capacity { .. })
            ),
            "case={kind}"
        );
    }
    let mut value = original.clone();
    value.tables = vec![table(); MAX_PROCESSOR_TABLES + 1];
    assert!(matches!(
        value.compile(&config, &selection),
        Err(ColorProcessorError::Capacity {
            field: "tables",
            ..
        })
    ));
    let mut value = original.clone();
    value.tables.push(table());
    assert!(matches!(
        value.compile(&config, &selection),
        Err(ColorProcessorError::Invalid {
            field: "table name",
            ..
        })
    ));
    assert!(matches!(
        ColorProcessorSource::from_json(&vec![b' '; MAX_PROCESSOR_JSON_BYTES + 1]),
        Err(ColorProcessorError::Capacity {
            field: "json bytes",
            ..
        })
    ));
    let error = ColorProcessorSource::from_json(br#"{"version":1,"unknown":true}"#)
        .err()
        .unwrap();
    assert!(matches!(error, ColorProcessorError::Json(_)));
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .is_some());
}

#[test]
fn native_ocio_programs_match_catalogue_and_selection_identity_for_all_phases() {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("color_processor/reference.json")).unwrap();
    assert_eq!(reference["runtime"], "2.6.0");
    assert_eq!(reference["configs"].as_array().unwrap().len(), 3);
    let configs: Vec<_> = reference["configs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let manifest =
                ColorConfigManifest::from_json(&serde_json::to_vec(&value["manifest"]).unwrap())
                    .unwrap();
            let bytes = Arc::from(value["config"].as_str().unwrap().as_bytes());
            let config = VerifiedColorConfig::new(manifest, bytes, BTreeMap::new()).unwrap();
            let selection: ColorSelection =
                serde_json::from_value(value["selection"].clone()).unwrap();
            (config, selection)
        })
        .collect();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    let mut keys = BTreeSet::new();
    for case in cases {
        let (config, selection) = &configs[case["config"].as_u64().unwrap() as usize];
        let source =
            ColorProcessorSource::from_json(&serde_json::to_vec(&case["processor"]).unwrap())
                .unwrap();
        let compiled = source.compile(config, selection).unwrap();
        assert!(keys.insert(compiled.key()));
        let parsed = naga::front::wgsl::parse_str(compiled.wgsl()).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&parsed)
        .unwrap();
        assert!(!compiled.wgsl().contains("textureSample"));
        assert!(compiled.wgsl().contains("arrayLength"));
    }
}
