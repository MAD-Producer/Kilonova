use super::*;

#[test]
fn cube_shape_capacity_and_dimension_are_validated_before_shader_compilation() {
    let (config, selection) = config();
    let mut original = source(&config, &selection);
    let mut cube = table();
    cube.width = 2;
    cube.height = 2;
    cube.depth = 2;
    cube.values = (0..24).map(|i| i as f32 / 8. - 0.5).collect();
    original.tables = vec![cube];
    original.shader = "layout(set=0, binding = 1) uniform sampler3D motion_ocio_tableSampler;\nvec4 motion_ocio_transform(vec4 c) { return vec4(texture(motion_ocio_tableSampler,c.rgb).rgb,c.a); }".into();
    let prepared = original.compile(&config, &selection).unwrap();
    assert_eq!(prepared.tables()[0].len(), 8);
    assert!(!prepared.wgsl().contains("textureSample"));
    assert!(prepared.wgsl().contains("motion_sample_0"));
    for change in [0, 1, 2, 3, 4] {
        let mut invalid = original.clone();
        match change {
            0 => invalid.tables[0].depth = 0,
            1 => invalid.tables[0].height = 3,
            2 => invalid.tables[0].channels = 1,
            3 => {
                invalid.tables[0].values.pop();
            }
            _ => invalid.tables[0].values[0] = f32::NAN,
        }
        assert!(matches!(
            invalid.compile(&config, &selection),
            Err(ColorProcessorError::Invalid { .. })
        ));
    }
    for edge in [65, u32::MAX] {
        let mut excessive = original.clone();
        excessive.tables[0].width = edge;
        excessive.tables[0].height = edge;
        excessive.tables[0].depth = edge;
        assert!(matches!(
            excessive.compile(&config, &selection),
            Err(ColorProcessorError::Capacity {
                field: "table texels",
                limit: MAX_PROCESSOR_TABLE_TEXELS,
                ..
            })
        ));
    }
    let mut mismatched = original.clone();
    mismatched.shader = mismatched.shader.replace("sampler3D", "sampler2D");
    assert!(matches!(
        mismatched.compile(&config, &selection),
        Err(ColorProcessorError::Invalid {
            field: "table declaration",
            ..
        })
    ));
    assert_eq!(
        original.compile(&config, &selection).unwrap().key(),
        prepared.key()
    );
}

#[test]
fn ordinary_table_depth_has_plane_semantics_and_unchanged_json() {
    let table = table();
    let encoded = serde_json::to_value(&table).unwrap();
    assert!(encoded.get("depth").is_none());
    let decoded: ColorProcessorTable = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(decoded.depth, 1);
    assert_eq!(serde_json::to_value(&decoded).unwrap(), encoded);
}
