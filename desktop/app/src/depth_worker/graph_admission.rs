//! Static blueprint -> validated research IR. No device or inference is run here.
//! This is deliberately separate from a product compute-plan capability.
use crate::admission::{MODEL, Result, WEIGHTS};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Blueprint {
    pub version: u32,
    pub capability: String,
    pub model_sha256: String,
    pub original_weights_sha256: String,
    pub input: Input,
    pub outputs: Vec<String>,
    pub tensors: HashMap<String, Tensor>,
    pub resources: HashMap<String, Resource>,
    pub aliases: HashMap<String, String>,
    pub owner_slots: HashMap<String, usize>,
    pub pool_slot_capacities: Vec<u64>,
    pub persistent_capacities: HashMap<String, u64>,
    pub capacity_budget_bytes: u64,
    pub declared_retained_buffer_capacity_bytes: u64,
    pub uniform_ring_bytes: u64,
    pub uniform_stride: u64,
    pub readback_bytes: u64,
    pub upload_staging_bytes: u64,
    pub dummy_binding_bytes: u64,
    pub ordering_contract: String,
    pub allocation_contract: String,
    pub operations: Vec<Node>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub name: String,
    pub shape: Vec<u32>,
    pub dtype: String,
    pub preprocessing: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tensor {
    pub dtype: String,
    pub shape: Vec<u32>,
    pub bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub shape: Vec<u32>,
    pub bytes: u64,
    pub sha256: String,
    pub file: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub index: u32,
    pub name: String,
    pub family: String,
    pub operation: Value,
    pub inputs: Vec<String>,
    pub input_owners: Vec<String>,
    pub output: String,
    pub owner: String,
}
pub enum Shader {
    Numerical,
    Convolution,
    Erf,
    Index,
}
pub struct CompiledNode {
    pub index: u32,
    pub name: String,
    pub entry: &'static str,
    pub shader: Shader,
    pub parameters: Vec<u32>,
    pub dispatch: [u32; 3],
    pub uniform_offset: u64,
    pub input_buffers: [usize; 3],
    pub input_bytes: [u64; 3],
    pub output_buffer: usize,
    pub output_bytes: u64,
}
pub struct CompiledGraph {
    pub nodes: Vec<CompiledNode>,
    pub persistent_names: Vec<String>,
    pub pool_buffer_base: usize,
    pub output_buffer: usize,
    pub retained_buffer_capacity_bytes: u64,
}
fn require(value: bool, cause: &str) -> Result<()> {
    if value { Ok(()) } else { Err(cause.into()) }
}
fn checked_sum(values: impl IntoIterator<Item = u64>) -> Result<u64> {
    values.into_iter().try_fold(0u64, |a, b| {
        a.checked_add(b)
            .ok_or_else(|| "graph capacity sum overflow".into())
    })
}
fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
impl Tensor {
    fn validate(&self, limits: &wgpu::Limits) -> Result<()> {
        require(
            self.dtype == "float32" && self.shape.len() <= 5,
            "graph tensor dtype/rank unsupported",
        )?;
        let elements = self.shape.iter().try_fold(1u64, |a, &d| {
            require(d > 0, "graph empty dimensions unsupported")?;
            a.checked_mul(u64::from(d))
                .ok_or_else(|| "graph element count overflow".to_owned())
        })?;
        require(
            elements <= i32::MAX as u64 && elements.checked_mul(4) == Some(self.bytes),
            "graph tensor byte/count mismatch",
        )?;
        require(
            self.bytes <= limits.max_storage_buffer_binding_size
                && self.bytes <= limits.max_buffer_size,
            "graph tensor exceeds device buffer limits",
        )
    }
}
impl Blueprint {
    fn canonical<'a>(&'a self, name: &'a str) -> &'a str {
        self.aliases.get(name).map(String::as_str).unwrap_or(name)
    }
    fn tensor(&self, name: &str) -> Result<&Tensor> {
        self.tensors
            .get(name)
            .ok_or_else(|| format!("missing graph tensor: {name}"))
    }
    pub fn compile_for_research(&self, limits: &wgpu::Limits) -> Result<CompiledGraph> {
        require(
            self.version == 1
                && self.capability == "static_program_only_not_executable"
                && self.model_sha256 == MODEL
                && self.original_weights_sha256 == WEIGHTS,
            "unsupported graph blueprint identity/version/capability",
        )?;
        require(
            self.input.name == "rgb"
                && self.input.dtype == "float32"
                && self.input.shape == [1, 3, 518, 518]
                && self.input.preprocessing.len() <= 256,
            "graph input/preprocessing boundary differs",
        )?;
        require(
            self.outputs == ["relative_inverse_depth"],
            "graph output identity differs",
        )?;
        require(
            !self.operations.is_empty()
                && self.operations.len() <= 1024
                && self.tensors.len() <= 2048
                && self.resources.len() <= 1024
                && self.aliases.len() <= 1024
                && !self.pool_slot_capacities.is_empty()
                && self.pool_slot_capacities.len() <= 1024,
            "graph count bounds exceeded",
        )?;
        require(
            self.ordering_contract
                == "Separate ordered compute passes in one submission; retain pool until completion before next-job reuse"
                && self.allocation_contract
                    == "Allocate every final slot capacity upfront; trace assigns owners without runtime resize",
            "unqualified graph ownership/ordering contract",
        )?;
        require(
            self.capacity_budget_bytes > 0
                && self.capacity_budget_bytes <= 512 << 20
                && self.uniform_stride == 256
                && limits.min_uniform_buffer_offset_alignment > 0
                && self.uniform_stride % u64::from(limits.min_uniform_buffer_offset_alignment) == 0
                && limits.max_uniform_buffer_binding_size >= 256,
            "graph budget/uniform alignment unsupported",
        )?;
        require(
            self.uniform_ring_bytes == self.operations.len() as u64 * self.uniform_stride
                && self.uniform_ring_bytes <= limits.max_buffer_size
                && self.upload_staging_bytes > 0
                && self.upload_staging_bytes <= 16 << 20
                && self.upload_staging_bytes <= limits.max_buffer_size
                && self.dummy_binding_bytes == 12,
            "graph parameter/staging capacity differs",
        )?;
        for (name, tensor) in &self.tensors {
            require(identity(name), "graph tensor name is unbounded")?;
            tensor.validate(limits)?;
        }
        require(
            self.tensor("rgb")?.shape == self.input.shape,
            "graph input metadata differs",
        )?;
        let output = &self.outputs[0];
        let final_tensor = self.tensor(output)?;
        require(
            final_tensor.shape == [1, 518, 518]
                && self.readback_bytes == final_tensor.bytes
                && self.readback_bytes <= limits.max_buffer_size,
            "graph final output/readback differs",
        )?;
        for (capacity, name) in self
            .persistent_capacities
            .values()
            .map(|&v| (v, "persistent"))
            .chain(self.pool_slot_capacities.iter().map(|v| (*v, "pool")))
        {
            require(
                capacity > 0
                    && capacity % 4 == 0
                    && capacity <= limits.max_buffer_size
                    && capacity <= limits.max_storage_buffer_binding_size,
                &format!("{name} graph buffer exceeds device limit"),
            )?;
        }
        require(
            self.resources.keys().collect::<HashSet<_>>()
                == self.persistent_capacities.keys().collect::<HashSet<_>>(),
            "persistent graph capacities/resources differ",
        )?;
        for (name, resource) in &self.resources {
            require(identity(name), "graph resource name is unbounded")?;
            let metadata = self.tensor(name)?;
            require(
                resource.bytes == metadata.bytes
                    && resource.shape == metadata.shape
                    && self.persistent_capacities[name] >= resource.bytes,
                "graph resource shape/capacity mismatch",
            )?;
            require(
                resource.sha256.len() == 64
                    && resource
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && resource.file == format!("weights/{}.f32", resource.sha256),
                "graph resource identity/path invalid",
            )?;
        }
        for (name, target) in &self.aliases {
            require(
                identity(name)
                    && identity(target)
                    && name != target
                    && !self.aliases.contains_key(target)
                    && !self.resources.contains_key(name)
                    && !self.owner_slots.contains_key(name),
                "graph alias is cyclic or physical",
            )?;
            require(
                self.tensor(name)?.bytes == self.tensor(target)?.bytes
                    && (self.resources.contains_key(target)
                        || self.owner_slots.contains_key(target)),
                "graph alias changes storage extent",
            )?;
        }
        require(
            self.owner_slots.keys().all(|name| {
                !self.resources.contains_key(name)
                    && self.owner_slots[name] < self.pool_slot_capacities.len()
                    && self.tensors.get(name).is_some_and(|t| {
                        t.bytes <= self.pool_slot_capacities[self.owner_slots[name]]
                    })
            }),
            "graph owner slot missing, immutable or undersized",
        )?;
        let total = checked_sum(
            self.persistent_capacities
                .values()
                .copied()
                .chain(self.pool_slot_capacities.iter().copied())
                .chain([
                    self.uniform_ring_bytes,
                    self.readback_bytes,
                    self.upload_staging_bytes,
                    self.dummy_binding_bytes,
                ]),
        )?;
        require(
            total == self.declared_retained_buffer_capacity_bytes
                && total <= self.capacity_budget_bytes,
            "graph retained capacity mismatch or exhausted budget",
        )?;
        let mut persistent_names: Vec<_> = self.resources.keys().cloned().collect();
        persistent_names.sort();
        let pool_base = persistent_names.len();
        let dummy_base = pool_base + self.pool_slot_capacities.len();
        let mut buffer_ids: HashMap<String, usize> = persistent_names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
        buffer_ids.extend(
            self.owner_slots
                .iter()
                .map(|(n, &i)| (n.clone(), pool_base + i)),
        );
        require(
            self.owner_slots.contains_key("rgb"),
            "graph input owner missing",
        )?;
        let mut available: HashSet<String> = self
            .resources
            .keys()
            .cloned()
            .chain(["rgb".into()])
            .collect();
        let mut created: HashMap<String, i64> = HashMap::from([("rgb".into(), -1)]);
        let mut last: HashMap<String, i64> = HashMap::from([("rgb".into(), -1)]);
        let mut compiled = Vec::new();
        let mut previous = None;
        for (ordinal, node) in self.operations.iter().enumerate() {
            require(
                node.index < 598 && previous.is_none_or(|p| node.index > p) && identity(&node.name),
                "graph operation order/name invalid",
            )?;
            previous = Some(node.index);
            require(
                !node.inputs.is_empty()
                    && node.inputs.len() <= 3
                    && node.inputs.len() == node.input_owners.len()
                    && identity(&node.output)
                    && identity(&node.owner)
                    && node.inputs.iter().all(|name| identity(name))
                    && node.input_owners.iter().all(|name| identity(name))
                    && node.output == node.owner
                    && self.canonical(&node.output) == node.output
                    && !available.contains(&node.output)
                    && self.owner_slots.contains_key(&node.owner),
                "graph operation output/input ownership invalid",
            )?;
            let mut inputs = [dummy_base, dummy_base + 1, dummy_base + 2];
            let mut input_bytes = [4u64; 3];
            for (i, name) in node.inputs.iter().enumerate() {
                let owner = self.canonical(name);
                require(
                    node.input_owners[i] == owner && available.contains(owner),
                    "graph input missing, stale or mis-aliased",
                )?;
                inputs[i] = *buffer_ids.get(owner).ok_or("graph read buffer missing")?;
                input_bytes[i] = self.tensor(name)?.bytes;
                if self.owner_slots.contains_key(owner) {
                    last.insert(owner.into(), i64::from(node.index));
                }
            }
            let target = *buffer_ids
                .get(&node.owner)
                .ok_or("graph output buffer missing")?;
            require(
                !inputs[..node.inputs.len()].contains(&target),
                "graph write aliases a current read",
            )?;
            // Case is used only to validate shapes and derive dispatch/uniforms.
            // No per-node CPU expected output exists or is read by this compiler.
            let metadata = |name: &str| -> Result<Value> {
                let t = self.tensor(name)?;
                Ok(json!({"name":name,"dtype":"float32","shape":t.shape,
                    "file":"unused-planning-metadata.f32","sha256":"0".repeat(64)}))
            };
            let descriptor = json!({"id":format!("node-{}",node.index),"origin":"same_model_intermediate","source_node":node.name,
                "operation":node.operation,"inputs":node.inputs.iter().map(|n|metadata(n)).collect::<Result<Vec<_>>>()?,
                "expected":metadata(&node.output)?});
            let (entry, shader, parameters, dispatch, output_bytes) = match node.family.as_str() {
                "numeric" => {
                    let case: crate::admission::Case =
                        serde_json::from_value(descriptor).map_err(|e| {
                            format!(
                                "numeric node #{} {} schema: {}",
                                node.index,
                                node.name,
                                e.to_string().chars().take(512).collect::<String>()
                            )
                        })?;
                    let p = case.plan(limits, self.capacity_budget_bytes).map_err(|e| {
                        format!("numeric node #{} {} admission: {e}", node.index, node.name)
                    })?;
                    let shader = match p.entry {
                        "conv_nchw" | "conv_transpose_no_overlap_nchw" => Shader::Convolution,
                        "erf_elementwise" => Shader::Erf,
                        _ => Shader::Numerical,
                    };
                    (
                        p.entry,
                        shader,
                        p.parameters.to_vec(),
                        p.dispatch,
                        p.output_bytes,
                    )
                }
                "index" => {
                    let case: crate::index_admission::Case = serde_json::from_value(descriptor)
                        .map_err(|e| {
                            format!(
                                "index node #{} {} schema: {}",
                                node.index,
                                node.name,
                                e.to_string().chars().take(512).collect::<String>()
                            )
                        })?;
                    let p = case.plan(limits, self.capacity_budget_bytes).map_err(|e| {
                        format!("index node #{} {} admission: {e}", node.index, node.name)
                    })?;
                    (
                        p.entry,
                        Shader::Index,
                        p.parameters.to_vec(),
                        p.dispatch,
                        p.output_bytes,
                    )
                }
                _ => return Err("unimplemented graph operation family".into()),
            };
            require(
                output_bytes == self.tensor(&node.output)?.bytes,
                "graph planned output storage differs",
            )?;
            created.insert(node.owner.clone(), i64::from(node.index));
            last.entry(node.owner.clone())
                .or_insert(i64::from(node.index));
            available.insert(node.owner.clone());
            compiled.push(CompiledNode {
                index: node.index,
                name: node.name.clone(),
                entry,
                shader,
                parameters,
                dispatch,
                uniform_offset: ordinal as u64 * self.uniform_stride,
                input_buffers: inputs,
                input_bytes,
                output_buffer: target,
                output_bytes,
            });
        }
        let output_owner = self.canonical(output);
        require(
            available.contains(output_owner),
            "graph final output has no producer",
        )?;
        if self.owner_slots.contains_key(output_owner) {
            last.insert(output_owner.into(), 598);
        }
        require(
            created.keys().collect::<HashSet<_>>()
                == self.owner_slots.keys().collect::<HashSet<_>>(),
            "unproduced or extra graph owner slots",
        )?;
        // Independent of the Python planner: inclusive intervals forbid a future
        // alias read from sharing an earlier write's physical pool buffer.
        let mut by_slot: HashMap<usize, Vec<(&str, i64, i64)>> = HashMap::new();
        for (name, &slot) in &self.owner_slots {
            by_slot
                .entry(slot)
                .or_default()
                .push((name, created[name], last[name]));
        }
        for intervals in by_slot.values_mut() {
            intervals.sort_by_key(|row| row.1);
            require(
                intervals.windows(2).all(|pair| pair[0].2 < pair[1].1),
                "overlapping graph owner lifetimes share a buffer",
            )?;
        }
        Ok(CompiledGraph {
            nodes: compiled,
            persistent_names,
            pool_buffer_base: pool_base,
            output_buffer: buffer_ids[output_owner],
            retained_buffer_capacity_bytes: total,
        })
    }
}
