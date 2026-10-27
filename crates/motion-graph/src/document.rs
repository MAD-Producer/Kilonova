use crate::{ColorBlendMode, Error, ErrorCode, Result};
use motion_effects::{AlphaMode, WorkingSpace};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

pub const GRAPH_FORMAT_VERSION: u32 = 1;
pub const INPUT_DEFAULTS_FORMAT_VERSION: u32 = 2;
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Float,
    Vec2,
    Vec3,
    Color,
    Bool,
    Image,
    DistancePixels,
}
impl ValueType {
    pub(crate) fn wgsl(self) -> &'static str {
        match self {
            Self::Float | Self::DistancePixels => "f32",
            Self::Vec2 => "vec2<f32>",
            Self::Vec3 => "vec3<f32>",
            Self::Color => "vec4<f32>",
            Self::Bool => "bool",
            Self::Image => "image",
        }
    }
    pub(crate) fn dimensions(self) -> usize {
        match self {
            Self::Vec2 => 2,
            Self::Vec3 => 3,
            Self::Color => 4,
            _ => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageSource {
    Input,
    Source,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleCoordinates {
    Pixels,
    Uv,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Abs,
    Sin,
    Cos,
    Sqrt,
    Floor,
    Fract,
    Clamp,
    Lerp,
    Smoothstep,
    Dot,
    Length,
    Normalize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorOp {
    Invert,
    Mix,
    Multiply,
    Screen,
    SrgbEncode,
    SrgbDecode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GradientMode {
    Linear,
    Radial,
    Conic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoiseMode {
    Value,
    Perlin,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeOp {
    Group {
        graph: Box<NodeGroup>,
    },
    GroupInput {
        ports: BTreeMap<String, GroupPort>,
    },
    GroupOutput {
        ports: BTreeMap<String, GroupPort>,
    },
    Time,
    FrameIndex,
    Resolution,
    Position,
    Uv,
    UvMapping,
    Image {
        source: ImageSource,
    },
    SampleImage {
        coordinates: SampleCoordinates,
    },
    Float {
        value: f32,
    },
    Vec2 {
        value: [f32; 2],
    },
    Vec3 {
        value: [f32; 3],
    },
    Color {
        value: [f32; 4],
    },
    Bool {
        value: bool,
    },
    Parameter {
        parameter: String,
    },
    Math {
        operation: MathOp,
        value_type: ValueType,
    },
    Splat {
        value_type: ValueType,
    },
    CombineVec2,
    CombineVec3,
    CombineColor,
    SplitColor,
    ColorMath {
        operation: ColorOp,
    },
    Gradient {
        mode: GradientMode,
    },
    Noise {
        mode: NoiseMode,
        octaves: u8,
    },
    Blend {
        mode: ColorBlendMode,
    },
    ApplyMask,
    Select {
        value_type: ValueType,
    },
    SdfCircle,
    SdfRoundedRectangle,
    SdfLine,
    SdfCapsule,
    SdfUnion,
    SdfIntersection,
    SdfDifference,
    SdfSmoothUnion,
    SdfFill,
    SdfStroke,
    ColorOutput,
    /// Known future/PC capabilities remain explicit and block N1 compilation.
    Unsupported {
        feature: String,
    },
}
impl NodeOp {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Group { .. } => "group",
            Self::GroupInput { .. } => "group_input",
            Self::GroupOutput { .. } => "group_output",
            Self::Time => "time",
            Self::FrameIndex => "frame_index",
            Self::Resolution => "resolution",
            Self::Position => "position",
            Self::Uv => "uv",
            Self::UvMapping => "uv_mapping",
            Self::Image { .. } => "image",
            Self::SampleImage { .. } => "sample_image",
            Self::Float { .. } => "float",
            Self::Vec2 { .. } => "vec2",
            Self::Vec3 { .. } => "vec3",
            Self::Color { .. } => "color",
            Self::Bool { .. } => "bool",
            Self::Parameter { .. } => "parameter",
            Self::Math { .. } => "math",
            Self::Splat { .. } => "splat",
            Self::CombineVec2 => "combine_vec2",
            Self::CombineVec3 => "combine_vec3",
            Self::CombineColor => "combine_color",
            Self::SplitColor => "split_color",
            Self::ColorMath { .. } => "color_math",
            Self::Gradient { .. } => "gradient",
            Self::Noise { .. } => "noise",
            Self::Blend { .. } => "blend",
            Self::ApplyMask => "apply_mask",
            Self::Select { .. } => "select",
            Self::SdfCircle => "sdf_circle",
            Self::SdfRoundedRectangle => "sdf_rounded_rectangle",
            Self::SdfLine => "sdf_line",
            Self::SdfCapsule => "sdf_capsule",
            Self::SdfUnion => "sdf_union",
            Self::SdfIntersection => "sdf_intersection",
            Self::SdfDifference => "sdf_difference",
            Self::SdfSmoothUnion => "sdf_smooth_union",
            Self::SdfFill => "sdf_fill",
            Self::SdfStroke => "sdf_stroke",
            Self::ColorOutput => "color_output",
            Self::Unsupported { .. } => "unsupported",
        }
    }
    pub fn title(&self) -> (&'static str, &'static str) {
        match self {
            Self::Group { .. } => ("节点组", "Node Group"),
            Self::GroupInput { .. } => ("组输入", "Group Input"),
            Self::GroupOutput { .. } => ("组输出", "Group Output"),
            Self::Time => ("时间", "Time"),
            Self::FrameIndex => ("帧（含小数）", "Frame Index"),
            Self::Resolution => ("图层尺寸", "Resolution"),
            Self::Position => ("位置（像素）", "Position (pixels)"),
            Self::Uv => ("UV", "UV"),
            Self::UvMapping => ("UV 映射", "UV Mapping"),
            Self::Image { .. } => ("图层图像", "Image"),
            Self::SampleImage { .. } => ("采样图像", "Sample Image"),
            Self::Float { .. } => ("浮点常量", "Float"),
            Self::Vec2 { .. } => ("二维向量", "Vector 2"),
            Self::Vec3 { .. } => ("三维向量", "Vector 3"),
            Self::Color { .. } => ("颜色常量", "Color"),
            Self::Bool { .. } => ("开关常量", "Boolean"),
            Self::Parameter { .. } => ("公开参数", "Public Parameter"),
            Self::Math { .. } => ("数学", "Math"),
            Self::Splat { .. } => ("标量扩展", "Splat"),
            Self::CombineVec2 => ("组合二维向量", "Combine Vector 2"),
            Self::CombineVec3 => ("组合三维向量", "Combine Vector 3"),
            Self::CombineColor => ("组合颜色", "Combine Color"),
            Self::SplitColor => ("拆分颜色", "Split Color"),
            Self::ColorMath { .. } => ("颜色运算", "Color Math"),
            Self::Gradient { .. } => ("渐变", "Gradient"),
            Self::Noise { .. } => ("噪声", "Noise"),
            Self::Blend { .. } => ("颜色合成", "Blend"),
            Self::ApplyMask => ("应用覆盖率", "Apply Mask"),
            Self::Select { .. } => ("选择", "Select"),
            Self::SdfCircle => ("SDF圆", "SDF Circle"),
            Self::SdfRoundedRectangle => ("SDF圆角矩形", "SDF Rounded Rectangle"),
            Self::SdfLine => ("SDF线段", "SDF Line"),
            Self::SdfCapsule => ("SDF胶囊", "SDF Capsule"),
            Self::SdfUnion => ("SDF并集", "SDF Union"),
            Self::SdfIntersection => ("SDF交集", "SDF Intersection"),
            Self::SdfDifference => ("SDF差集", "SDF Difference"),
            Self::SdfSmoothUnion => ("SDF平滑并集", "SDF Smooth Union"),
            Self::SdfFill => ("SDF填充", "SDF Fill"),
            Self::SdfStroke => ("SDF描边", "SDF Stroke"),
            Self::ColorOutput => ("颜色输出", "Color Output"),
            Self::Unsupported { .. } => ("尚不支持", "Unsupported"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    /// Optional author-facing title; never used as a node or socket identity.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    pub version: u32,
    pub operation: NodeOp,
    #[serde(default)]
    pub position: [f32; 2],
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub input_defaults: BTreeMap<String, InputValue>,
}
impl Node {
    pub fn new(id: impl Into<String>, operation: NodeOp) -> Self {
        Self {
            id: id.into(),
            display_name: String::new(),
            version: 1,
            operation,
            position: [0.0; 2],
            collapsed: false,
            input_defaults: BTreeMap::new(),
        }
    }
}

/// Stored fallback values. Connections always take precedence; values stay in
/// authoring space (straight RGBA for Color), just like constant nodes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum InputValue {
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Color([f32; 4]),
    Bool(bool),
    DistancePixels(f32),
}
impl InputValue {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Float(_) => ValueType::Float,
            Self::Vec2(_) => ValueType::Vec2,
            Self::Vec3(_) => ValueType::Vec3,
            Self::Color(_) => ValueType::Color,
            Self::Bool(_) => ValueType::Bool,
            Self::DistancePixels(_) => ValueType::DistancePixels,
        }
    }
    pub fn components(&self) -> &[f32] {
        match self {
            Self::Float(v) | Self::DistancePixels(v) => std::slice::from_ref(v),
            Self::Vec2(v) => v,
            Self::Vec3(v) => v,
            Self::Color(v) => v,
            Self::Bool(_) => &[],
        }
    }
    pub fn set_component(&mut self, component: usize, value: f32) -> bool {
        let values: &mut [f32] = match self {
            Self::Float(v) | Self::DistancePixels(v) => std::slice::from_mut(v),
            Self::Vec2(v) => v,
            Self::Vec3(v) => v,
            Self::Color(v) => v,
            Self::Bool(v) => {
                if component != 0 || !matches!(value, 0.0 | 1.0) {
                    return false;
                }
                *v = value == 1.0;
                return true;
            }
        };
        if let Some(slot) = values.get_mut(component) {
            *slot = value;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupPort {
    pub name: String,
    pub english_name: String,
    pub value_type: ValueType,
    pub semantic: crate::PortSemantic,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeGroup {
    pub name: String,
    pub english_name: String,
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
    pub input_node: String,
    pub output_node: String,
}
impl NodeGroup {
    pub fn ports(&self, output: bool) -> Result<&BTreeMap<String, GroupPort>> {
        let id = if output {
            &self.output_node
        } else {
            &self.input_node
        };
        match self
            .nodes
            .iter()
            .find(|node| node.id == *id)
            .map(|node| &node.operation)
        {
            Some(NodeOp::GroupInput { ports }) if !output => Ok(ports),
            Some(NodeOp::GroupOutput { ports }) if output => Ok(ports),
            _ => Err(Error::invalid(
                ErrorCode::UnknownNode,
                Some(id),
                None,
                "group boundary node is missing or has the wrong kind",
            )),
        }
    }
}
pub fn group_port<'a>(operation: &'a NodeOp, id: &str, output: bool) -> Option<&'a GroupPort> {
    match operation {
        NodeOp::Group { graph } => graph.ports(output).ok()?.get(id),
        NodeOp::GroupInput { ports } if output => ports.get(id),
        NodeOp::GroupOutput { ports } if !output => ports.get(id),
        _ => None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortRef {
    pub node: String,
    pub port: String,
}
impl PortRef {
    pub fn new(node: impl Into<String>, port: impl Into<String>) -> Self {
        Self {
            node: node.into(),
            port: port.into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub from: PortRef,
    pub to: PortRef,
}
impl Connection {
    pub fn new(from_node: &str, from_port: &str, to_node: &str, to_port: &str) -> Self {
        Self {
            from: PortRef::new(from_node, from_port),
            to: PortRef::new(to_node, to_port),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicParameter {
    pub id: String,
    pub name: String,
    pub english_name: String,
    pub value_type: ValueType,
    pub default: [f32; 4],
    pub min: f32,
    pub max: f32,
    pub step: f32,
    #[serde(default)]
    pub units: String,
    #[serde(default)]
    pub animatable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphDocument {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub english_name: String,
    pub working_space: WorkingSpace,
    pub alpha_mode: AlphaMode,
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
    pub public_parameters: Vec<PublicParameter>,
}
impl GraphDocument {
    pub fn minimum_format_version(&self) -> u32 {
        fn uses_defaults(nodes: &[Node]) -> bool {
            nodes.iter().any(|node| !node.input_defaults.is_empty()
                || matches!(&node.operation, NodeOp::Group { graph } if uses_defaults(&graph.nodes)))
        }
        if uses_defaults(&self.nodes) {
            INPUT_DEFAULTS_FORMAT_VERSION
        } else {
            GRAPH_FORMAT_VERSION
        }
    }
    pub fn from_json(text: &str) -> Result<Self> {
        if text.len() > MAX_DOCUMENT_BYTES {
            return Err(Error::invalid(
                ErrorCode::Budget,
                None,
                None,
                "graph document exceeds 4 MiB",
            ));
        }
        let value: serde_json::Value = serde_json::from_str(text)?;
        if let Some(version) = value.get("format_version").and_then(|v| v.as_u64()) {
            if !(GRAPH_FORMAT_VERSION as u64..=INPUT_DEFAULTS_FORMAT_VERSION as u64)
                .contains(&version)
            {
                return Err(Error::invalid(
                    ErrorCode::DocumentVersion,
                    None,
                    None,
                    "supported .msgraph versions are 1 and 2 (typed input defaults)",
                ));
            }
        }
        if let Some(nodes) = value.get("nodes").and_then(|v| v.as_array()) {
            for node in nodes {
                let id = node.get("id").and_then(|v| v.as_str());
                if let Some(kind) = node.pointer("/operation/kind").and_then(|v| v.as_str()) {
                    let known = [
                        "group",
                        "group_input",
                        "group_output",
                        "time",
                        "frame_index",
                        "resolution",
                        "position",
                        "uv",
                        "uv_mapping",
                        "image",
                        "sample_image",
                        "float",
                        "vec2",
                        "vec3",
                        "color",
                        "bool",
                        "parameter",
                        "math",
                        "splat",
                        "combine_vec2",
                        "combine_vec3",
                        "combine_color",
                        "split_color",
                        "color_math",
                        "gradient",
                        "noise",
                        "blend",
                        "apply_mask",
                        "select",
                        "sdf_circle",
                        "sdf_rounded_rectangle",
                        "sdf_line",
                        "sdf_capsule",
                        "sdf_union",
                        "sdf_intersection",
                        "sdf_difference",
                        "sdf_smooth_union",
                        "sdf_fill",
                        "sdf_stroke",
                        "color_output",
                        "unsupported",
                    ];
                    if !known.contains(&kind) {
                        let (code, reason) = unsupported_reason(kind);
                        return Err(Error::invalid(
                            code,
                            id,
                            Some("operation.kind"),
                            format!("{kind}: {reason}"),
                        ));
                    }
                }
            }
        }
        let document: Self = serde_json::from_value(value)?;
        if document.format_version < document.minimum_format_version() {
            return Err(Error::invalid(
                ErrorCode::DocumentVersion,
                None,
                None,
                "stored input defaults require .msgraph version 2",
            ));
        }
        Ok(document)
    }
    pub fn load(path: &Path) -> Result<Self> {
        if fs::metadata(path)?.len() > MAX_DOCUMENT_BYTES as u64 {
            return Err(Error::invalid(
                ErrorCode::Budget,
                None,
                None,
                "graph document exceeds 4 MiB",
            ));
        }
        Self::from_json(&fs::read_to_string(path)?)
    }
    pub fn to_json(&self) -> Result<String> {
        let text = serde_json::to_string_pretty(self)? + "\n";
        // Reject NaN/Infinity serialized as null before touching source files.
        Self::from_json(&text)?;
        Ok(text)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        crate::export::save_source(self, path)
    }
}
pub(crate) fn unsupported_reason(kind: &str) -> (ErrorCode, &'static str) {
    let reason = match kind {
        "world_position" | "normal" | "tangent" | "camera_distance" => {
            "SDK 6 image passes do not provide world-space fragment or mesh context"
        }
        "depth" | "depth_output" => {
            "scene depth input/output requires a host ABI that N1 does not implement"
        }
        "pbr" | "pbr_output" | "pbr_material_output" => {
            "material output requires the mesh/material renderer ABI"
        }
        "volume" | "ray_march" | "path_trace" | "sdf_3d" => {
            "PC-only/3D execution is outside the N1 portable image target"
        }
        _ => {
            return (
                ErrorCode::UnknownNode,
                "unknown node kind; nothing is silently removed",
            )
        }
    };
    (ErrorCode::UnsupportedNode, reason)
}
