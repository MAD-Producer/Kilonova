use super::*;
use std::cell::Cell;

fn shared(error: &RenderError) -> &Arc<RenderError> {
    match error {
        RenderError::Shared(cause) => cause,
        other => panic!("expected shared native compiler error, got {other:?}"),
    }
}

#[test]
fn failed_program_cache_keeps_one_native_parser_cause_without_recompiling() {
    let cache = OnceLock::new();
    let calls = Cell::new(0);
    let compile = || {
        calls.set(calls.get() + 1);
        motion_effects::shader::compile("invalid WGSL", "main_fx")
    };
    let first = cached_program(&cache, compile).unwrap_err();
    let second = cached_program(&cache, compile).unwrap_err();
    assert_eq!(calls.get(), 1);
    assert!(Arc::ptr_eq(shared(&first), shared(&second)));
    let RenderError::Effects(motion_effects::Error::ShaderParse { cause, message }) =
        shared(&first).as_ref()
    else {
        panic!("parser cause was flattened: {first:?}");
    };
    assert!(!cause.to_string().is_empty());
    assert_eq!(
        first.to_string(),
        format!("WGSL parse (phase=wgsl_parse): {message}")
    );
    assert_eq!(second.to_string(), first.to_string());
}

#[test]
fn cached_glsl_failure_keeps_original_stage_phase_and_cause() {
    let cache = OnceLock::new();
    let compile = || {
        motion_effects::shader::compile_host_program(
            "@vertex fn vertex_main() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0); }",
            "missing_vertex",
            "missing_fragment",
        )
    };
    let first = cached_program(&cache, compile).unwrap_err();
    let second = cached_program(&cache, compile).unwrap_err();
    assert!(Arc::ptr_eq(shared(&first), shared(&second)));
    let RenderError::Effects(motion_effects::Error::ShaderGlsl {
        phase,
        stage,
        cause,
    }) = shared(&first).as_ref()
    else {
        panic!("GLSL cause was flattened: {first:?}");
    };
    assert_eq!(*phase, "writer_init");
    assert_eq!(format!("{stage:?}"), "Vertex");
    assert_eq!(
        first.to_string(),
        format!("GLSL ES 300: phase=writer_init stage=Vertex: {cause}")
    );
}

#[test]
fn failed_immutable_cache_does_not_affect_a_distinct_healthy_program() {
    let bad = OnceLock::new();
    assert!(cached_program(&bad, || motion_effects::shader::compile(
        "bad WGSL", "main_fx"
    ))
    .is_err());
    let healthy = OnceLock::new();
    let shader = cached_program(&healthy, || {
        motion_effects::shader::compile(BLEND, "main_fx")
    })
    .unwrap();
    let again = cached_program(&healthy, || {
        motion_effects::shader::compile("bad WGSL", "main_fx")
    })
    .unwrap();
    assert!(std::ptr::eq(shader, again));
    assert_eq!(shader.wgsl, again.wgsl);
    assert_eq!(
        serde_json::to_vec(&shader.glsl).unwrap(),
        serde_json::to_vec(&again.glsl).unwrap()
    );
    // The failed immutable cache remains failed; recovery is a distinct cache.
    assert!(cached_program(&bad, || motion_effects::shader::compile(BLEND, "main_fx")).is_err());
}

#[test]
fn production_mask_and_compositing_programs_reuse_unchanged_shader_data() {
    let blend = shader().unwrap();
    assert!(std::ptr::eq(blend, shader().unwrap()));
    let fresh = motion_effects::shader::compile(BLEND, "main_fx").unwrap();
    assert_eq!(blend.wgsl, fresh.wgsl);
    assert_eq!(
        serde_json::to_vec(&blend.glsl).unwrap(),
        serde_json::to_vec(&fresh.glsl).unwrap()
    );
    let planes = plane_programs().unwrap();
    assert!(std::ptr::eq(planes, plane_programs().unwrap()));
    assert_eq!(planes.len(), 3);
    let masks = crate::mask_plan::shaders().unwrap();
    assert!(std::ptr::eq(masks, crate::mask_plan::shaders().unwrap()));
    for (compiled, source) in masks
        .iter()
        .zip([crate::mask_plan::GAUSSIAN, crate::mask_plan::COMBINE])
    {
        let fresh = motion_effects::shader::compile(source, "main_fx").unwrap();
        assert_eq!(compiled.wgsl, fresh.wgsl);
        assert_eq!(
            serde_json::to_vec(&compiled.glsl).unwrap(),
            serde_json::to_vec(&fresh.glsl).unwrap()
        );
    }
}
