use motion_effects::{native_layout::*, ParamDefinition};

fn parameter(id: &str, kind: &str) -> ParamDefinition {
    serde_json::from_value(serde_json::json!({"id":id,"name":id,"kind":kind,
        "default":[0,0,0,0],"min":0,"max":100,"step":1}))
    .unwrap()
}
fn field(id: &str, parameter: &str, control: ParameterControl) -> LayoutNode {
    LayoutNode {
        id: id.into(),
        width: LayoutExtent::Auto,
        height: LayoutExtent::Auto,
        content: LayoutContent::Parameter {
            parameter: parameter.into(),
            control,
        },
    }
}
fn layout(children: Vec<LayoutNode>) -> NativeLayout {
    NativeLayout {
        format_version: LAYOUT_VERSION,
        root: LayoutNode {
            id: "root".into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Column {
                gap: LayoutSpacing::Compact,
                align: LayoutAlign::Stretch,
                children,
            },
        },
    }
}

#[test]
fn draft_roundtrip_retains_dangling_bindings_but_preview_identifies_the_component() {
    let source = layout(vec![field(
        "position-field",
        "position",
        ParameterControl::Vector,
    )]);
    source.validate_structure().unwrap();
    let decoded: NativeLayout =
        serde_json::from_str(&serde_json::to_string(&source).unwrap()).unwrap();
    assert_eq!(source, decoded);
    let error = decoded.validate_bindings(&[]).unwrap_err().to_string();
    assert!(
        error.contains("node=position-field field=parameter") && error.contains("missing"),
        "{error}"
    );
    decoded
        .validate_bindings(&[parameter("position", "vec2")])
        .unwrap();
    let error = decoded
        .validate_bindings(&[parameter("position", "float")])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("node=position-field field=control"),
        "{error}"
    );
}

#[test]
fn controls_follow_actual_parameter_types_and_multiple_views_share_one_binding() {
    for (kind, control) in [
        ("float", ParameterControl::Number),
        ("vec2", ParameterControl::Vector),
        ("vec3", ParameterControl::Vector),
        ("color", ParameterControl::Color),
        ("bool", ParameterControl::Checkbox),
        ("enum", ParameterControl::Dropdown),
    ] {
        let panel = layout(vec![
            field("first", "value", control),
            field("second", "value", ParameterControl::Auto),
        ]);
        panel
            .validate_bindings(&[parameter("value", kind)])
            .unwrap();
        let mut unavailable = parameter("value", kind);
        unavailable.implemented = false;
        assert!(panel
            .validate_bindings(&[unavailable])
            .unwrap_err()
            .to_string()
            .contains("unavailable"));
    }
    assert!(
        layout(vec![field("curve", "value", ParameterControl::Auto)])
            .validate_bindings(&[parameter("value", "curve")])
            .is_err()
    );
}

#[test]
fn nested_tabs_splits_and_size_constraints_roundtrip_without_theme_or_value_state() {
    let mut panel = layout(vec![]);
    panel.root.width = LayoutExtent::Weight { value: 2. };
    panel.root.content = LayoutContent::Tabs {
        tabs: vec![LayoutTab {
            id: "tab-one".into(),
            title: "发射器".into(),
            child: LayoutNode {
                id: "split".into(),
                width: LayoutExtent::Auto,
                height: LayoutExtent::Points { value: 400. },
                content: LayoutContent::Split {
                    axis: LayoutAxis::Horizontal,
                    ratio: 0.3,
                    first: Box::new(field("left", "position", ParameterControl::Auto)),
                    second: Box::new(field("right", "position", ParameterControl::Auto)),
                },
            },
        }],
    };
    panel
        .validate_bindings(&[parameter("position", "vec2")])
        .unwrap();
    let text = serde_json::to_string(&panel).unwrap();
    assert_eq!(serde_json::from_str::<NativeLayout>(&text).unwrap(), panel);
    for unexpected in ["color", "command", "native_code", "value", "min", "max"] {
        let mut value = serde_json::to_value(&panel).unwrap();
        value["root"][unexpected] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<NativeLayout>(value).is_err(),
            "{unexpected}"
        );
    }
}

#[test]
fn layout_limits_reject_duplicate_ids_singletons_depth_counts_and_nonfinite_sizes() {
    assert!(layout(vec![
        field("same", "a", ParameterControl::Auto),
        field("same", "b", ParameterControl::Auto)
    ])
    .validate_structure()
    .is_err());
    let mut preview = field("preview-one", "a", ParameterControl::Auto);
    preview.content = LayoutContent::Preview;
    let mut second = preview.clone();
    second.id = "preview-two".into();
    assert!(layout(vec![preview, second]).validate_structure().is_err());
    let too_many = layout(
        (0..MAX_LAYOUT_NODES)
            .map(|i| field(&format!("field-{i}"), "a", ParameterControl::Auto))
            .collect(),
    );
    assert!(too_many
        .validate_structure()
        .unwrap_err()
        .to_string()
        .contains("256"));
    let mut deep = field("leaf", "a", ParameterControl::Auto);
    for i in 0..MAX_LAYOUT_DEPTH {
        deep = LayoutNode {
            id: format!("group-{i}"),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Group {
                title: "Group".into(),
                collapsible: true,
                children: vec![deep],
            },
        };
    }
    assert!(NativeLayout {
        format_version: LAYOUT_VERSION,
        root: deep
    }
    .validate_structure()
    .unwrap_err()
    .to_string()
    .contains("depth 16"));
    for value in [f32::NAN, f32::INFINITY, -1., 0., 8193.] {
        let mut panel = layout(vec![]);
        panel.root.width = LayoutExtent::Points { value };
        assert!(panel.validate_structure().is_err());
    }
    for version in [0, LAYOUT_VERSION + 1] {
        let mut panel = layout(vec![]);
        panel.format_version = version;
        assert!(panel.validate_structure().is_err());
    }
}
