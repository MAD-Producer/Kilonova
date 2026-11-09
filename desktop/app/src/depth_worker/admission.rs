use serde::Deserialize;
use std::collections::HashSet;

pub type Result<T> = std::result::Result<T, String>;
pub const MODEL: &str = "274368034c0fcef08f9b197d33b606bae9c578acca861ef72feb268143b21434";
pub const WEIGHTS: &str = "715fade13be8f229f8a70cc02066f656f2423a59effd0579197bbf57860e1378";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub model_sha256: String,
    pub original_weights_sha256: String,
    pub requested_backend: String,
    pub buffer_budget_bytes: u64,
    pub cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tensor {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<u32>,
    pub file: String,
    pub sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub origin: Origin,
    pub source_node: Option<String>,
    pub operation: Operation,
    pub inputs: Vec<Tensor>,
    pub expected: Tensor,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    SameModelIntermediate,
    AuthoredControl,
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Operation {
    Conv {
        attributes: crate::convolution_admission::Attributes,
    },
    ConvTranspose {
        attributes: crate::convolution_admission::Attributes,
    },
    Erf,
    MatMul,
    Softmax {
        axis: i32,
    },
    LayerNormalization {
        axis: i32,
        epsilon: f32,
    },
    Resize {
        mode: String,
        coordinate_transformation_mode: String,
    },
}
pub struct Plan {
    pub entry: &'static str,
    pub parameters: [u32; 16],
    pub dispatch: [u32; 3],
    pub output_bytes: u64,
    pub admitted_buffer_bytes: u64,
}
fn ensure(ok: bool, reason: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(reason.into()) }
}
fn product(shape: &[u32]) -> Result<u32> {
    shape.iter().try_fold(1u32, |value, &d| {
        ensure(d > 0, "zero tensor dimension unsupported")?;
        value
            .checked_mul(d)
            .ok_or_else(|| "tensor element count overflow".into())
    })
}
impl Tensor {
    pub fn bytes(&self) -> Result<u64> {
        ensure(
            self.dtype == "float32",
            "only explicit Float32 tensors are admitted",
        )?;
        ensure(
            self.shape.len() <= 4,
            "tensor rank exceeds research contract",
        )?;
        ensure(
            self.name.len() <= 256 && self.file.len() <= 240,
            "tensor identity/path too long",
        )?;
        ensure(
            self.sha256.len() == 64
                && self
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid tensor SHA-256",
        )?;
        Ok(u64::from(product(&self.shape)?) * 4)
    }
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.version == 1
                && self.model_sha256 == MODEL
                && self.original_weights_sha256 == WEIGHTS,
            "unsupported version or unqualified model identity",
        )?;
        ensure(
            matches!(self.requested_backend.as_str(), "vulkan" | "dx12" | "metal"),
            "explicit supported backend required; no fallback",
        )?;
        ensure(
            self.buffer_budget_bytes > 0 && self.buffer_budget_bytes <= 512 * 1024 * 1024,
            "buffer budget must be explicit and bounded to 512 MiB",
        )?;
        ensure(
            !self.cases.is_empty() && self.cases.len() <= 1024,
            "case count outside research contract",
        )?;
        let mut ids = HashSet::new();
        for case in &self.cases {
            ensure(
                !case.id.is_empty()
                    && case.id.len() <= 64
                    && case
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
                "invalid bounded case ID",
            )?;
            ensure(ids.insert(&case.id), "duplicate case ID")?;
            match case.origin {
                Origin::SameModelIntermediate => ensure(
                    case.source_node
                        .as_ref()
                        .is_some_and(|s| !s.is_empty() && s.len() <= 256),
                    "same-model case requires bounded original node identity",
                )?,
                Origin::AuthoredControl => ensure(
                    case.source_node.is_none(),
                    "authored controls must not claim a model node identity",
                )?,
            }
            ensure(
                !case.inputs.is_empty() && case.inputs.len() <= 3,
                "invalid input count",
            )?;
            for tensor in case.inputs.iter().chain([&case.expected]) {
                tensor.bytes()?;
            }
        }
        Ok(())
    }
}
impl Case {
    pub fn plan(&self, limits: &wgpu::Limits, budget: u64) -> Result<Plan> {
        ensure(
            !self.inputs.is_empty() && self.inputs.len() <= 3,
            "invalid input count",
        )?;
        for tensor in self.inputs.iter().chain([&self.expected]) {
            tensor.bytes()?;
        }
        let mut p = [0u32; 16];
        let input = &self.inputs[0].shape;
        let expected = &self.expected.shape;
        let max_groups = limits.max_compute_workgroups_per_dimension;
        ensure(
            max_groups > 0
                && limits.max_compute_invocations_per_workgroup >= 64
                && limits.max_compute_workgroup_size_x >= 64,
            "adapter lacks required compute workgroups",
        )?;
        let rows_dispatch = |count: u32, p: &mut [u32; 16]| -> Result<[u32; 3]> {
            let x = count.min(max_groups);
            let y = count.div_ceil(x);
            ensure(
                y <= max_groups,
                "two-dimensional workgroup count exceeds device limit",
            )?;
            p[13] = x;
            Ok([x, y, 1])
        };
        let (entry, dispatch) = match &self.operation {
            Operation::Conv { attributes } | Operation::ConvTranspose { attributes } => {
                ensure(
                    self.inputs.len() >= 2
                        && input.len() == 4
                        && self.inputs[1].shape.len() == 4
                        && expected.len() == 4,
                    "convolution requires rank-four image, weights and output",
                )?;
                let bias = if self.inputs.len() == 3 {
                    ensure(
                        self.inputs[2].shape.len() == 1,
                        "convolution bias must be a vector",
                    )?;
                    Some(self.inputs[2].shape[0])
                } else {
                    None
                };
                let kind = if matches!(&self.operation, Operation::Conv { .. }) {
                    crate::convolution_admission::Kind::CrossCorrelation
                } else {
                    crate::convolution_admission::Kind::TransposeNonoverlap
                };
                let conv = crate::convolution_admission::plan(
                    kind,
                    input
                        .as_slice()
                        .try_into()
                        .map_err(|_| "invalid image rank")?,
                    self.inputs[1]
                        .shape
                        .as_slice()
                        .try_into()
                        .map_err(|_| "invalid weight rank")?,
                    bias,
                    expected
                        .as_slice()
                        .try_into()
                        .map_err(|_| "invalid output rank")?,
                    attributes,
                    limits,
                    budget,
                )?;
                p = conv.parameters;
                (conv.entry, conv.dispatch)
            }
            Operation::Erf => {
                ensure(
                    self.inputs.len() == 1 && input == expected,
                    "Erf requires identical contiguous input/output shapes",
                )?;
                p[0] = product(expected)?;
                let dispatch = rows_dispatch(p[0].div_ceil(64), &mut p)?;
                p[1] = dispatch[0]
                    .checked_mul(64)
                    .ok_or("Erf dispatch stride overflow")?;
                ("erf_elementwise", dispatch)
            }
            Operation::MatMul => {
                ensure(
                    self.inputs.len() == 2 && input.len() >= 2,
                    "MatMul requires two rank>=2 inputs",
                )?;
                let right = &self.inputs[1].shape;
                ensure(right.len() >= 2, "MatMul right input rank below two")?;
                let (m, k, n) = (
                    input[input.len() - 2],
                    input[input.len() - 1],
                    right[right.len() - 1],
                );
                ensure(
                    right[right.len() - 2] == k,
                    "MatMul contraction dimensions differ",
                )?;
                let a_batch = &input[..input.len() - 2];
                let b_batch = &right[..right.len() - 2];
                let rank = a_batch.len().max(b_batch.len());
                let mut prefix = vec![1u32; rank];
                for (offset, dimension) in prefix.iter_mut().rev().enumerate() {
                    let a = a_batch.iter().rev().nth(offset).copied().unwrap_or(1);
                    let b = b_batch.iter().rev().nth(offset).copied().unwrap_or(1);
                    ensure(
                        a == b || a == 1 || b == 1,
                        "incompatible MatMul batch broadcasting",
                    )?;
                    *dimension = a.max(b);
                }
                let batches = product(&prefix)?;
                let a_count = product(a_batch)?;
                let b_count = product(b_batch)?;
                ensure(
                    (a_count == 1 || a_count == batches) && (b_count == 1 || b_count == batches),
                    "interleaved MatMul batch broadcasting not implemented",
                )?;
                let mut shape = prefix;
                shape.extend([m, n]);
                ensure(&shape == expected, "MatMul output shape mismatch")?;
                ensure(
                    limits.max_compute_workgroup_size_y >= 8
                        && limits.max_compute_workgroup_storage_size >= 512,
                    "adapter lacks matrix tile capacity",
                )?;
                p[..4].copy_from_slice(&[m, n, k, batches]);
                p[4] = if a_count == 1 {
                    0
                } else {
                    m.checked_mul(k).ok_or("MatMul stride overflow")?
                };
                p[5] = if b_count == 1 {
                    0
                } else {
                    k.checked_mul(n).ok_or("MatMul stride overflow")?
                };
                ("matmul", [n.div_ceil(8), m.div_ceil(8), batches])
            }
            Operation::Softmax { axis } | Operation::LayerNormalization { axis, .. } => {
                ensure(
                    *axis == -1 && !input.is_empty() && input == expected,
                    "normalization requires explicit final axis and identical nonempty shape",
                )?;
                let width = *input.last().ok_or("missing normalization width")?;
                let rows = product(&input[..input.len() - 1])?;
                p[0] = rows;
                p[1] = width;
                ensure(
                    limits.max_compute_workgroup_storage_size >= 256,
                    "adapter lacks reduction capacity",
                )?;
                let entry = match &self.operation {
                    Operation::Softmax { .. } => {
                        ensure(self.inputs.len() == 1, "Softmax input count mismatch")?;
                        "softmax"
                    }
                    Operation::LayerNormalization { epsilon, .. } => {
                        ensure(
                            self.inputs.len() == 3 && epsilon.is_finite() && *epsilon > 0.,
                            "LayerNormalization requires scale, bias and finite positive epsilon",
                        )?;
                        ensure(
                            self.inputs[1].shape == [width] && self.inputs[2].shape == [width],
                            "LayerNormalization scale/bias dimensions differ",
                        )?;
                        p[8] = epsilon.to_bits();
                        "layer_norm"
                    }
                    _ => return Err("internal normalization dispatch mismatch".into()),
                };
                (entry, rows_dispatch(rows, &mut p)?)
            }
            Operation::Resize {
                mode,
                coordinate_transformation_mode,
            } => {
                ensure(
                    self.inputs.len() == 1
                        && input.len() == 4
                        && expected.len() == 4
                        && input[..2] == expected[..2],
                    "Resize requires unchanged N/C and rank-four tensors",
                )?;
                ensure(
                    mode == "linear" && coordinate_transformation_mode == "align_corners",
                    "only explicit linear/align_corners Resize is implemented",
                )?;
                p[..4].copy_from_slice(input);
                p[4] = expected[2];
                p[5] = expected[3];
                let groups = product(expected)?.div_ceil(64);
                let dispatch = rows_dispatch(groups, &mut p)?;
                p[12] = dispatch[0]
                    .checked_mul(64)
                    .ok_or("Resize dispatch stride overflow")?;
                ("resize_align_corners", dispatch)
            }
        };
        ensure(
            dispatch.iter().all(|&v| v > 0 && v <= max_groups),
            "dispatch exceeds device limits",
        )?;
        let output_bytes = self.expected.bytes()?;
        ensure(
            output_bytes <= limits.max_storage_buffer_binding_size as u64
                && output_bytes <= limits.max_buffer_size,
            "output exceeds device buffer limits",
        )?;
        let mut bytes = output_bytes
            .checked_mul(2)
            .and_then(|b| b.checked_add(64))
            .ok_or("readback allocation overflow")?;
        for tensor in &self.inputs {
            let size = tensor.bytes()?;
            ensure(
                size <= limits.max_storage_buffer_binding_size as u64
                    && size <= limits.max_buffer_size,
                "input exceeds device buffer limits",
            )?;
            bytes = bytes
                .checked_add(size)
                .ok_or("input allocation total overflow")?;
        }
        bytes = bytes
            .checked_add((3 - self.inputs.len()) as u64 * 4)
            .ok_or("dummy allocation total overflow")?;
        ensure(bytes <= budget, "logical operator buffer budget exhausted")?;
        Ok(Plan {
            entry,
            parameters: p,
            dispatch,
            output_bytes,
            admitted_buffer_bytes: bytes,
        })
    }
}
