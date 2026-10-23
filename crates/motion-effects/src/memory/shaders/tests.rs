use super::*;

#[test]
fn real_compiled_text_shares_by_arc_but_independent_equal_copies_keep_capacity() {
    let package = crate::builtin::legacy_current_package().unwrap();
    let mut compiled = package.shaders.values().next().unwrap().clone();
    compiled
        .wgsl
        .push_str("\n// private-shader-text-never-returned\n");
    compiled.wgsl.reserve(128);
    let first = Arc::new(compiled);
    let second = Arc::new(first.as_ref().clone());
    let strong = Arc::strong_count(&first);
    let observed = compiled_shader_memory([("one", &first), ("alias", &first), ("copy", &second)]);
    assert_eq!(observed.resources.len(), 2);
    assert_eq!(observed.resources[0].program_references, 2);
    assert_eq!(observed.resources[1].program_references, 1);
    let capacity = |s: &CompiledShader| {
        (s.wgsl.capacity() + s.glsl.vertex.capacity() + s.glsl.fragment.capacity()) as u64
    };
    assert_eq!(
        observed.known_capacity_bytes,
        capacity(&first) + capacity(&second)
    );
    assert!(observed.known_capacity_bytes > observed.known_payload_bytes);
    assert_eq!(Arc::strong_count(&first), strong);
    assert!(observed.all_program_owners_observed);
    assert!(observed.unknown_bytes.is_none());
    assert!(!serde_json::to_string(&observed)
        .unwrap()
        .contains("private-shader-text-never-returned"));
    assert_eq!(
        compiled_shader_memory(std::iter::empty()).known_capacity_bytes,
        0
    );
}

#[test]
fn shader_observation_limits_bound_entries_and_context_without_zero_claims() {
    let shaders: Vec<_> = (0..MAX_SHADERS + 1)
        .map(|_| {
            Arc::new(CompiledShader {
                wgsl: "w".into(),
                glsl: crate::shader::GlslShader {
                    vertex: "v".into(),
                    fragment: "f".into(),
                    blocks: Default::default(),
                    textures: Default::default(),
                },
                sprite: false,
                additive: false,
                loop_work: 0,
            })
        })
        .collect();
    let keys: Vec<_> = (0..MAX_CONTEXTS + 3)
        .map(|i| format!("program-{i}-{}", "x".repeat(200)))
        .collect();
    let observed = compiled_shader_memory(
        shaders
            .iter()
            .map(|s| ("one", s))
            .chain(keys.iter().map(|key| (key.as_str(), &shaders[0]))),
    );
    assert_eq!(observed.resources.len(), MAX_SHADERS);
    assert_eq!(observed.unobserved_references, 1);
    assert!(!observed.all_program_owners_observed);
    assert_eq!(observed.known_capacity_bytes, (MAX_SHADERS * 3) as u64);
    assert_eq!(observed.resources[0].programs.len(), MAX_CONTEXTS);
    assert!(observed.resources[0]
        .programs
        .iter()
        .all(|key| key.chars().count() <= 160));
    assert_eq!(observed.resources[0].omitted_program_contexts, 4);
    assert_eq!(observed.resources[0].program_references, 12);
    assert!(observed.unknown_bytes.is_none());
}
