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
    Add,
    Mul,
    Div,
    Relu,
    Expand,
    Transpose {
        permutation: Vec<u32>,
    },
    GatherScalar {
        axis: i32,
        index: i64,
    },
    Slice {
        starts: Vec<i64>,
        ends: Vec<i64>,
        axes: Vec<i32>,
        steps: Vec<i64>,
    },
    Concat {
        axis: i32,
    },
}
pub struct Plan {
    pub entry: &'static str,
    pub parameters: [u32; 64],
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
            self.shape.len() <= 5,
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

include!("index_plan_impl.rs");
