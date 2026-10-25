use super::*;
use crate::{AlphaMode, WorkingSpace, image_contract::ChannelPrecision};

fn value_type() -> ImageValueType {
    ImageValueType {
        image: ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            ChannelPrecision::Float16,
        ),
        rect: PixelRect {
            origin: [0, 0],
            size: [64, 64],
        },
        coordinates: CoordinateDomain::Composition,
        depth: None,
    }
}
fn node(id: u32, inputs: &[u32]) -> ExecutionNode {
    ExecutionNode {
        id: ExecutionNodeId(id),
        origin: OperationOrigin {
            composition: "main".into(),
            layer: u64::from(id),
            effect: Some(1),
        },
        inputs: inputs
            .iter()
            .enumerate()
            .map(|(port, &value)| ImageInputPort {
                role: if port == 0 {
                    ImagePortRole::Primary
                } else {
                    ImagePortRole::Resource {
                        index: port as u16 - 1,
                    }
                },
                value: ImageValueId(value),
                expected: value_type(),
            })
            .collect(),
        output: ImageValueId(id),
        output_type: value_type(),
        semantics: OperationSemantics {
            domain: ExecutionDomain::Image,
            time: TimeDependency::CurrentSample,
            halo: Some([0; 4]),
            stateful: false,
            side_effects: false,
        },
        cost: OperationCost {
            shader_bytes: 32,
            static_loop_work: 1,
            samples_per_fragment: Some(2),
            passes: 1,
        },
        code_identity: [id as u8; 32],
        uniform_cells: 4,
    }
}
fn diamond() -> ExecutionPlan {
    ExecutionPlan {
        version: EXECUTION_PLAN_VERSION,
        required_capabilities: BTreeSet::from([ExecutionCapability::ImageDagV1]),
        nodes: vec![node(4, &[2, 3]), node(3, &[1]), node(2, &[1]), node(1, &[])],
        outputs: vec![ImageValueId(4)],
    }
}
fn available() -> BTreeSet<ExecutionCapability> {
    BTreeSet::from([ExecutionCapability::ImageDagV1])
}
fn limits() -> ExecutionLimits {
    ExecutionLimits {
        nodes: MAX_EXECUTION_NODES,
        edges: MAX_EXECUTION_EDGES,
        passes: 8192,
        shader_bytes: 64 << 20,
        static_loop_work: 1 << 24,
        samples_per_fragment: 1 << 24,
        require_sample_bounds: true,
        uniform_cells_per_node: 32,
        resource_inputs_per_node: 8,
        rectangle_side: 16384,
        halo_pixels: 4096,
        history_samples: 64,
    }
}

#[test]
fn unknown_sampling_cost_is_neither_zero_nor_a_proof_for_budgeted_execution() {
    let mut plan = diamond();
    assert_eq!(
        plan.check(&available(), limits())
            .unwrap()
            .total_cost()
            .samples_per_fragment,
        Some(8)
    );
    plan.nodes[0].cost.samples_per_fragment = None;
    assert!(matches!(
        plan.check(&available(), limits()),
        Err(ExecutionPlanError::UnknownCost {
            node: ExecutionNodeId(4),
            resource: "samples_per_fragment"
        })
    ));
    let mut cap = limits();
    cap.require_sample_bounds = false;
    assert_eq!(
        plan.check(&available(), cap)
            .unwrap()
            .total_cost()
            .samples_per_fragment,
        None
    );
    plan.nodes[0].cost.samples_per_fragment = Some(0);
    assert_eq!(
        plan.check(&available(), limits())
            .unwrap()
            .total_cost()
            .samples_per_fragment,
        Some(6)
    );
}

#[test]
fn checked_diamond_has_one_logical_producer_and_explicit_terminal_lifetimes() {
    let plan = diamond();
    let reopened: ExecutionPlan =
        serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
    assert_eq!(reopened, plan);
    let checked = reopened.check(&available(), limits()).unwrap();
    assert_eq!(checked.order(), [1, 2, 3, 4].map(ExecutionNodeId));
    assert_eq!(checked.total_cost().passes, 4);
    let shared = checked
        .lifetimes()
        .iter()
        .find(|v| v.value == ImageValueId(1))
        .unwrap();
    assert_eq!(
        (
            shared.producer_step,
            shared.last_consumer_step,
            shared.consumer_count,
            shared.terminal
        ),
        (0, 2, 2, false)
    );
    let terminal = checked
        .lifetimes()
        .iter()
        .find(|v| v.value == ImageValueId(4))
        .unwrap();
    assert_eq!(
        (
            terminal.producer_step,
            terminal.last_consumer_step,
            terminal.consumer_count,
            terminal.terminal
        ),
        (3, 4, 1, true)
    );
    // This proves logical traversal only. Native shared execution is a separate
    // renderer integration test, not inferred from these counts.
}

#[test]
fn iterative_traversal_handles_the_maximum_chain_and_bounds_real_cycle_members() {
    let mut plan = diamond();
    plan.nodes = (1..=MAX_EXECUTION_NODES as u32)
        .map(|id| {
            let input = id - 1;
            node(
                id,
                if id == 1 {
                    &[]
                } else {
                    std::slice::from_ref(&input)
                },
            )
        })
        .collect();
    plan.outputs = vec![ImageValueId(MAX_EXECUTION_NODES as u32)];
    let checked = plan.check(&available(), limits()).unwrap();
    assert_eq!(checked.order().len(), MAX_EXECUTION_NODES);
    plan.nodes[0].inputs = vec![ImageInputPort {
        role: ImagePortRole::Primary,
        value: *plan.outputs.first().unwrap(),
        expected: value_type(),
    }];
    match plan.check(&available(), limits()).unwrap_err() {
        ExecutionPlanError::Cycle { members, total } => {
            assert_eq!(total, MAX_EXECUTION_NODES);
            assert_eq!(members.len(), 16);
        }
        other => panic!("{other}"),
    }
    // A residual descendant is not itself a member of its upstream cycle.
    plan.nodes = vec![node(1, &[3]), node(2, &[3]), node(3, &[2])];
    plan.outputs = vec![ImageValueId(1)];
    match plan.check(&available(), limits()).unwrap_err() {
        ExecutionPlanError::Cycle { members, total } => {
            assert_eq!(total, 2);
            assert_eq!(
                members.into_iter().collect::<BTreeSet<_>>(),
                BTreeSet::from([ExecutionNodeId(2), ExecutionNodeId(3)])
            );
        }
        other => panic!("{other}"),
    }
}

#[test]
fn edge_mismatch_preserves_node_port_value_and_both_image_contracts() {
    let mut plan = diamond();
    let expected = &mut plan.nodes[0].inputs[0].expected;
    expected.image = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Srgb,
            alpha: AlphaMode::Straight,
        },
        ChannelPrecision::Unorm16,
    );
    let error = plan.check(&available(), limits()).unwrap_err();
    match &error {
        ExecutionPlanError::TypeMismatch {
            node,
            role,
            value,
            producer,
            expected,
            actual,
        } => {
            assert_eq!(
                (*node, *role, *value, *producer),
                (
                    ExecutionNodeId(4),
                    ImagePortRole::Primary,
                    ImageValueId(2),
                    ExecutionNodeId(2)
                )
            );
            assert_eq!(expected.image.precision, ChannelPrecision::Unorm16);
            assert_eq!(actual.image.precision, ChannelPrecision::Float16);
            assert_ne!(expected.image.meaning, actual.image.meaning);
        }
        other => panic!("{other}"),
    }
    assert!(error.to_string().contains("phase=value_contract"));
    plan.nodes[0].inputs[0].expected = value_type();
    plan.check(&available(), limits()).unwrap();
}

#[test]
fn malformed_identity_ports_and_terminal_edges_cannot_publish_a_checked_plan() {
    let mut p = diamond();
    p.nodes[0].id = ExecutionNodeId(3);
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::DuplicateNode {
            node: ExecutionNodeId(3)
        })
    ));
    p = diamond();
    p.nodes[0].output = ImageValueId(3);
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::DuplicateProducer {
            value: ImageValueId(3),
            ..
        })
    ));
    p = diamond();
    p.nodes[0].inputs[1].role = ImagePortRole::Primary;
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::DuplicatePort {
            node: ExecutionNodeId(4),
            role: ImagePortRole::Primary
        })
    ));
    p = diamond();
    p.nodes[0].inputs[0].value = ImageValueId(99);
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::MissingValue {
            node: Some(ExecutionNodeId(4)),
            value: ImageValueId(99),
            ..
        })
    ));
    p = diamond();
    p.outputs = vec![ImageValueId(4), ImageValueId(4)];
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::DuplicateOutput {
            value: ImageValueId(4)
        })
    ));
    diamond().check(&available(), limits()).unwrap();
}

#[test]
fn preparation_limits_reject_measured_counts_and_cost_overflow_without_wrapping() {
    let plan = diamond();
    let mut cap = limits();
    cap.nodes = 3;
    assert!(matches!(
        plan.check(&available(), cap),
        Err(ExecutionPlanError::Budget {
            resource: "nodes",
            required: 4,
            maximum: 3,
            ..
        })
    ));
    cap = limits();
    cap.edges = 3;
    assert!(matches!(
        plan.check(&available(), cap),
        Err(ExecutionPlanError::Budget {
            resource: "edges",
            required: 4,
            maximum: 3,
            ..
        })
    ));
    let mut p = plan.clone();
    p.nodes[0].uniform_cells = 33;
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::Budget {
            node: Some(ExecutionNodeId(4)),
            resource: "uniform_cells",
            required: 33,
            maximum: 32
        })
    ));
    p = plan.clone();
    p.nodes[0].cost.shader_bytes = u64::MAX;
    cap = limits();
    cap.shader_bytes = u64::MAX;
    assert!(matches!(
        p.check(&available(), cap),
        Err(ExecutionPlanError::CostOverflow {
            resource: "shader_bytes",
            ..
        })
    ));
    cap = limits();
    cap.rectangle_side = 63;
    assert!(matches!(
        plan.check(&available(), cap),
        Err(ExecutionPlanError::Budget {
            resource: "rectangle_side",
            required: 64,
            maximum: 63,
            ..
        })
    ));
    p = plan.clone();
    p.nodes[0].inputs[1].role = ImagePortRole::Resource { index: u16::MAX };
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::Budget {
            node: Some(ExecutionNodeId(4)),
            resource: "resource_inputs",
            required: 65536,
            maximum: 8,
        })
    ));
}

#[test]
fn version_and_capability_negotiation_require_explicit_supported_semantics() {
    let mut p = diamond();
    p.version = 0;
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::Version { actual: 0 })
    ));
    p = diamond();
    p.required_capabilities.insert(ExecutionCapability::History);
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::UnsupportedCapability {
            capability: ExecutionCapability::History
        })
    ));
    p = diamond();
    p.nodes[0].semantics.time = TimeDependency::History { samples: 1 };
    assert!(matches!(
        p.check(&available(), limits()),
        Err(ExecutionPlanError::UndeclaredCapability {
            node: Some(ExecutionNodeId(4)),
            capability: ExecutionCapability::History
        })
    ));
    p.required_capabilities.insert(ExecutionCapability::History);
    let consumer = BTreeSet::from([
        ExecutionCapability::ImageDagV1,
        ExecutionCapability::History,
    ]);
    p.check(&consumer, limits()).unwrap();
    p.nodes[0].semantics.time = TimeDependency::History { samples: 0 };
    assert!(matches!(
        p.check(&consumer, limits()),
        Err(ExecutionPlanError::EmptyHistory {
            node: ExecutionNodeId(4)
        })
    ));
    let mut json = serde_json::to_value(diamond()).unwrap();
    json["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ExecutionPlan>(json).is_err());
}

#[test]
fn value_contract_requires_depth_conventions_and_preserves_format_error_sources() {
    let mut value = value_type();
    value.image = ImageContract::new(ImageMeaning::RelativeDepth {}, ChannelPrecision::Unorm16);
    assert!(matches!(
        value.validate(),
        Err(ValueTypeError::Depth { .. })
    ));
    value.depth = Some(DepthConvention::RelativeNearOneFarZero);
    value.validate().unwrap();
    value.image = ImageContract::new(ImageMeaning::LinearDepth {}, ChannelPrecision::Float16);
    assert!(matches!(
        value.validate(),
        Err(ValueTypeError::Depth { .. })
    ));
    value.depth = Some(DepthConvention::PositiveSceneDistance);
    value.validate().unwrap();
    value.image.precision = ChannelPrecision::Unorm8;
    let error = value.validate().unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
    assert!(matches!(
        error,
        ValueTypeError::Image(ImageFormatError::InvalidPrecision { .. })
    ));
    value = value_type();
    value.rect.origin[0] = i32::MAX;
    assert!(matches!(value.validate(), Err(ValueTypeError::Rectangle)));
}
