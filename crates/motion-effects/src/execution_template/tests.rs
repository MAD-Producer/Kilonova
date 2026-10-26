use super::*;

#[test]
fn actual_compiled_costs_are_enforced_independently_and_unknown_samples_are_refused() {
    let (definition, shader, template) = fixture();
    let code = [compiled_code_identity(&shader)];
    let healthy = check(&template, &definition, &shader).unwrap();
    let cost = healthy.checked().total_cost();
    assert!(cost.shader_bytes > 0);
    assert!(cost.static_loop_work > 0);
    assert_eq!(cost.samples_per_fragment, Some(2));
    for resource in ["shader_bytes", "static_loop_work", "samples_per_fragment"] {
        let mut limits = image_template_limits();
        let (slot, required) = match resource {
            "shader_bytes" => (&mut limits.shader_bytes, cost.shader_bytes),
            "static_loop_work" => (&mut limits.static_loop_work, cost.static_loop_work),
            _ => (&mut limits.samples_per_fragment, 2),
        };
        *slot = required - 1;
        let error = template
            .check(
                &definition,
                &code,
                &TemplateContext::package(),
                &image_template_capabilities(),
                limits,
            )
            .err()
            .unwrap();
        assert!(
            matches!(error, TemplateError::Plan(ExecutionPlanError::Budget {
            resource: actual, required: actual_required, maximum, ..
        }) if actual == resource && actual_required == required && maximum == required - 1),
            "{error}"
        );
    }
    let mut unknown = shader.clone();
    unknown.samples_per_fragment = None;
    let unknown_template = ExecutionTemplate::single_pass(&definition, &unknown);
    assert!(matches!(
        check(&unknown_template, &definition, &unknown),
        Err(TemplateError::Plan(ExecutionPlanError::UnknownCost {
            resource: "samples_per_fragment",
            ..
        }))
    ));
    // Structure-only package validation does not pretend to have code costs.
    template.validate_structure(&definition).unwrap();
    assert_eq!(
        check(&template, &definition, &shader)
            .unwrap()
            .checked()
            .total_cost()
            .samples_per_fragment,
        Some(2)
    );
}

fn fixture() -> (EffectDefinition, CompiledShader, ExecutionTemplate) {
    let package = crate::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "invert")
        .unwrap()
        .clone();
    let shader = package.shaders[&(definition.id.clone(), 0)].clone();
    let template = ExecutionTemplate::single_pass(&definition, &shader);
    (definition, shader, template)
}
fn check(
    template: &ExecutionTemplate,
    definition: &EffectDefinition,
    shader: &CompiledShader,
) -> Result<CheckedTemplate, TemplateError> {
    template.check(
        definition,
        &vec![compiled_code_identity(shader); definition.passes.len()],
        &TemplateContext::package(),
        &image_template_capabilities(),
        image_template_limits(),
    )
}

#[test]
fn runtime_template_binds_current_precision_extent_and_origin_without_texture_ids() {
    let (definition, shader, template) = fixture();
    let bytes = serde_json::to_vec(&template).unwrap();
    let template: ExecutionTemplate = serde_json::from_slice(&bytes).unwrap();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let context = TemplateContext {
            origin: OperationOrigin {
                composition: "active".into(),
                layer: 41,
                effect: Some(9),
            },
            precision,
            input_size: [32, 24],
            output_size: [48, 40],
        };
        let result = template
            .check(
                &definition,
                &[compiled_code_identity(&shader)],
                &context,
                &image_template_capabilities(),
                image_template_limits(),
            )
            .unwrap();
        assert_eq!(
            result.checked().order(),
            &[ExecutionNodeId(0), ExecutionNodeId(1)]
        );
        let output = &result.plan().nodes[1];
        assert_eq!(output.origin, context.origin);
        assert_eq!(output.output_type.rect.size, [48, 40]);
        assert_eq!(output.output_type.image.precision, precision);
        assert_eq!(output.inputs[0].expected.rect.size, [32, 24]);
        assert_eq!(result.pass(output.id), Some(0));
        assert_eq!(result.checked().total_cost().passes, 1);
        assert_eq!(result.checked().total_cost().samples_per_fragment, Some(2));
    }
}

#[test]
fn runtime_template_refuses_versions_caps_contracts_code_and_limits_then_recovers() {
    let (definition, mut shader, template) = fixture();
    let mut invalid = template.clone();
    invalid.version += 1;
    assert!(matches!(
        check(&invalid, &definition, &shader),
        Err(TemplateError::Version { .. })
    ));
    let code = [compiled_code_identity(&shader)];
    assert!(matches!(
        template.check(
            &definition,
            &code,
            &TemplateContext::package(),
            &BTreeSet::new(),
            image_template_limits()
        ),
        Err(TemplateError::Plan(
            ExecutionPlanError::UnsupportedCapability { .. }
        ))
    ));
    let mut limits = image_template_limits();
    limits.passes = 0;
    assert!(matches!(
        template.check(
            &definition,
            &code,
            &TemplateContext::package(),
            &image_template_capabilities(),
            limits
        ),
        Err(TemplateError::Plan(ExecutionPlanError::Budget {
            resource: "passes",
            ..
        }))
    ));
    invalid = template.clone();
    invalid.resources.push("assets/absent.png".into());
    assert!(matches!(
        check(&invalid, &definition, &shader),
        Err(TemplateError::Contract)
    ));
    shader
        .glsl
        .fragment
        .push_str("\n// implementation changed\n");
    assert!(matches!(
        check(&template, &definition, &shader),
        Err(TemplateError::Code { node: 0, pass: 0 })
    ));
    let (_, shader, _) = fixture();
    assert!(check(&template, &definition, &shader).is_ok());
}

#[test]
fn runtime_template_checks_unordered_topology_cycles_pass_bijection_and_terminal_closure() {
    let (mut definition, shader, mut template) = fixture();
    definition.passes = vec![definition.passes[0].clone(); 3];
    let node = |id, pass, input| TemplateNode {
        id,
        pass,
        input,
        source: TemplateValue::Input,
        code_identity: compiled_code_identity(&shader).digest,
    };
    template.nodes = vec![
        node(3, 2, TemplateValue::Node { id: 2 }),
        node(1, 0, TemplateValue::Input),
        node(2, 1, TemplateValue::Node { id: 1 }),
    ];
    template.output = 3;
    let checked = check(&template, &definition, &shader).unwrap();
    assert_eq!(
        checked.checked().order(),
        &[
            ExecutionNodeId(0),
            ExecutionNodeId(2),
            ExecutionNodeId(3),
            ExecutionNodeId(4)
        ]
    );
    assert_eq!(checked.checked().total_cost().passes, 3);
    let mut invalid = template.clone();
    invalid.nodes[1].input = TemplateValue::Node { id: 3 };
    assert!(matches!(
        check(&invalid, &definition, &shader),
        Err(TemplateError::Plan(ExecutionPlanError::Cycle {
            total: 3,
            ..
        }))
    ));
    invalid = template.clone();
    invalid.nodes[1].input = TemplateValue::Node { id: 99 };
    assert!(matches!(
        check(&invalid, &definition, &shader),
        Err(TemplateError::Plan(ExecutionPlanError::MissingValue { .. }))
    ));
    invalid = template.clone();
    invalid.nodes[1].pass = 2;
    assert!(matches!(
        check(&invalid, &definition, &shader),
        Err(TemplateError::Binding { pass: Some(2) })
    ));
    invalid = template.clone();
    invalid.output = 2;
    assert!(matches!(
        check(&invalid, &definition, &shader),
        Err(TemplateError::Unused { node: 3 })
    ));
    assert!(check(&template, &definition, &shader).is_ok());
}

#[test]
fn runtime_template_capability_declaration_is_required_and_optional_for_ordinary_effects() {
    let package = crate::builtin::package().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.effects.truncate(1);
    let definition = &mut manifest.effects[0];
    let shader = &package.shaders[&(definition.id.clone(), 0)];
    definition.execution = Some(ExecutionTemplate::single_pass(definition, shader));
    assert!(manifest
        .validate()
        .unwrap_err()
        .to_string()
        .contains("capability"));
    manifest.effects[0]
        .required_capabilities
        .push(EXECUTION_TEMPLATE_CAPABILITY.into());
    manifest.validate().unwrap();
    manifest.effects[0].execution = None;
    assert!(manifest
        .validate()
        .unwrap_err()
        .to_string()
        .contains("capability"));
    manifest.effects[0]
        .required_capabilities
        .retain(|c| c != EXECUTION_TEMPLATE_CAPABILITY);
    manifest.validate().unwrap();
}
