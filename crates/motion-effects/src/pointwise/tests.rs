use super::*;

fn proof(op: PointwiseOp) -> PointwiseProof {
    let package = builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == op.id())
        .unwrap();
    let Eligibility::Proven(proof) = prove_builtin(&package, definition).unwrap() else {
        panic!(
            "current {} implementation requires an updated pointwise proof",
            op.id()
        );
    };
    proof
}
fn stage(op: PointwiseOp, object: u64, effect: u64) -> PointwiseStage {
    let proof = proof(op);
    let values: Vec<_> = proof.parameters().iter().map(|p| p.default).collect();
    PointwiseStage::new(proof, StageNamespace { object, effect }, &values).unwrap()
}

/// Model a caller forging public package fields, including the claimed hash.
/// Proof must bind source, semantic metadata and both compiled representations.
fn candidate(op: PointwiseOp) -> (EffectPackage, EffectDefinition) {
    let reference = builtin::package().unwrap();
    let definition = reference
        .manifest
        .effects
        .iter()
        .find(|e| e.id == op.id())
        .unwrap()
        .clone();
    let mut manifest = reference.manifest.clone();
    manifest.effects = vec![definition.clone()];
    let shader_path = &definition.passes[0].shader;
    let key = (definition.id.clone(), 0);
    (
        EffectPackage {
            manifest,
            hash: reference.hash.clone(),
            files: [(shader_path.clone(), reference.files[shader_path].clone())].into(),
            shaders: [(key.clone(), reference.shaders[&key].clone())].into(),
            bytes: vec![],
        },
        definition,
    )
}

#[test]
fn changed_implementation_cannot_acquire_proof_with_a_recognized_name_or_hash() {
    let (mut package, definition) = candidate(PointwiseOp::Invert);
    let path = &definition.passes[0].shader;
    package.files.insert(
        path.clone(),
        b"fn main_fx(p:vec2<f32>)->vec4<f32>{return sample_input(p+vec2(1.0));}".to_vec(),
    );
    assert!(matches!(
        prove_builtin(&package, &definition).unwrap(),
        Eligibility::Retain(RetainReason::SourceChanged)
    ));
    for glsl in [false, true] {
        let (mut package, definition) = candidate(PointwiseOp::Tint);
        let shader = package
            .shaders
            .get_mut(&(definition.id.clone(), 0))
            .unwrap();
        if glsl {
            shader.glsl.fragment.push_str("\n// altered backend");
        } else {
            shader.wgsl.push_str("\n// altered backend");
        }
        assert!(matches!(
            prove_builtin(&package, &definition).unwrap(),
            Eligibility::Retain(RetainReason::CompiledImplementationChanged)
        ));
    }
}

#[test]
fn semantic_changes_and_unrecognized_packages_keep_a_normal_boundary() {
    let (package, mut definition) = candidate(PointwiseOp::Tint);
    definition.alpha_mode = AlphaMode::Premultiplied;
    assert!(matches!(
        prove_builtin(&package, &definition).unwrap(),
        Eligibility::Retain(RetainReason::DefinitionChanged)
    ));
    definition.id = "unknown_pointwise_claim".into();
    assert!(matches!(
        prove_builtin(&package, &definition).unwrap(),
        Eligibility::Retain(RetainReason::UnknownOperation)
    ));
    let (mut package, definition) = candidate(PointwiseOp::Invert);
    package.manifest.sdk_version = 1;
    assert!(matches!(
        prove_builtin(&package, &definition).unwrap(),
        Eligibility::Retain(RetainReason::UnsupportedSdk)
    ));
}

#[test]
fn namespaced_numeric_updates_change_data_without_specializing_structure() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        // Reusing a local effect ID on different objects cannot alias uniforms.
        let mut stages = [
            stage(PointwiseOp::Invert, 1, 7),
            stage(PointwiseOp::Tint, 2, 7),
        ];
        let before = PointwiseChain::new(precision, &stages).unwrap();
        let binding = before
            .bindings()
            .iter()
            .find(|p| p.namespace.object == 2 && p.parameter == "p0002")
            .unwrap();
        let edited = binding.slot.index();
        stages[1].values[2] = [0.25, 0.5, 0.75, 0.];
        let after = PointwiseChain::new(precision, &stages).unwrap();
        assert_eq!(before.structure_key(), after.structure_key());
        assert_eq!(before.bindings(), after.bindings());
        assert_eq!(after.uniforms()[edited], [0.25, 0.5, 0.75, 0.]);
        for i in 0..MAX_PARAMS {
            if i != edited {
                assert_eq!(before.uniforms()[i], after.uniforms()[i]);
            }
        }
        stages[1].namespace.object = 123;
        assert_eq!(
            before.structure_key(),
            PointwiseChain::new(precision, &stages)
                .unwrap()
                .structure_key()
        );
        stages.reverse();
        assert_ne!(
            before.structure_key(),
            PointwiseChain::new(precision, &stages)
                .unwrap()
                .structure_key()
        );
    }
}

#[test]
fn bounded_groups_refuse_aliasing_and_oversized_layouts_before_retaining_uniforms() {
    let same = stage(PointwiseOp::Tint, 1, 1);
    assert_eq!(
        PointwiseChain::new(ChannelPrecision::Unorm8, &[same.clone()]).unwrap_err(),
        RetainReason::SingleStage
    );
    assert_eq!(
        PointwiseChain::new(ChannelPrecision::Unorm8, &[same.clone(), same]).unwrap_err(),
        RetainReason::DuplicateNamespace
    );
    let oversized: Vec<_> = (0..9).map(|i| stage(PointwiseOp::Tint, 1, i)).collect();
    assert_eq!(
        PointwiseChain::new(ChannelPrecision::Float32, &oversized).unwrap_err(),
        RetainReason::ParameterLimit
    );
    let oversized: Vec<_> = (0..17).map(|i| stage(PointwiseOp::Invert, 1, i)).collect();
    assert_eq!(
        PointwiseChain::new(ChannelPrecision::Float16, &oversized).unwrap_err(),
        RetainReason::StageLimit
    );
}

#[test]
fn invalid_numeric_payload_keeps_object_effect_and_parameter_without_printing_values() {
    let proof = proof(PointwiseOp::Invert);
    let mut values: Vec<_> = proof.parameters().iter().map(|p| p.default).collect();
    values[0][0] = f32::NAN;
    let error = PointwiseStage::new(
        proof,
        StageNamespace {
            object: 42,
            effect: 17,
        },
        &values,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("object=42")
            && error.contains("effect=17")
            && error.contains("param=channel"),
        "{error}"
    );
    assert!(!error.contains("NaN"));
}

#[test]
fn all_precisions_compile_to_both_backends_and_keep_bounded_sampling() {
    let stages = [
        stage(PointwiseOp::Invert, 10, 11),
        stage(PointwiseOp::Tint, 10, 12),
        stage(PointwiseOp::Invert, 10, 13),
    ];
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let chain = PointwiseChain::new(precision, &stages).unwrap();
        let fused = chain
            .compile()
            .unwrap_or_else(|e| panic!("{precision:?}: {e}"));
        assert_eq!(fused.metrics.stages, 3);
        assert_eq!(fused.metrics.uniform_cells, 10);
        // The existing portable loop-cost ABI uses a base multiplier of one.
        assert_eq!(fused.metrics.static_loop_work, 1);
        assert_eq!(fused.metrics.shader_texture_samples_upper_bound, 2);
        assert!(fused.metrics.generated_source_bytes < 32 * 1024);
        assert!(!fused.compiled.glsl.fragment.is_empty());
        assert!(!fused.compiled.glsl.vertex.is_empty());
        assert!(!fused.compiled.sprite);
        // Two intermediate canonical stores and three working conversions:
        // the last canonical store belongs to the physical output attachment.
        let source = chain.generated_source().unwrap();
        assert_eq!(
            source
                .matches("current=pointwise_canonical_store(current)")
                .count(),
            2
        );
        assert_eq!(
            source
                .matches("=pointwise_work_store(convert_pixel")
                .count(),
            3
        );
        assert_eq!(source.matches("=pointwise_work_store(mix(").count(), 3);
    }
}

#[test]
fn numeric_values_and_instance_namespaces_never_enter_generated_shader_code() {
    let mut stages = [
        stage(PointwiseOp::Tint, 1, 2),
        stage(PointwiseOp::Invert, 1, 3),
    ];
    let original = PointwiseChain::new(ChannelPrecision::Float16, &stages).unwrap();
    stages[0].values[1] = [0.1234567, 0.7654321, 0.2345678, 0.];
    stages[0].namespace = StageNamespace {
        object: 999_123,
        effect: 987_654,
    };
    let edited = PointwiseChain::new(ChannelPrecision::Float16, &stages).unwrap();
    assert_ne!(original.uniforms(), edited.uniforms());
    assert_ne!(original.bindings(), edited.bindings());
    assert_eq!(original.structure_key(), edited.structure_key());
    assert_eq!(
        original.generated_source().unwrap(),
        edited.generated_source().unwrap()
    );
    let original = original.compile().unwrap();
    let edited = edited.compile().unwrap();
    assert!(same_compilation(&original.compiled, &edited.compiled));
}
