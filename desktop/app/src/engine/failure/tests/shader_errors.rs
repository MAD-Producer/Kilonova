use super::*;
use motion_effects::{shader, Error as EffectsError};

#[test]
fn native_shader_failures_keep_default_channels_and_healthy_compilation() {
    const CHILD: &str = "MOTION_SHADER_ERROR_LOG_CHILD";
    const NAME: &str = "engine::failure::tests::shader_errors::native_shader_failures_keep_default_channels_and_healthy_compilation";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let healthy =
            "fn main_fx(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.25, 0.5, 0.75, 1.0); }";
        let before = shader::compile(healthy, "main_fx").unwrap();
        let parse = shader::compile("invalid WGSL", "main_fx").unwrap_err();
        assert!(matches!(&parse, EffectsError::ShaderParse { .. }));
        let validation = shader::compile(
            "struct Bad { values: array<f32>, after: f32, } fn main_fx(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0); }",
            "main_fx",
        )
        .unwrap_err();
        assert!(matches!(&validation, EffectsError::ShaderValidation { .. }));
        let glsl = shader::compile_host_program(
            "@vertex fn vertex_main() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }",
            "missing_vertex",
            "missing_fragment",
        )
        .unwrap_err();
        assert!(matches!(
            &glsl,
            EffectsError::ShaderGlsl {
                phase: "writer_init",
                ..
            }
        ));
        let output_source = "@group(0) @binding(0) var image: texture_2d<f32>;\n\
            @group(0) @binding(1) var first: sampler;\n\
            @group(0) @binding(2) var second: sampler;\n\
            @vertex fn vertex_main() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }\n\
            @fragment fn fragment_main() -> @location(0) vec4<f32> { return textureSampleLevel(image, first, vec2<f32>(0.5), 0.0) + textureSampleLevel(image, second, vec2<f32>(0.5), 0.0); }";
        let output = shader::compile_host_program(output_source, "vertex_main", "fragment_main")
            .unwrap_err();
        assert!(matches!(
            &output,
            EffectsError::ShaderGlsl {
                phase: "writer_output",
                ..
            }
        ));
        let long_source = "invalid".repeat(1024);
        let long_parse = shader::compile(&long_source, "main_fx").unwrap_err();
        assert!(matches!(&long_parse, EffectsError::ShaderParse { .. }));
        assert!(long_parse.to_string().chars().count() > 2048);
        let mut causes = Vec::new();
        for (request, native) in [
            (811, parse),
            (812, validation),
            (813, glsl),
            (814, output),
            (815, long_parse),
        ] {
            let original = native.to_string();
            let error = motion_host::Error::from(motion_render::RenderError::from(native));
            assert!(matches!(
                &error,
                motion_host::Error::Render(motion_render::RenderError::Effects(_))
            ));
            assert_eq!(error.to_string(), original);
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_shader_compile",
                started: Instant::now(),
            }));
            respond(reply, Err(error));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            causes.push(original);
        }
        let after = shader::compile(healthy, "main_fx").unwrap();
        assert_eq!(before.wgsl, after.wgsl);
        assert_eq!(
            serde_json::to_vec(&before.glsl).unwrap(),
            serde_json::to_vec(&after.glsl).unwrap()
        );
        std::fs::write(
            root.join("primary-causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 5);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (811..816).zip(causes) {
        let marker = format!("error #{request} test_shader_compile ");
        // Existing log protocol flattens controls and bounds the diagnostic;
        // the child separately requires the complete, unchanged reply.
        let logged: String = cause
            .chars()
            .take(2048)
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect();
        for text in [&file, &stderr] {
            assert_eq!(
                text.lines().filter(|line| line.contains(&marker)).count(),
                1,
                "{text}"
            );
            assert!(text.contains(&logged), "{text}");
            if request == 815 {
                assert!(cause.chars().count() > logged.chars().count());
                assert!(!text.contains(&cause));
                let line = text.lines().find(|line| line.contains(&marker)).unwrap();
                assert!(line.contains("phase=wgsl_parse"));
            }
        }
        assert!(!stdout.contains(&cause));
    }
    for text in [&file, &stderr] {
        assert!(text.contains("phase=wgsl_parse"), "{text}");
        assert!(text.contains("phase=writer_init stage=Vertex"), "{text}");
        assert!(
            text.contains("phase=writer_output stage=Fragment"),
            "{text}"
        );
        assert_eq!(
            text.lines().filter(|line| line.contains("ERROR")).count(),
            6,
            "{text}"
        );
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("Conflicting samplers for "))
                .count(),
            1,
            "{text}"
        );
    }
    for marker in [
        "logging initialized",
        "test_shader_compile",
        "INFO",
        "ERROR",
    ] {
        assert!(!stdout.contains(marker), "{stdout}");
    }
}
