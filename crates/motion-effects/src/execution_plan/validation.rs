use super::*;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum ExecutionPlanError {
    #[error(
        "phase=version: execution plan version {actual} is unsupported; required={EXECUTION_PLAN_VERSION}"
    )]
    Version { actual: u32 },
    #[error("phase=capability: unsupported capability {capability:?}")]
    UnsupportedCapability { capability: ExecutionCapability },
    #[error("phase=capability: node={node:?} requires undeclared capability {capability:?}")]
    UndeclaredCapability {
        node: Option<ExecutionNodeId>,
        capability: ExecutionCapability,
    },
    #[error("phase=admission: node={node:?} {resource} requires {required}; maximum={maximum}")]
    Budget {
        node: Option<ExecutionNodeId>,
        resource: &'static str,
        required: u64,
        maximum: u64,
    },
    #[error("phase=admission: node={node:?} cost {resource} overflows u64")]
    CostOverflow {
        node: ExecutionNodeId,
        resource: &'static str,
    },
    #[error("phase=identity: duplicate node={node:?}")]
    DuplicateNode { node: ExecutionNodeId },
    #[error("phase=identity: value={value:?} has producers {first:?} and {second:?}")]
    DuplicateProducer {
        value: ImageValueId,
        first: ExecutionNodeId,
        second: ExecutionNodeId,
    },
    #[error(
        "phase=identity: node={node:?} composition identity must be visible text of at most 128 bytes"
    )]
    InvalidOrigin { node: ExecutionNodeId },
    #[error("phase=ports: node={node:?} has duplicate input role {role:?}")]
    DuplicatePort {
        node: ExecutionNodeId,
        role: ImagePortRole,
    },
    #[error("phase=ports: node={node:?} role={role:?} value={value:?} has no producer")]
    MissingValue {
        node: Option<ExecutionNodeId>,
        role: Option<ImagePortRole>,
        value: ImageValueId,
    },
    #[error("phase=value_contract: node={node:?} role={role:?}: {source}")]
    ValueType {
        node: ExecutionNodeId,
        role: Option<ImagePortRole>,
        #[source]
        source: ValueTypeError,
    },
    #[error(
        "phase=value_contract: node={node:?} role={role:?} value={value:?} producer={producer:?} expected={expected:?} actual={actual:?}"
    )]
    TypeMismatch {
        node: ExecutionNodeId,
        role: ImagePortRole,
        value: ImageValueId,
        producer: ExecutionNodeId,
        expected: ImageValueType,
        actual: ImageValueType,
    },
    #[error("phase=terminal: duplicate output value={value:?}")]
    DuplicateOutput { value: ImageValueId },
    #[error("phase=time: node={node:?} history sample count must be positive")]
    EmptyHistory { node: ExecutionNodeId },
    #[error("phase=topology: cycle contains {total} nodes; bounded members={members:?}")]
    Cycle {
        members: Vec<ExecutionNodeId>,
        total: usize,
    },
    #[error("phase=topology: residual node={node:?} has no residual predecessor")]
    InvalidResidual { node: ExecutionNodeId },
}

fn limit(
    node: Option<ExecutionNodeId>,
    resource: &'static str,
    required: u64,
    maximum: u64,
) -> Result<(), ExecutionPlanError> {
    if required > maximum {
        return Err(ExecutionPlanError::Budget {
            node,
            resource,
            required,
            maximum,
        });
    }
    Ok(())
}
fn add(
    sum: &mut u64,
    value: u64,
    maximum: u64,
    node: ExecutionNodeId,
    resource: &'static str,
) -> Result<(), ExecutionPlanError> {
    let required = sum
        .checked_add(value)
        .ok_or(ExecutionPlanError::CostOverflow { node, resource })?;
    limit(Some(node), resource, required, maximum)?;
    *sum = required;
    Ok(())
}

impl ExecutionPlan {
    /// Check logical dataflow and preparation budgets before the native owner
    /// negotiates formats or allocates resources. No shader is compiled here.
    pub fn check(
        &self,
        available: &BTreeSet<ExecutionCapability>,
        limits: ExecutionLimits,
    ) -> Result<CheckedExecutionOrder, ExecutionPlanError> {
        if self.version != EXECUTION_PLAN_VERSION {
            return Err(ExecutionPlanError::Version {
                actual: self.version,
            });
        }
        for &capability in &self.required_capabilities {
            if !available.contains(&capability) {
                return Err(ExecutionPlanError::UnsupportedCapability { capability });
            }
        }
        let require = |node, capability| {
            if self.required_capabilities.contains(&capability) {
                Ok(())
            } else {
                Err(ExecutionPlanError::UndeclaredCapability { node, capability })
            }
        };
        require(None, ExecutionCapability::ImageDagV1)?;
        limit(
            None,
            "nodes",
            self.nodes.len() as u64,
            limits.nodes.min(MAX_EXECUTION_NODES) as u64,
        )?;
        limit(
            None,
            "outputs",
            self.outputs.len() as u64,
            self.nodes.len() as u64,
        )?;
        // Bound every input vector and total edge count before traversal storage.
        let mut edges = 0_u64;
        for node in &self.nodes {
            limit(
                Some(node.id),
                "input_ports",
                node.inputs.len() as u64,
                MAX_NODE_INPUTS as u64,
            )?;
            add(
                &mut edges,
                node.inputs.len() as u64,
                limits.edges.min(MAX_EXECUTION_EDGES) as u64,
                node.id,
                "edges",
            )?;
        }
        let mut ids = BTreeMap::new();
        let mut producers = BTreeMap::new();
        let mut totals = [0_u64; 4];
        for (index, node) in self.nodes.iter().enumerate() {
            if ids.insert(node.id, index).is_some() {
                return Err(ExecutionPlanError::DuplicateNode { node: node.id });
            }
            if let Some(first) = producers.insert(node.output, index) {
                return Err(ExecutionPlanError::DuplicateProducer {
                    value: node.output,
                    first: self.nodes[first].id,
                    second: node.id,
                });
            }
            if node.origin.composition.is_empty()
                || node.origin.composition.len() > 128
                || node.origin.composition.trim().is_empty()
                || node.origin.composition.chars().any(char::is_control)
            {
                return Err(ExecutionPlanError::InvalidOrigin { node: node.id });
            }
            self.check_type(node, None, node.output_type, limits)?;
            limit(
                Some(node.id),
                "uniform_cells",
                u64::from(node.uniform_cells),
                u64::from(limits.uniform_cells_per_node),
            )?;
            if let Some(halo) = node.semantics.halo {
                for distance in halo {
                    limit(
                        Some(node.id),
                        "halo_pixels",
                        u64::from(distance),
                        u64::from(limits.halo_pixels),
                    )?;
                }
            }
            if node.semantics.domain == ExecutionDomain::Spatial {
                require(Some(node.id), ExecutionCapability::SpatialProducers)?;
            }
            if node.semantics.stateful {
                require(Some(node.id), ExecutionCapability::StatefulExecution)?;
            }
            if node.semantics.side_effects {
                require(Some(node.id), ExecutionCapability::SideEffects)?;
            }
            if let TimeDependency::History { samples } = node.semantics.time {
                if samples == 0 {
                    return Err(ExecutionPlanError::EmptyHistory { node: node.id });
                }
                require(Some(node.id), ExecutionCapability::History)?;
                limit(
                    Some(node.id),
                    "history_samples",
                    u64::from(samples),
                    u64::from(limits.history_samples),
                )?;
            }
            for (slot, (value, maximum, name)) in [
                (node.cost.shader_bytes, limits.shader_bytes, "shader_bytes"),
                (
                    node.cost.static_loop_work,
                    limits.static_loop_work,
                    "static_loop_work",
                ),
                (
                    node.cost.samples_per_fragment,
                    limits.samples_per_fragment,
                    "samples_per_fragment",
                ),
                (
                    u64::from(node.cost.passes),
                    limits.passes.min(u64::from(u32::MAX)),
                    "passes",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                add(&mut totals[slot], value, maximum, node.id, name)?;
            }
        }
        let mut dependents = vec![Vec::new(); self.nodes.len()];
        let mut remaining = vec![0_usize; self.nodes.len()];
        let mut consumers = vec![0_u32; self.nodes.len()];
        for (index, node) in self.nodes.iter().enumerate() {
            let mut roles = BTreeSet::new();
            for port in &node.inputs {
                if let ImagePortRole::Resource { index } = port.role {
                    limit(
                        Some(node.id),
                        "resource_inputs",
                        u64::from(index) + 1,
                        u64::from(limits.resource_inputs_per_node).min(MAX_NODE_INPUTS as u64),
                    )?;
                }
                if !roles.insert(port.role) {
                    return Err(ExecutionPlanError::DuplicatePort {
                        node: node.id,
                        role: port.role,
                    });
                }
                self.check_type(node, Some(port.role), port.expected, limits)?;
                let &producer =
                    producers
                        .get(&port.value)
                        .ok_or(ExecutionPlanError::MissingValue {
                            node: Some(node.id),
                            role: Some(port.role),
                            value: port.value,
                        })?;
                let actual = self.nodes[producer].output_type;
                if actual != port.expected {
                    return Err(ExecutionPlanError::TypeMismatch {
                        node: node.id,
                        role: port.role,
                        value: port.value,
                        producer: self.nodes[producer].id,
                        expected: port.expected,
                        actual,
                    });
                }
                dependents[producer].push(index);
                remaining[index] += 1;
                consumers[producer] += 1;
            }
        }
        let mut terminals = BTreeSet::new();
        for &value in &self.outputs {
            if !terminals.insert(value) {
                return Err(ExecutionPlanError::DuplicateOutput { value });
            }
            let &producer = producers
                .get(&value)
                .ok_or(ExecutionPlanError::MissingValue {
                    node: None,
                    role: None,
                    value,
                })?;
            consumers[producer] += 1;
        }
        let mut ready: BTreeSet<_> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| remaining[*index] == 0)
            .map(|(index, node)| (node.id, index))
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut steps = vec![0_u32; self.nodes.len()];
        while let Some((id, index)) = ready.pop_first() {
            steps[index] = order.len() as u32;
            order.push(id);
            for &consumer in &dependents[index] {
                remaining[consumer] -= 1;
                if remaining[consumer] == 0 {
                    ready.insert((self.nodes[consumer].id, consumer));
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err(self.cycle_error(&remaining, &producers));
        }
        let lifetimes = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let terminal = terminals.contains(&node.output);
                let last = if terminal {
                    self.nodes.len() as u32
                } else {
                    dependents[index]
                        .iter()
                        .map(|&consumer| steps[consumer])
                        .max()
                        .unwrap_or(steps[index])
                };
                ValueLifetime {
                    value: node.output,
                    producer_step: steps[index],
                    last_consumer_step: last,
                    consumer_count: consumers[index],
                    terminal,
                }
            })
            .collect();
        Ok(CheckedExecutionOrder {
            order,
            lifetimes,
            total_cost: OperationCost {
                shader_bytes: totals[0],
                static_loop_work: totals[1],
                samples_per_fragment: totals[2],
                passes: totals[3] as u32,
            },
        })
    }

    fn check_type(
        &self,
        node: &ExecutionNode,
        role: Option<ImagePortRole>,
        value: ImageValueType,
        limits: ExecutionLimits,
    ) -> Result<(), ExecutionPlanError> {
        value
            .validate()
            .map_err(|source| ExecutionPlanError::ValueType {
                node: node.id,
                role,
                source,
            })?;
        for side in value.rect.size {
            limit(
                Some(node.id),
                "rectangle_side",
                u64::from(side),
                u64::from(limits.rectangle_side),
            )?;
        }
        Ok(())
    }

    fn cycle_error(
        &self,
        remaining: &[usize],
        producers: &BTreeMap<ImageValueId, usize>,
    ) -> ExecutionPlanError {
        // Follow residual predecessors iteratively until one repeats. A tail
        // leading out of a cycle is excluded from the reported cycle members.
        let Some(mut cursor) = remaining.iter().position(|&count| count != 0) else {
            return ExecutionPlanError::Cycle {
                members: vec![],
                total: 0,
            };
        };
        let mut seen: Vec<Option<usize>> = vec![None; self.nodes.len()];
        let mut path: Vec<usize> = Vec::new();
        loop {
            if let Some(start) = seen[cursor] {
                let cycle = &path[start..];
                return ExecutionPlanError::Cycle {
                    members: cycle
                        .iter()
                        .take(16)
                        .map(|&index| self.nodes[index].id)
                        .collect(),
                    total: cycle.len(),
                };
            }
            seen[cursor] = Some(path.len());
            path.push(cursor);
            let predecessor = self.nodes[cursor]
                .inputs
                .iter()
                .filter_map(|port| producers.get(&port.value))
                .copied()
                .find(|&index| remaining[index] != 0);
            let Some(next) = predecessor else {
                return ExecutionPlanError::InvalidResidual {
                    node: self.nodes[cursor].id,
                };
            };
            cursor = next;
        }
    }
}
