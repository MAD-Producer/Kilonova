use motion_effects::{native_layout::*, AlphaMode, ParamDefinition, WorkingSpace};
use motion_graph::plugin_project::*;
use std::error::Error as _;

fn identity() -> PackageIdentity {
    PackageIdentity {
        id: "com.example.plugin".into(),
        version: "0.1.0".into(),
        author: "Motion Studio tests".into(),
        license: "MIT".into(),
    }
}
fn parameter(id: &str, kind: &str) -> ParamDefinition {
    serde_json::from_value(serde_json::json!({"id":id,"name":id,"kind":kind,
        "default":[0.25,0.5,0.75,1],"min":0,"max":1,"step":0.01}))
    .unwrap()
}
fn wgsl() -> PluginProject {
    PluginProject::from_wgsl(WgslDocument { id: "test_plugin".into(), name: "测试插件".into(),
        english_name: "Test Plugin".into(), source: "fn plugin_fx(p: vec2<f32>) -> vec4<f32> { return sample_source(p) * param_amount(); }".into(),
        entry: "plugin_fx".into(), parameters: vec![parameter("amount", "float")],
        working_space: WorkingSpace::Srgb, alpha_mode: AlphaMode::Straight }, identity()).unwrap()
}
fn children(project: &mut PluginProject) -> &mut Vec<LayoutNode> {
    match &mut project.interface.root.content {
        LayoutContent::Column { children, .. } => children,
        _ => unreachable!(),
    }
}

#[test]
fn graph_and_wgsl_sources_save_reopen_and_overwrite_as_one_authoring_document() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("nested/example.msplugin");
    for project in [
        PluginProject::from_graph(motion_graph::demo_graph(), identity()).unwrap(),
        wgsl(),
    ] {
        project.save(&path).unwrap();
        assert_eq!(PluginProject::load(&path).unwrap(), project);
        assert_eq!(
            PluginProject::from_json(&project.to_json().unwrap()).unwrap(),
            project
        );
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1,
            "only the published authoring file survives staging"
        );
        project.compile_logic().unwrap();
    }
}

#[test]
fn incomplete_graph_and_wgsl_drafts_remain_saveable_and_fail_only_at_compile() {
    let mut graph = motion_graph::demo_graph();
    graph.connections.clear();
    let graph = PluginProject::from_graph(graph, identity()).unwrap();
    PluginProject::from_json(&graph.to_json().unwrap()).unwrap();
    let error = graph.compile_logic().err().unwrap();
    assert_eq!(error.phase(), Phase::Compile);
    assert!(error.source().is_some());
    let mut project = wgsl();
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document.source = "fn plugin_fx( PRIVATE_USER_SOURCE".into();
    PluginProject::from_json(&project.to_json().unwrap()).unwrap();
    let error = project.compile_logic().err().unwrap();
    assert_eq!(error.phase(), Phase::Compile);
    assert!(
        !error.to_string().contains("PRIVATE_USER_SOURCE"),
        "logs must not include authored source"
    );
    assert!(
        !format!("{error:?}").contains("PRIVATE_USER_SOURCE"),
        "CLI termination uses Debug"
    );
    assert!(error.source().unwrap().to_string().contains("WGSL"));
}

#[test]
fn missing_and_incompatible_bindings_are_persisted_and_keep_native_diagnostics() {
    let mut project = wgsl();
    let LayoutContent::Parameter { parameter, .. } = &mut children(&mut project)[0].content else {
        unreachable!()
    };
    *parameter = "removed_parameter".into();
    assert_eq!(
        PluginProject::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
    let error = project.compile_logic().err().unwrap();
    assert_eq!(error.phase(), Phase::Bindings);
    assert!(error
        .source()
        .unwrap()
        .to_string()
        .contains("node=field-0 field=parameter"));
    children(&mut project)[0].content = LayoutContent::Parameter {
        parameter: "amount".into(),
        control: ParameterControl::Color,
    };
    let error = project.validate_bindings().unwrap_err();
    assert!(error
        .source()
        .unwrap()
        .to_string()
        .contains("node=field-0 field=control"));
}

#[test]
fn graph_parameter_metadata_is_identical_to_the_existing_compiler_and_layout_edits_do_not_change_code(
) {
    let graph = motion_graph::demo_graph();
    let direct = motion_graph::compile(
        &graph,
        &motion_graph::CompileOptions {
            plugin_id: identity().id,
            author: identity().author,
            ..Default::default()
        },
    )
    .unwrap();
    let mut project = PluginProject::from_graph(graph, identity()).unwrap();
    let compiled = project.compile_logic().unwrap();
    assert_eq!(compiled.manifest(), &direct.manifest);
    assert_eq!(
        project.parameters().unwrap(),
        direct.manifest.effects[0].params
    );
    let text = compiled.source().to_owned();
    children(&mut project).reverse();
    children(&mut project)[0].width = LayoutExtent::Points { value: 320. };
    assert_eq!(project.compile_logic().unwrap().source(), text);
}

#[test]
fn wgsl_named_accessors_keep_id_bindings_across_reorder_without_identifier_collisions() {
    let mut project = wgsl();
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document
        .parameters
        .push(parameter("amount-offset", "float"));
    document
        .parameters
        .push(parameter("amount_offset", "float"));
    let first = project.compile_logic().unwrap();
    let CompiledLogic::Wgsl { accessors, .. } = first else {
        unreachable!()
    };
    assert_eq!(accessors[0].function, "param_amount");
    assert_ne!(accessors[1].function, accessors[2].function);
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document.parameters.reverse();
    let CompiledLogic::Wgsl {
        accessors: reordered,
        ..
    } = project.compile_logic().unwrap()
    else {
        unreachable!()
    };
    for old in accessors {
        let new = reordered
            .iter()
            .find(|p| p.parameter == old.parameter)
            .unwrap();
        assert_eq!(old.function, new.function);
        assert_eq!(old.slot + new.slot, 2);
    }
}

#[test]
fn all_first_version_wgsl_parameter_types_compile_through_the_actual_portable_validator() {
    let mut project = wgsl();
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document.parameters = ["float", "vec2", "vec3", "color", "bool", "enum"]
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            let mut p = parameter(&format!("p{i}"), kind);
            if matches!(*kind, "bool" | "enum") {
                p.default = [0.; 4];
                p.step = 1.;
            }
            if *kind == "enum" {
                p.options = vec!["A".into(), "B".into()];
            }
            p
        })
        .collect();
    document.source = "fn plugin_fx(p:vec2<f32>)->vec4<f32> { return vec4(param_p1(),param_p0(),1.0) + param_p3() * select(0.0,1.0,param_p4()) + vec4(param_p2(), f32(param_p5())); }".into();
    project.interface.root.content = LayoutContent::Column {
        gap: LayoutSpacing::None,
        align: LayoutAlign::Stretch,
        children: vec![],
    };
    let CompiledLogic::Wgsl {
        shader, accessors, ..
    } = project.compile_logic().unwrap()
    else {
        unreachable!()
    };
    assert_eq!(accessors.len(), 6);
    assert!(shader.glsl.fragment.contains("void main"));
}

#[test]
fn source_and_layout_versions_unknown_fields_and_required_fields_are_checked() {
    let original = serde_json::to_value(wgsl()).unwrap();
    for version in [0, PROJECT_VERSION + 1] {
        let mut value = original.clone();
        value["format_version"] = version.into();
        assert!(PluginProject::from_json(&value.to_string()).is_err());
    }
    for field in ["format_version", "package", "logic", "interface"] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            PluginProject::from_json(&value.to_string()).is_err(),
            "{field}"
        );
    }
    for pointer in [
        "",
        "/logic",
        "/logic/document",
        "/interface",
        "/interface/root",
    ] {
        let mut value = original.clone();
        value.pointer_mut(pointer).unwrap()["unrecognized"] = true.into();
        assert!(
            PluginProject::from_json(&value.to_string()).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn failed_save_preserves_existing_bytes_and_does_not_leave_a_staged_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("source.msplugin");
    let project = wgsl();
    project.save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut broken = project.clone();
    broken.interface.root.width = LayoutExtent::Weight { value: f32::NAN };
    assert!(broken.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let directory = root.path().join("directory.msplugin");
    std::fs::create_dir(&directory).unwrap();
    let error = project.save(&directory).unwrap_err();
    assert_eq!(error.phase(), Phase::Write);
    assert!(error.source().is_some());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(project.save(&root.path().join("source.msfx")).is_err());
}

#[test]
fn bounded_reads_and_nonfinite_parameter_checks_fail_before_replacing_source() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("oversize.msplugin");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_PROJECT_BYTES as u64 + 1).unwrap();
    drop(file);
    assert_eq!(PluginProject::load(&path).unwrap_err().phase(), Phase::Read);
    let error = PluginProject::load(&root.path().join("missing.msplugin")).unwrap_err();
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .is_some());
    let mut project = wgsl();
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document.parameters[0].default[0] = f32::INFINITY;
    assert!(project.to_json().is_err());
}

#[test]
fn graph_image_content_and_native_error_causes_survive_the_authoring_envelope() {
    let mut graph = motion_graph::demo_graph();
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[12, 34, 56, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    let image = motion_graph::GraphImage::from_png(png).unwrap();
    let image_id = image.id().to_owned();
    graph.format_version = motion_graph::IMAGE_ASSETS_FORMAT_VERSION;
    graph.images.insert(image_id.clone(), image);
    let project = PluginProject::from_graph(graph, identity()).unwrap();
    assert_eq!(
        PluginProject::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
    let mut value = serde_json::to_value(&project).unwrap();
    value["logic"]["document"]["images"][&image_id]["png"] = "not valid base64!!".into();
    let error = PluginProject::from_json(&value.to_string()).unwrap_err();
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<motion_graph::Error>()
        .is_some());
}

#[test]
fn authored_source_and_parameter_metadata_obey_independent_budgets() {
    let mut project = wgsl();
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document.source = " ".repeat(MAX_SOURCE_BYTES + 1);
    assert!(project
        .to_json()
        .unwrap_err()
        .to_string()
        .contains("logic.source"));
    let mut project = wgsl();
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document.parameters[0].name = "x".repeat(MAX_PARAMETER_METADATA_BYTES + 1);
    assert!(project
        .to_json()
        .unwrap_err()
        .to_string()
        .contains("logic.parameters"));
    let mut project = wgsl();
    let PluginLogic::Wgsl { document } = &mut project.logic else {
        unreachable!()
    };
    document.parameters[0].options = vec!["option".into(); 257];
    assert!(project.to_json().is_err());
}

#[test]
fn checked_in_source_reopens_and_compiles_all_its_real_parameter_bindings() {
    let project =
        PluginProject::from_json(include_str!("../examples/parameter-tint.msplugin")).unwrap();
    assert_eq!(
        PluginProject::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
    let CompiledLogic::Wgsl {
        accessors, shader, ..
    } = project.compile_logic().unwrap()
    else {
        unreachable!()
    };
    assert_eq!(
        accessors
            .iter()
            .map(|a| a.parameter.as_str())
            .collect::<Vec<_>>(),
        ["enabled", "tint", "amount"]
    );
    assert!(shader.wgsl.contains("param_tint()"));
}

#[test]
fn default_error_summaries_bound_native_context_without_losing_the_original_cause() {
    let native = "original cause\n".repeat(1024);
    let error = Error::Io {
        phase: Phase::Read,
        path: "source.msplugin".into(),
        cause: std::io::Error::other(native.clone()),
    };
    for summary in [error.to_string(), format!("{error:?}")] {
        assert!(summary.chars().count() < 512);
        assert!(!summary.contains('\n'));
        assert!(summary.contains("original cause"));
    }
    assert_eq!(error.source().unwrap().to_string(), native);
}
