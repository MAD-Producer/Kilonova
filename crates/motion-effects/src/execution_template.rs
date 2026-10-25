//! Package-local image programs. Sizes, precision, origins and storage are bound
//! by the consumer; a reusable package never serializes a frame's texture IDs.
use crate::execution_plan::*;
use crate::image_contract::{ChannelPrecision, ImageContract, ImageMeaning};
use crate::shader::{CompiledCodeIdentity, CompiledShader, compiled_code_identity};
use crate::{AlphaMode, EffectDefinition, RendererKind, WorkingSpace};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const EXECUTION_TEMPLATE_VERSION: u32 = 1;
pub const EXECUTION_TEMPLATE_CAPABILITY: &str = "image_execution_v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TemplateValue {
    Input,
    Node { id: u16 },
}
impl TemplateValue {
    fn value(self) -> ImageValueId {
        ImageValueId(match self {
            Self::Input => 0,
            Self::Node { id } => u32::from(id) + 1,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateNode {
    pub id: u16,
    pub pass: u16,
    pub input: TemplateValue,
    pub source: TemplateValue,
    pub code_identity: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplatePrecision {
    Working,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTemplate {
    pub version: u32,
    pub required_capabilities: BTreeSet<ExecutionCapability>,
    pub working_space: WorkingSpace,
    pub alpha_mode: AlphaMode,
    pub precision: TemplatePrecision,
    /// Ordered package bindings; numeric overrides retain shader_data's contract.
    pub resources: Vec<String>,
    pub nodes: Vec<TemplateNode>,
    pub output: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error(
        "phase=runtime_version: execution template version {actual} is unsupported; required={EXECUTION_TEMPLATE_VERSION}"
    )]
    Version { actual: u32 },
    #[error(
        "phase=runtime_contract: template must match the image effect's color, alpha and resource bindings"
    )]
    Contract,
    #[error(
        "phase=runtime_binding: every declared shader pass must have exactly one template node; pass={pass:?}"
    )]
    Binding { pass: Option<u16> },
    #[error(
        "phase=runtime_code: node={node} pass={pass} compiled implementation differs from the package template"
    )]
    Code { node: u16, pass: u16 },
    #[error("phase=runtime_topology: node={node} is not an ancestor of the output")]
    Unused { node: u16 },
    #[error(
        "phase=runtime_storage: node={node:?} needs {required} simultaneous working images; maximum={maximum}"
    )]
    Storage {
        node: ExecutionNodeId,
        required: u64,
        maximum: u64,
    },
    #[error(transparent)]
    Plan(#[from] ExecutionPlanError),
}

pub struct TemplateContext {
    pub origin: OperationOrigin,
    pub precision: ChannelPrecision,
    pub input_size: [u32; 2],
    pub output_size: [u32; 2],
}
impl TemplateContext {
    pub fn package() -> Self {
        Self {
            origin: OperationOrigin {
                composition: "package-template".into(),
                layer: 0,
                effect: None,
            },
            precision: ChannelPrecision::Unorm8,
            input_size: [1; 2],
            output_size: [1; 2],
        }
    }
}

pub fn image_template_capabilities() -> BTreeSet<ExecutionCapability> {
    BTreeSet::from([ExecutionCapability::ImageDagV1])
}

pub fn image_template_limits() -> ExecutionLimits {
    ExecutionLimits {
        nodes: crate::MAX_PASSES + 1,
        edges: crate::MAX_PASSES * 2,
        passes: crate::MAX_PASSES as u64,
        shader_bytes: 64 * 1024 * 1024,
        static_loop_work: u64::MAX,
        samples_per_fragment: u64::MAX,
        require_sample_bounds: false,
        uniform_cells_per_node: crate::MAX_PARAMS as u16,
        resource_inputs_per_node: 4,
        rectangle_side: i32::MAX as u32,
        halo_pixels: 0,
        history_samples: 0,
    }
}

pub struct CheckedTemplate {
    plan: ExecutionPlan,
    checked: CheckedExecutionOrder,
    passes: BTreeMap<ExecutionNodeId, u16>,
}
impl CheckedTemplate {
    pub fn plan(&self) -> &ExecutionPlan {
        &self.plan
    }
    pub fn checked(&self) -> &CheckedExecutionOrder {
        &self.checked
    }
    pub fn pass(&self, node: ExecutionNodeId) -> Option<u16> {
        self.passes.get(&node).copied()
    }
}

impl ExecutionTemplate {
    pub fn single_pass(effect: &EffectDefinition, shader: &CompiledShader) -> Self {
        Self {
            version: EXECUTION_TEMPLATE_VERSION,
            required_capabilities: image_template_capabilities(),
            working_space: effect.working_space,
            alpha_mode: effect.alpha_mode,
            precision: TemplatePrecision::Working,
            resources: effect.resources.clone(),
            nodes: vec![TemplateNode {
                id: 0,
                pass: 0,
                input: TemplateValue::Input,
                source: TemplateValue::Input,
                code_identity: compiled_code_identity(shader).digest,
            }],
            output: 0,
        }
    }

    pub(crate) fn validate_structure(
        &self,
        effect: &EffectDefinition,
    ) -> Result<(), TemplateError> {
        self.check_inner(
            effect,
            None,
            &TemplateContext::package(),
            &image_template_capabilities(),
            image_template_limits(),
        )
        .map(|_| ())
    }

    /// Actual consumers supply identities derived from their compiled programs.
    /// The checked graph, rather than serialized vector order, defines execution.
    pub fn check(
        &self,
        effect: &EffectDefinition,
        code: &[CompiledCodeIdentity],
        context: &TemplateContext,
        available: &BTreeSet<ExecutionCapability>,
        limits: ExecutionLimits,
    ) -> Result<CheckedTemplate, TemplateError> {
        self.check_inner(effect, Some(code), context, available, limits)
    }

    fn check_inner(
        &self,
        effect: &EffectDefinition,
        code: Option<&[CompiledCodeIdentity]>,
        context: &TemplateContext,
        available: &BTreeSet<ExecutionCapability>,
        limits: ExecutionLimits,
    ) -> Result<CheckedTemplate, TemplateError> {
        if self.version != EXECUTION_TEMPLATE_VERSION {
            return Err(TemplateError::Version {
                actual: self.version,
            });
        }
        if effect.renderer != RendererKind::Image
            || self.working_space != effect.working_space
            || self.alpha_mode != effect.alpha_mode
            || self.resources != effect.resources
        {
            return Err(TemplateError::Contract);
        }
        if self.nodes.is_empty()
            || self.nodes.len() > crate::MAX_PASSES
            || self.nodes.len() != effect.passes.len()
            || code.is_some_and(|c| c.len() != effect.passes.len())
        {
            return Err(TemplateError::Binding { pass: None });
        }
        let mut bindings = BTreeSet::new();
        for node in &self.nodes {
            if usize::from(node.pass) >= effect.passes.len() || !bindings.insert(node.pass) {
                return Err(TemplateError::Binding {
                    pass: Some(node.pass),
                });
            }
            if code.is_some_and(|c| c[usize::from(node.pass)].digest != node.code_identity) {
                return Err(TemplateError::Code {
                    node: node.id,
                    pass: node.pass,
                });
            }
        }
        let value_type = |size| ImageValueType {
            image: ImageContract::new(
                ImageMeaning::Color {
                    space: self.working_space,
                    alpha: self.alpha_mode,
                },
                context.precision,
            ),
            rect: PixelRect {
                origin: [0; 2],
                size,
            },
            coordinates: CoordinateDomain::Layer {
                layer: context.origin.layer,
            },
            depth: None,
        };
        let input_type = value_type(context.input_size);
        let output_type = value_type(context.output_size);
        let semantics = |domain| OperationSemantics {
            domain,
            time: TimeDependency::CurrentSample,
            halo: None,
            stateful: false,
            side_effects: false,
        };
        let zero_cost = OperationCost {
            shader_bytes: 0,
            static_loop_work: 0,
            samples_per_fragment: None,
            passes: 0,
        };
        let mut nodes = vec![ExecutionNode {
            id: ExecutionNodeId(0),
            origin: context.origin.clone(),
            inputs: vec![],
            output: ImageValueId(0),
            output_type: input_type,
            semantics: semantics(ExecutionDomain::Source),
            cost: zero_cost,
            code_identity: [0; 32],
            uniform_cells: 0,
        }];
        let mut passes = BTreeMap::new();
        for node in &self.nodes {
            let id = ExecutionNodeId(u32::from(node.id) + 1);
            passes.insert(id, node.pass);
            let inputs = [
                (ImagePortRole::Primary, node.input),
                (ImagePortRole::OriginalSource, node.source),
            ]
            .map(|(role, value)| ImageInputPort {
                role,
                value: value.value(),
                expected: if value == TemplateValue::Input {
                    input_type
                } else {
                    output_type
                },
            })
            .to_vec();
            let identity = code.map(|c| c[usize::from(node.pass)]);
            nodes.push(ExecutionNode {
                id,
                origin: context.origin.clone(),
                inputs,
                output: ImageValueId(id.0),
                output_type,
                semantics: semantics(ExecutionDomain::Image),
                cost: OperationCost {
                    shader_bytes: identity.map_or(0, |c| c.text_bytes),
                    static_loop_work: identity.map_or(0, |c| c.loop_work),
                    samples_per_fragment: None,
                    passes: 1,
                },
                code_identity: node.code_identity,
                uniform_cells: crate::MAX_PARAMS as u16,
            });
        }
        let plan = ExecutionPlan {
            version: EXECUTION_PLAN_VERSION,
            required_capabilities: self.required_capabilities.clone(),
            nodes,
            outputs: vec![ImageValueId(u32::from(self.output) + 1)],
        };
        let checked = plan.check(available, limits)?;
        let by_value: BTreeMap<_, _> = plan.nodes.iter().map(|n| (n.output, n)).collect();
        let mut used = BTreeSet::new();
        let mut pending = plan.outputs.clone();
        while let Some(value) = pending.pop() {
            if used.insert(value) {
                pending.extend(by_value[&value].inputs.iter().map(|p| p.value));
            }
        }
        for node in &self.nodes {
            if !used.contains(&TemplateValue::Node { id: node.id }.value()) {
                return Err(TemplateError::Unused { node: node.id });
            }
        }
        Ok(CheckedTemplate {
            plan,
            checked,
            passes,
        })
    }
}

#[cfg(test)]
mod tests;
