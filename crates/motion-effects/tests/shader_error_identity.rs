use motion_effects::shader;
use std::error::Error as _;

const HEALTHY_HOST: &str = r#"
@vertex fn vertex_main() -> @builtin(position) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}
@fragment fn fragment_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.5, 0.75, 1.0);
}
"#;

#[test]
fn plugin_parse_failure_retains_the_native_parser_error() {
    let error = shader::compile(
        "fn main_fx(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(; }",
        "main_fx",
    )
    .unwrap_err();
    let source = error.source().expect("native WGSL parser cause");
    assert!(source
        .downcast_ref::<naga::front::wgsl::ParseError>()
        .is_some());
    assert!(error.to_string().contains("error:"));
    assert!(error.to_string().contains("phase=wgsl_parse"));
}

#[test]
fn host_parse_failure_retains_the_native_parser_error() {
    let error =
        shader::compile_host_program("not valid WGSL", "vertex_main", "fragment_main").unwrap_err();
    let source = error.source().expect("native host WGSL parser cause");
    assert!(source
        .downcast_ref::<naga::front::wgsl::ParseError>()
        .is_some());
    assert!(error.to_string().contains("error:"));
    assert!(error.to_string().contains("phase=wgsl_parse"));
}

#[test]
fn plugin_validation_failure_retains_the_native_span_and_context() {
    let error = shader::compile(
        "struct Bad { values: array<f32>, after: f32, } fn main_fx(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0); }",
        "main_fx",
    )
    .unwrap_err();
    let source = error.source().expect("native WGSL validation cause");
    assert!(
        source
            .downcast_ref::<naga::WithSpan<naga::valid::ValidationError>>()
            .is_some(),
        "{error:?}"
    );
    assert!(error.to_string().starts_with("WGSL validation: "));
}

#[test]
fn host_validation_failure_retains_the_native_span_and_context() {
    let source = HEALTHY_HOST
        .replace(
            "@builtin(position) vec4<f32>",
            "@builtin(position) vec4<u32>",
        )
        .replace(
            "return vec4<f32>(0.0, 0.0, 0.0, 1.0);",
            "return vec4<u32>(0u, 0u, 0u, 1u);",
        );
    let error = shader::compile_host_program(&source, "vertex_main", "fragment_main").unwrap_err();
    let source = error.source().expect("native host WGSL validation cause");
    assert!(
        source
            .downcast_ref::<naga::WithSpan<naga::valid::ValidationError>>()
            .is_some(),
        "{error:?}"
    );
    assert!(error.to_string().starts_with("host WGSL validation: "));
}

#[test]
fn missing_vertex_entry_retains_the_native_glsl_error() {
    let error =
        shader::compile_host_program(HEALTHY_HOST, "missing_vertex", "fragment_main").unwrap_err();
    assert!(error
        .source()
        .expect("native GLSL writer cause")
        .downcast_ref::<naga::back::glsl::Error>()
        .is_some());
    assert!(error.to_string().starts_with("GLSL ES 300: "));
    assert!(error.to_string().contains("phase=writer_init stage=Vertex"));
}

#[test]
fn missing_fragment_entry_retains_the_native_glsl_error() {
    let error =
        shader::compile_host_program(HEALTHY_HOST, "vertex_main", "missing_fragment").unwrap_err();
    assert!(error
        .source()
        .expect("native GLSL writer cause")
        .downcast_ref::<naga::back::glsl::Error>()
        .is_some());
    assert!(error.to_string().starts_with("GLSL ES 300: "));
    assert!(error
        .to_string()
        .contains("phase=writer_init stage=Fragment"));
}

#[test]
fn glsl_writer_output_failure_retains_the_native_conflicting_sampler_error() {
    let source = format!(
        "@group(0) @binding(0) var image: texture_2d<f32>;\n\
         @group(0) @binding(1) var first: sampler;\n\
         @group(0) @binding(2) var second: sampler;\n{}",
        HEALTHY_HOST.replace(
            "return vec4<f32>(0.25, 0.5, 0.75, 1.0);",
            "return textureSampleLevel(image, first, vec2<f32>(0.5), 0.0) + textureSampleLevel(image, second, vec2<f32>(0.5), 0.0);",
        )
    );
    let error = shader::compile_host_program(&source, "vertex_main", "fragment_main").unwrap_err();
    assert!(matches!(
        error
            .source()
            .unwrap()
            .downcast_ref::<naga::back::glsl::Error>(),
        Some(naga::back::glsl::Error::ImageMultipleSamplers)
    ));
    assert_eq!(
        error.to_string(),
        "GLSL ES 300: phase=writer_output stage=Fragment: A image was used with multiple samplers"
    );
}

#[test]
fn healthy_programs_compile_after_failures_without_changing_their_output() {
    let before =
        shader::compile_host_program(HEALTHY_HOST, "vertex_main", "fragment_main").unwrap();
    let plugin =
        "fn main_fx(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.25, 0.5, 0.75, 1.0); }";
    let plugin_before = shader::compile(plugin, "main_fx").unwrap();
    assert!(shader::compile_host_program(HEALTHY_HOST, "missing", "fragment_main").is_err());
    assert!(shader::compile("invalid WGSL", "main_fx").is_err());
    let after = shader::compile_host_program(HEALTHY_HOST, "vertex_main", "fragment_main").unwrap();
    let plugin_after = shader::compile(plugin, "main_fx").unwrap();
    let snapshot = |host: &shader::GlslShader, plugin: &shader::CompiledShader| {
        serde_json::to_vec(&(
            host,
            &plugin.wgsl,
            &plugin.glsl,
            plugin.sprite,
            plugin.additive,
            plugin.loop_work,
        ))
        .unwrap()
    };
    let output = snapshot(&before, &plugin_before);
    assert_eq!(output, snapshot(&after, &plugin_after));
    if let Some(directory) = std::env::var_os("MOTION_SHADER_ERROR_RECEIPT_DIR") {
        let path = std::path::PathBuf::from(directory).join("healthy-programs.json");
        if std::env::var_os("MOTION_SHADER_ERROR_RECORD_BASELINE").is_some() {
            std::fs::write(path, &output).unwrap();
        } else {
            assert_eq!(std::fs::read(path).unwrap(), output);
        }
    }
}
