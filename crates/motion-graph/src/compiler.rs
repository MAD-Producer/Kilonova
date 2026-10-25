use crate::{document::unsupported_reason, *};
use motion_effects::{shader, AlphaMode, ParamDefinition, ParamKind, PluginManifest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const HARD_MAX_NODES: usize = 1024;
const NUMBER_LIMIT: f32 = 1.0e12;
const DIV_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompileOptions {
    pub plugin_id: String,
    pub version: String,
    pub effect_id: Option<String>,
    pub author: String,
    pub license: String,
    pub target_sdk: u32,
    pub max_nodes: usize,
    pub max_connections: usize,
    pub max_shader_bytes: usize,
    pub max_loop_work: u64,
    /// An author-selected static sampling budget, not a hardware texture limit.
    pub max_samples_per_fragment: u64,
}
impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            plugin_id: "local.motionstudio.graph".into(),
            version: "0.1.0".into(),
            effect_id: None,
            author: String::new(),
            license: "MIT".into(),
            target_sdk: 6,
            max_nodes: 512,
            max_connections: 4096,
            max_shader_bytes: 256 * 1024,
            max_loop_work: 65536,
            max_samples_per_fragment: 1025,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct NodePorts {
    pub inputs: BTreeMap<String, ValueType>,
    pub outputs: BTreeMap<String, ValueType>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ValidationReport {
    pub topological_order: Vec<String>,
    pub output_node: String,
    pub reachable_nodes: Vec<String>,
    pub unused_nodes: Vec<String>,
    pub parameter_slots: BTreeMap<String, usize>,
}
#[derive(Clone, Debug, Serialize)]
pub struct CompileReport {
    pub target_sdk: u32,
    pub nodes: usize,
    pub reachable_nodes: usize,
    pub unused_nodes: Vec<String>,
    pub passes: usize,
    pub loop_work: u64,
    pub graph_sample_sites: u64,
    /// SDK image opacity blending calls sample_source once per fragment.
    pub host_sample_sites: u64,
    pub total_sample_sites: u64,
    pub reflected_texture_bindings: usize,
    pub package_resource_textures: usize,
    pub package_resource_bytes: u64,
    pub shader_bytes: usize,
    /// Actual portable compiler calls for this request (zero on an exact hit).
    pub portable_compilations: u64,
    /// UTF-8 code bytes retained by the owning editable compiler, excluding
    /// returned graphs, container overhead and native driver allocations.
    pub compiler_cache_code_bytes: u64,
    pub emitted_values: usize,
    pub common_values_reused: usize,
    pub folded_math_nodes: usize,
    pub parameter_slots: BTreeMap<String, usize>,
    /// Size of one RGBA8 pass output only; excludes host conversions, masks and input.
    pub pass_output_rgba8_bytes_4k: u64,
}
pub struct CompiledGraph {
    pub source: String,
    pub shader: shader::CompiledShader,
    pub manifest: PluginManifest,
    pub report: CompileReport,
    pub(crate) resources: BTreeMap<String, Vec<u8>>,
}

pub fn node_ports(node: &Node, parameters: &[PublicParameter]) -> Result<NodePorts> {
    if !node.input_defaults.is_empty()
        && matches!(
            node.operation,
            NodeOp::Group { .. } | NodeOp::GroupInput { .. } | NodeOp::GroupOutput { .. }
        )
    {
        return Err(Error::invalid(
            ErrorCode::InvalidValue,
            Some(&node.id),
            None,
            "group boundary defaults must be authored on the internal value inputs",
        ));
    }
    let mut p = NodePorts {
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
    };
    if node.display_name.len() > 120
        || node.display_name.chars().any(char::is_control)
        || (!node.display_name.is_empty() && node.display_name.trim().is_empty())
    {
        return Err(Error::invalid(
            ErrorCode::InvalidValue,
            Some(&node.id),
            Some("display_name"),
            "node display names must be visible text of at most 120 UTF-8 bytes",
        ));
    }
    let group_ports = |ports: &BTreeMap<String, GroupPort>| -> Result<BTreeMap<String, ValueType>> {
        for (id, port) in ports {
            use crate::PortSemantic::*;
            let semantic_valid = match port.value_type {
                ValueType::Float => matches!(port.semantic, Scalar | Seconds | Frame | SizePixels),
                ValueType::Vec2 | ValueType::Vec3 => {
                    matches!(port.semantic, Vector | PositionPixels | Uv | SizePixels)
                }
                ValueType::Color => port.semantic == Color,
                ValueType::Bool => port.semantic == Bool,
                ValueType::Image => port.semantic == Image,
                ValueType::DistancePixels => port.semantic == DistancePixels,
            };
            if !semantic_valid {
                return Err(Error::invalid(
                    ErrorCode::TypeMismatch,
                    Some(&node.id),
                    Some(id),
                    "group port storage shape does not match its semantic units",
                ));
            }
            if !motion_effects::valid_id(id)
                || port.name.is_empty()
                || port.english_name.is_empty()
                || port.name.len() > 120
                || port.english_name.len() > 120
            {
                return Err(Error::invalid(
                    ErrorCode::InvalidValue,
                    Some(&node.id),
                    Some(id),
                    "invalid group port identity or bilingual name",
                ));
            }
        }
        Ok(ports
            .iter()
            .map(|(id, port)| (id.clone(), port.value_type))
            .collect())
    };
    let (inputs, outputs): (Vec<(&str, ValueType)>, Vec<(&str, ValueType)>) = match &node.operation
    {
        NodeOp::Group { graph } => {
            p.inputs = group_ports(graph.ports(false)?)?;
            p.outputs = group_ports(graph.ports(true)?)?;
            return Ok(p);
        }
        NodeOp::GroupInput { ports } => {
            p.outputs = group_ports(ports)?;
            return Ok(p);
        }
        NodeOp::GroupOutput { ports } => {
            p.inputs = group_ports(ports)?;
            return Ok(p);
        }
        NodeOp::Time | NodeOp::FrameIndex | NodeOp::Float { .. } => {
            (vec![], vec![("value", ValueType::Float)])
        }
        NodeOp::Resolution | NodeOp::Position | NodeOp::Uv | NodeOp::Vec2 { .. } => {
            (vec![], vec![("value", ValueType::Vec2)])
        }
        NodeOp::Vec3 { .. } => (vec![], vec![("value", ValueType::Vec3)]),
        NodeOp::Color { .. } => (vec![], vec![("value", ValueType::Color)]),
        NodeOp::Bool { .. } => (vec![], vec![("value", ValueType::Bool)]),
        NodeOp::Image { .. } => (vec![], vec![("image", ValueType::Image)]),
        NodeOp::SampleImage { .. } => (
            vec![("image", ValueType::Image), ("coordinate", ValueType::Vec2)],
            vec![("color", ValueType::Color)],
        ),
        NodeOp::Parameter { parameter } => {
            let ty = parameters
                .iter()
                .find(|p| p.id == *parameter)
                .ok_or_else(|| {
                    Error::invalid(
                        ErrorCode::UnknownParameter,
                        Some(&node.id),
                        Some("parameter"),
                        format!("public parameter {parameter} is missing"),
                    )
                })?
                .value_type;
            (vec![], vec![("value", ty)])
        }
        NodeOp::Math {
            operation,
            value_type,
        } => {
            let ty = *value_type;
            if !matches!(ty, ValueType::Float | ValueType::Vec2 | ValueType::Vec3) {
                return Err(Error::invalid(
                    ErrorCode::TypeMismatch,
                    Some(&node.id),
                    Some("value_type"),
                    "math accepts float, vec2 or vec3; conversions must be explicit",
                ));
            }
            use MathOp::*;
            let input = match operation {
                Abs | Sin | Cos | Sqrt | Floor | Fract | Length | Normalize => vec![("value", ty)],
                Clamp => vec![("value", ty), ("min", ty), ("max", ty)],
                Lerp => vec![("a", ty), ("b", ty), ("factor", ValueType::Float)],
                Smoothstep => vec![("edge0", ty), ("edge1", ty), ("value", ty)],
                _ => vec![("a", ty), ("b", ty)],
            };
            if matches!(operation, Dot | Length | Normalize) && ty == ValueType::Float {
                return Err(Error::invalid(
                    ErrorCode::TypeMismatch,
                    Some(&node.id),
                    None,
                    "dot, length and normalize require vec2 or vec3",
                ));
            }
            (
                input,
                vec![(
                    "value",
                    if matches!(operation, Dot | Length) {
                        ValueType::Float
                    } else {
                        ty
                    },
                )],
            )
        }
        NodeOp::Splat { value_type } => {
            if !matches!(value_type, ValueType::Vec2 | ValueType::Vec3) {
                return Err(Error::invalid(
                    ErrorCode::TypeMismatch,
                    Some(&node.id),
                    None,
                    "splat explicitly converts a float to vec2 or vec3",
                ));
            }
            (
                vec![("value", ValueType::Float)],
                vec![("value", *value_type)],
            )
        }
        NodeOp::CombineVec2 => (
            vec![("x", ValueType::Float), ("y", ValueType::Float)],
            vec![("value", ValueType::Vec2)],
        ),
        NodeOp::CombineVec3 => (
            vec![
                ("x", ValueType::Float),
                ("y", ValueType::Float),
                ("z", ValueType::Float),
            ],
            vec![("value", ValueType::Vec3)],
        ),
        NodeOp::CombineColor => (
            vec![
                ("r", ValueType::Float),
                ("g", ValueType::Float),
                ("b", ValueType::Float),
                ("a", ValueType::Float),
            ],
            vec![("color", ValueType::Color)],
        ),
        NodeOp::SplitColor => (
            vec![("color", ValueType::Color)],
            vec![
                ("r", ValueType::Float),
                ("g", ValueType::Float),
                ("b", ValueType::Float),
                ("a", ValueType::Float),
            ],
        ),
        NodeOp::ColorMath { operation } => {
            let inputs = match operation {
                ColorOp::Mix => vec![
                    ("a", ValueType::Color),
                    ("b", ValueType::Color),
                    ("factor", ValueType::Float),
                ],
                ColorOp::Multiply | ColorOp::Screen => {
                    vec![("a", ValueType::Color), ("b", ValueType::Color)]
                }
                _ => vec![("color", ValueType::Color)],
            };
            (inputs, vec![("color", ValueType::Color)])
        }
        NodeOp::Blend { .. } => (
            vec![
                ("backdrop", ValueType::Color),
                ("source", ValueType::Color),
                ("factor", ValueType::Float),
            ],
            vec![("color", ValueType::Color)],
        ),
        NodeOp::Noise { octaves, .. } => {
            if !(1..=4).contains(octaves) {
                return Err(Error::invalid(
                    ErrorCode::InvalidValue,
                    Some(&node.id),
                    None,
                    "noise octaves must be in 1..=4",
                ));
            }
            (
                vec![
                    ("coordinate", ValueType::Vec2),
                    ("offset", ValueType::Vec2),
                    ("scale", ValueType::Float),
                    ("seed", ValueType::Float),
                ],
                vec![("factor", ValueType::Float), ("color", ValueType::Color)],
            )
        }
        NodeOp::Gradient { .. } => (
            vec![
                ("coordinate", ValueType::Vec2),
                ("start", ValueType::Vec2),
                ("end", ValueType::Vec2),
                ("falloff", ValueType::Float),
                ("a", ValueType::Color),
                ("b", ValueType::Color),
            ],
            vec![("factor", ValueType::Float), ("color", ValueType::Color)],
        ),
        NodeOp::ApplyMask => (
            vec![("color", ValueType::Color), ("coverage", ValueType::Float)],
            vec![("color", ValueType::Color)],
        ),
        NodeOp::Select { value_type } => {
            if *value_type == ValueType::Image {
                return Err(Error::invalid(
                    ErrorCode::UnsupportedNode,
                    Some(&node.id),
                    None,
                    "image selection requires a sampling/pass plan outside N1",
                ));
            }
            (
                vec![
                    ("condition", ValueType::Bool),
                    ("true_value", *value_type),
                    ("false_value", *value_type),
                ],
                vec![("value", *value_type)],
            )
        }
        NodeOp::SdfCircle => (
            vec![
                ("position", ValueType::Vec2),
                ("center", ValueType::Vec2),
                ("radius", ValueType::Float),
            ],
            vec![("distance", ValueType::DistancePixels)],
        ),
        NodeOp::SdfRoundedRectangle => (
            vec![
                ("position", ValueType::Vec2),
                ("center", ValueType::Vec2),
                ("half_size", ValueType::Vec2),
                ("radius", ValueType::Float),
            ],
            vec![("distance", ValueType::DistancePixels)],
        ),
        NodeOp::SdfLine | NodeOp::SdfCapsule => (
            vec![
                ("position", ValueType::Vec2),
                ("a", ValueType::Vec2),
                ("b", ValueType::Vec2),
                (
                    if matches!(node.operation, NodeOp::SdfLine) {
                        "width"
                    } else {
                        "radius"
                    },
                    ValueType::Float,
                ),
            ],
            vec![("distance", ValueType::DistancePixels)],
        ),
        NodeOp::SdfUnion | NodeOp::SdfIntersection | NodeOp::SdfDifference => (
            vec![
                ("a", ValueType::DistancePixels),
                ("b", ValueType::DistancePixels),
            ],
            vec![("distance", ValueType::DistancePixels)],
        ),
        NodeOp::SdfSmoothUnion => (
            vec![
                ("a", ValueType::DistancePixels),
                ("b", ValueType::DistancePixels),
                ("smoothness", ValueType::Float),
            ],
            vec![("distance", ValueType::DistancePixels)],
        ),
        NodeOp::SdfFill => (
            vec![("distance", ValueType::DistancePixels)],
            vec![("coverage", ValueType::Float)],
        ),
        NodeOp::SdfStroke => (
            vec![
                ("distance", ValueType::DistancePixels),
                ("width", ValueType::Float),
            ],
            vec![("coverage", ValueType::Float)],
        ),
        NodeOp::ColorOutput => (vec![("color", ValueType::Color)], vec![]),
        NodeOp::Unsupported { feature } => {
            let (_, reason) = unsupported_reason(feature);
            return Err(Error::invalid(
                ErrorCode::UnsupportedNode,
                Some(&node.id),
                None,
                format!("{feature}: {reason}; N1 has no implementation"),
            ));
        }
    };
    p.inputs
        .extend(inputs.into_iter().map(|(n, t)| (n.into(), t)));
    p.outputs
        .extend(outputs.into_iter().map(|(n, t)| (n.into(), t)));
    for (port, value) in &node.input_defaults {
        let ty = p.inputs.get(port).ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownPort,
                Some(&node.id),
                Some(port),
                "default refers to an unknown input",
            )
        })?;
        if *ty != value.value_type() {
            return Err(Error::invalid(
                ErrorCode::TypeMismatch,
                Some(&node.id),
                Some(port),
                "input default must have the input's exact storage type",
            ));
        }
        if value
            .components()
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1.0e6)
            || matches!(value, InputValue::Color(rgba) if rgba.iter().any(|v| !(0.0..=1.0).contains(v)))
        {
            return Err(Error::invalid(
                ErrorCode::InvalidValue,
                Some(&node.id),
                Some(port),
                "input defaults must be finite (±1e6); straight RGBA must be normalized",
            ));
        }
    }
    Ok(p)
}

/// Validate every node, edge and cycle, then require complete inputs only in the
/// color output's dependency chain. Supported, structurally valid unused nodes
/// may remain unfinished; they are retained in source and omitted from execution.
pub fn validate(document: &GraphDocument, options: &CompileOptions) -> Result<ValidationReport> {
    if crate::groups::structured(document) {
        let flat = crate::groups::flatten(document, options)?;
        validate_root(&flat.document, options, None).map_err(|error| flat.describe(error))
    } else {
        validate_root(document, options, None)
    }
}

fn validate_root(
    document: &GraphDocument,
    options: &CompileOptions,
    root: Option<&PortRef>,
) -> Result<ValidationReport> {
    if !(GRAPH_FORMAT_VERSION..=INPUT_DEFAULTS_FORMAT_VERSION).contains(&document.format_version)
        || document.format_version < document.minimum_format_version()
    {
        return Err(Error::invalid(
            ErrorCode::DocumentVersion,
            None,
            None,
            "supported .msgraph versions are 1 and 2; input defaults require version 2",
        ));
    }
    if options.target_sdk != 6 {
        return Err(Error::invalid(
            ErrorCode::UnsupportedNode,
            None,
            None,
            "N1 target is SDK 6 portable image effects",
        ));
    }
    if options.max_nodes == 0
        || options.max_nodes > HARD_MAX_NODES
        || options.max_connections > 8192
        || options.max_shader_bytes > 256 * 1024
    {
        return Err(Error::invalid(
            ErrorCode::Budget,
            None,
            None,
            "compile limits exceed N1 document/SDK budgets",
        ));
    }
    if document.nodes.len() > options.max_nodes
        || document.connections.len() > options.max_connections
        || document.public_parameters.len() > motion_effects::MAX_PARAMS
    {
        return Err(Error::invalid(
            ErrorCode::Budget,
            None,
            None,
            "graph exceeds its node, connection or 32 public-parameter budget",
        ));
    }
    if !motion_effects::valid_id(&document.id)
        || document.id.len() > 96
        || document.name.is_empty()
        || document.english_name.is_empty()
        || document.name.len() > 120
        || document.english_name.len() > 120
    {
        return Err(Error::invalid(
            ErrorCode::InvalidValue,
            None,
            None,
            "invalid graph identity or bilingual name (maximum 120 UTF-8 bytes each)",
        ));
    }
    let mut parameter_slots = BTreeMap::new();
    for (index, p) in document.public_parameters.iter().enumerate() {
        if !motion_effects::valid_id(&p.id) || parameter_slots.insert(p.id.clone(), index).is_some()
        {
            return Err(Error::invalid(
                ErrorCode::DuplicateParameter,
                None,
                Some(&p.id),
                "public parameter IDs must be valid and unique",
            ));
        }
        let kind = param_kind(p.value_type).ok_or_else(|| {
            Error::invalid(
                ErrorCode::TypeMismatch,
                None,
                Some(&p.id),
                "public parameters support float, vec2, vec3, color and bool",
            )
        })?;
        if p.name.is_empty()
            || p.english_name.is_empty()
            || p.name.len() > 120
            || p.english_name.len() > 120
            || !p.min.is_finite()
            || !p.max.is_finite()
            || p.min > p.max
            || p.min.abs() > 1.0e6
            || p.max.abs() > 1.0e6
            || !p.step.is_finite()
            || p.step <= 0.0
            || !kind.valid_value(&p.default, p.min, p.max)
        {
            return Err(Error::invalid(
                ErrorCode::InvalidValue,
                None,
                Some(&p.id),
                "invalid public parameter metadata, finite range or default (numeric range ±1e6)",
            ));
        }
        if kind == ParamKind::Bool && (p.min != 0.0 || p.max != 1.0)
            || kind == ParamKind::Color && (p.min < 0.0 || p.max > 1.0)
        {
            return Err(Error::invalid(
                ErrorCode::InvalidValue,
                None,
                Some(&p.id),
                "boolean/color parameters require normalized 0..1 ranges",
            ));
        }
    }
    let mut nodes = BTreeMap::new();
    let mut ports = BTreeMap::new();
    let mut port_specs = BTreeMap::new();
    let mut outputs = vec![];
    for node in &document.nodes {
        if !motion_effects::valid_id(&node.id) || nodes.insert(node.id.clone(), node).is_some() {
            return Err(Error::invalid(
                ErrorCode::DuplicateNode,
                Some(&node.id),
                None,
                "node IDs must be valid and unique",
            ));
        }
        if node.version != 1 {
            return Err(Error::invalid(
                ErrorCode::NodeVersion,
                Some(&node.id),
                None,
                "only node version 1 is implemented",
            ));
        }
        if !node.position.iter().all(|v| v.is_finite()) {
            return Err(Error::invalid(
                ErrorCode::InvalidValue,
                Some(&node.id),
                Some("position"),
                "node layout must be finite",
            ));
        }
        let literal: &[f32] = match &node.operation {
            NodeOp::Float { value } => std::slice::from_ref(value),
            NodeOp::Vec2 { value } => value,
            NodeOp::Vec3 { value } => value,
            NodeOp::Color { value } => value,
            _ => &[],
        };
        if literal.iter().any(|v| !v.is_finite() || v.abs() > 1.0e6)
            || matches!(&node.operation,NodeOp::Color{value} if value.iter().any(|v|!(0.0..=1.0).contains(v)))
        {
            return Err(Error::invalid(
                ErrorCode::InvalidValue,
                Some(&node.id),
                Some("value"),
                "constants must be finite (±1e6); RGBA colors must be normalized",
            ));
        }
        ports.insert(
            node.id.clone(),
            node_ports(node, &document.public_parameters)?,
        );
        port_specs.insert(
            node.id.clone(),
            node_port_specs(node, &document.public_parameters)?,
        );
        if matches!(node.operation, NodeOp::ColorOutput) {
            outputs.push(node.id.clone());
        }
    }
    if let Some(root) = root {
        let ports = ports.get(&root.node).ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownNode,
                Some(&root.node),
                Some(&root.port),
                "preview node is missing",
            )
        })?;
        if !ports.outputs.contains_key(&root.port) {
            return Err(Error::invalid(
                ErrorCode::UnknownPort,
                Some(&root.node),
                Some(&root.port),
                "preview output is missing",
            ));
        }
        outputs = vec![root.node.clone()];
    }
    if outputs.is_empty() {
        return Err(Error::invalid(
            ErrorCode::MissingOutput,
            None,
            None,
            "graph requires exactly one color_output node",
        ));
    }
    if outputs.len() != 1 {
        return Err(Error::invalid(
            ErrorCode::MultipleOutputs,
            None,
            None,
            "graph contains multiple color_output nodes",
        ));
    }
    let mut incoming = BTreeMap::new();
    let mut degrees: BTreeMap<String, usize> = nodes.keys().map(|id| (id.clone(), 0)).collect();
    let mut outgoing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in &document.connections {
        let from = ports.get(&edge.from.node).ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownNode,
                Some(&edge.from.node),
                Some(&edge.from.port),
                "connection source node is missing",
            )
        })?;
        let to = ports.get(&edge.to.node).ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownNode,
                Some(&edge.to.node),
                Some(&edge.to.port),
                "connection destination node is missing",
            )
        })?;
        let source = from.outputs.get(&edge.from.port).ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownPort,
                Some(&edge.from.node),
                Some(&edge.from.port),
                "unknown output port",
            )
        })?;
        let target = to.inputs.get(&edge.to.port).ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownPort,
                Some(&edge.to.node),
                Some(&edge.to.port),
                "unknown input port",
            )
        })?;
        if source != target {
            return Err(Error::invalid(
                ErrorCode::TypeMismatch,
                Some(&edge.to.node),
                Some(&edge.to.port),
                format!(
                    "{source:?} from {}.{} cannot connect to {target:?}; no implicit conversion",
                    edge.from.node, edge.from.port
                ),
            ));
        }
        crate::ports::check_specs(
            &edge.from,
            &edge.to,
            &port_specs[&edge.from.node].outputs[&edge.from.port],
            &port_specs[&edge.to.node].inputs[&edge.to.port],
        )?;
        if incoming
            .insert(edge.to.clone(), edge.from.clone())
            .is_some()
        {
            return Err(Error::invalid(
                ErrorCode::DuplicateInput,
                Some(&edge.to.node),
                Some(&edge.to.port),
                "an input may have only one connection",
            ));
        }
        *degrees.get_mut(&edge.to.node).unwrap() += 1;
        outgoing
            .entry(edge.from.node.clone())
            .or_default()
            .push(edge.to.node.clone());
    }
    let mut ready: BTreeSet<String> = degrees
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut order = vec![];
    while let Some(id) = ready.pop_first() {
        order.push(id.clone());
        for target in outgoing.get(&id).into_iter().flatten() {
            let degree = degrees.get_mut(target).unwrap();
            *degree -= 1;
            if *degree == 0 {
                ready.insert(target.clone());
            }
        }
    }
    if order.len() != nodes.len() {
        let unresolved: Vec<_> = degrees
            .iter()
            .filter(|(_, d)| **d > 0)
            .map(|(id, _)| id.as_str())
            .collect();
        return Err(Error::invalid(
            ErrorCode::Cycle,
            unresolved.first().copied(),
            None,
            format!(
                "dependency cycle; unresolved nodes: {}",
                unresolved.join(", ")
            ),
        ));
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![outputs[0].clone()];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id.clone()) {
            continue;
        }
        for port in ports[&id].inputs.keys() {
            // Incomplete nodes outside this output's dependency chain are drafts.
            // Structural, value, edge and cycle validation above still covers them.
            if let Some(source) = incoming.get(&PortRef::new(&id, port)) {
                pending.push(source.node.clone());
            } else if !nodes[&id].input_defaults.contains_key(port) {
                return Err(Error::invalid(
                    ErrorCode::MissingInput,
                    Some(&id),
                    Some(port),
                    "required input is not connected",
                ));
            }
        }
    }
    Ok(ValidationReport {
        topological_order: order.clone(),
        output_node: outputs.remove(0),
        reachable_nodes: order
            .iter()
            .filter(|id| reachable.contains(*id))
            .cloned()
            .collect(),
        unused_nodes: order
            .into_iter()
            .filter(|id| !reachable.contains(id))
            .collect(),
        parameter_slots,
    })
}

pub fn compile(document: &GraphDocument, options: &CompileOptions) -> Result<CompiledGraph> {
    compile_mode(document, options, false, None)
}

/// Editable runtime path. Numeric literals, input defaults and CPU-folded
/// results are private data; the ordinary compile API remains a baked oracle.
pub fn compile_editable(
    document: &GraphDocument,
    options: &CompileOptions,
) -> Result<CompiledGraph> {
    compile_mode(document, options, true, None)
}

pub(crate) fn compile_mode(
    document: &GraphDocument,
    options: &CompileOptions,
    editable: bool,
    cache: Option<&mut crate::EditableCompiler>,
) -> Result<CompiledGraph> {
    if crate::groups::structured(document) {
        let flat = crate::groups::flatten(document, options)?;
        let plan =
            validate_root(&flat.document, options, None).map_err(|error| flat.describe(error))?;
        return compile_plan(&flat.document, options, &plan, None, editable, cache)
            .map_err(|error| flat.describe(error));
    }
    let plan = validate(document, options)?;
    compile_plan(document, options, &plan, None, editable, cache)
}

/// Compile a typed diagnostic view of one output and its dependencies. This does
/// not alter the source document, permanent output or normal export contract.
pub fn compile_preview(
    document: &GraphDocument,
    options: &CompileOptions,
    target: &PortRef,
) -> Result<CompiledGraph> {
    compile_preview_mode(document, options, target, false, None)
}

pub fn compile_preview_editable(
    document: &GraphDocument,
    options: &CompileOptions,
    target: &PortRef,
) -> Result<CompiledGraph> {
    compile_preview_mode(document, options, target, true, None)
}

pub(crate) fn compile_preview_mode(
    document: &GraphDocument,
    options: &CompileOptions,
    target: &PortRef,
    editable: bool,
    cache: Option<&mut crate::EditableCompiler>,
) -> Result<CompiledGraph> {
    if crate::groups::structured(document) {
        let flat = crate::groups::flatten(document, options)?;
        let target = flat.resolve(target)?;
        return compile_preview_mode(&flat.document, options, &target, editable, cache)
            .map_err(|error| flat.describe(error));
    }
    if document.nodes.len() > options.max_nodes
        || document.connections.len() > options.max_connections
    {
        return Err(Error::invalid(
            ErrorCode::Budget,
            None,
            None,
            "source graph exceeds preview budget",
        ));
    }
    let mut keep = BTreeSet::new();
    let mut pending = vec![target.node.clone()];
    while let Some(id) = pending.pop() {
        if keep.insert(id.clone()) {
            pending.extend(
                document
                    .connections
                    .iter()
                    .filter(|edge| edge.to.node == id)
                    .map(|edge| edge.from.node.clone()),
            );
        }
    }
    let mut view = document.clone();
    view.nodes.retain(|node| keep.contains(&node.id));
    view.connections
        .retain(|edge| keep.contains(&edge.from.node) && keep.contains(&edge.to.node));
    let plan = validate_root(&view, options, Some(target))?;
    compile_plan(&view, options, &plan, Some(target), editable, cache)
}

fn compile_plan(
    document: &GraphDocument,
    options: &CompileOptions,
    plan: &ValidationReport,
    target: Option<&PortRef>,
    editable: bool,
    cache: Option<&mut crate::EditableCompiler>,
) -> Result<CompiledGraph> {
    let mut manifest = make_manifest(document, options)?;
    let mut builder = Builder::new(document, plan, editable);
    let nodes: BTreeMap<_, _> = document.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for id in &plan.reachable_nodes {
        builder.emit(nodes[id.as_str()])?;
    }
    let output = if let Some(target) = target {
        let symbol = &builder.symbols[target];
        let ty = node_ports(nodes[target.node.as_str()], &document.public_parameters)?.outputs
            [&target.port];
        match ty {
            ValueType::Color => symbol.expr.clone(),
            ValueType::Float => format!("ng_from_straight(vec4<f32>(vec3<f32>(clamp({},0.0,1.0)),1.0))", symbol.expr),
            ValueType::Vec2 => format!("ng_from_straight(vec4<f32>({},0.0,1.0))", symbol.expr),
            ValueType::Vec3 => format!("ng_from_straight(vec4<f32>({},1.0))", symbol.expr),
            ValueType::Bool => format!("ng_from_straight(vec4<f32>(vec3<f32>(select(0.0,1.0,{})),1.0))", symbol.expr),
            ValueType::DistancePixels => format!("ng_from_straight(vec4<f32>(vec3<f32>(clamp(0.5+{}/max(max(fx.size.x,fx.size.y),1.0),0.0,1.0)),1.0))", symbol.expr),
            ValueType::Image => {
                builder.samples += 1;
                format!("{}(p)", if symbol.image == Some(ImageSource::Input) { "sample_input" } else { "sample_source" })
            }
        }
    } else {
        builder.input(&plan.output_node, "color").expr
    };
    let mut support = helpers(document.alpha_mode);
    if plan
        .reachable_nodes
        .iter()
        .any(|id| matches!(nodes[id.as_str()].operation, NodeOp::Blend { .. }))
    {
        support.push_str(include_str!("blend.wgsl"));
    }
    if plan
        .reachable_nodes
        .iter()
        .any(|id| matches!(nodes[id.as_str()].operation, NodeOp::Noise { .. }))
    {
        support.push_str(include_str!("noise.wgsl"));
    }
    if plan
        .reachable_nodes
        .iter()
        .any(|id| matches!(nodes[id.as_str()].operation, NodeOp::Gradient { .. }))
    {
        support.push_str(include_str!("gradient.wgsl"));
    }
    let runtime = builder
        .data
        .as_ref()
        .map(|data| data.borrow().finish())
        .transpose()?;
    if builder
        .data
        .as_ref()
        .is_some_and(|data| data.borrow().has_overflow())
    {
        support.push_str(crate::runtime_data::HELPER);
    }
    let resources = if let Some(runtime) = runtime {
        let definition = &mut manifest.effects[0];
        definition.shader_data = Some(runtime.declaration);
        definition.resources = runtime.resources.keys().cloned().collect();
        definition
            .required_capabilities
            .push(motion_effects::shader_data::SHADER_DATA_CAPABILITY.into());
        manifest
            .validate()
            .map_err(|cause| Error::effect(ErrorCode::Manifest, "", cause))?;
        runtime.resources
    } else {
        BTreeMap::new()
    };
    let source=format!("// Motion Studio .msgraph format 1; SDK 6 single-pass image target.\n{}\nfn graph_fx(p:vec2<f32>)->vec4<f32>{{\n{}    return {};\n}}\n",support,builder.code,output);
    if source.len() > options.max_shader_bytes {
        return Err(Error::invalid(
            ErrorCode::Budget,
            None,
            None,
            "generated shader exceeds source byte budget",
        ));
    }
    let (shader, portable_compilations, compiler_cache_code_bytes) = match cache {
        Some(cache) => cache.shader(&source)?,
        None => (
            shader::compile(&source, "graph_fx")
                .map_err(|cause| Error::effect(ErrorCode::Shader, "", cause))?,
            1,
            0,
        ),
    };
    // This exact wrapper is shared with the effect package loader, not inferred
    // from loop_work (which is a separate portable-source validation metric).
    let host_sample_sites = 1;
    let data_samples = source.matches("ng_literal_f32(").count().saturating_sub(1) as u64;
    let total_sample_sites = builder.samples + data_samples + host_sample_sites;
    if shader.loop_work > options.max_loop_work
        || total_sample_sites > options.max_samples_per_fragment
    {
        return Err(Error::invalid(
            ErrorCode::Budget,
            None,
            None,
            format!(
                "compiled cost exceeds budget: loop_work={}, sample_sites={total_sample_sites}",
                shader.loop_work
            ),
        ));
    }
    let bindings: BTreeSet<_> = shader.glsl.textures.values().copied().collect();
    let report = CompileReport {
        target_sdk: 6,
        nodes: document.nodes.len(),
        reachable_nodes: plan.reachable_nodes.len(),
        unused_nodes: plan.unused_nodes.clone(),
        passes: manifest.effects[0].passes.len(),
        loop_work: shader.loop_work,
        graph_sample_sites: builder.samples + data_samples,
        host_sample_sites,
        total_sample_sites,
        reflected_texture_bindings: bindings.len(),
        package_resource_textures: manifest.effects[0].resources.len(),
        package_resource_bytes: resources.values().map(|bytes| bytes.len() as u64).sum(),
        shader_bytes: source.len(),
        portable_compilations,
        compiler_cache_code_bytes,
        emitted_values: builder.values,
        common_values_reused: builder.reused,
        folded_math_nodes: builder.folded,
        parameter_slots: plan.parameter_slots.clone(),
        pass_output_rgba8_bytes_4k: 3840 * 2160 * 4,
    };
    Ok(CompiledGraph {
        source,
        shader,
        manifest,
        report,
        resources,
    })
}
fn param_kind(ty: ValueType) -> Option<ParamKind> {
    Some(match ty {
        ValueType::Float => ParamKind::Float,
        ValueType::Vec2 => ParamKind::Vec2,
        ValueType::Vec3 => ParamKind::Vec3,
        ValueType::Color => ParamKind::Color,
        ValueType::Bool => ParamKind::Bool,
        _ => return None,
    })
}
fn make_manifest(document: &GraphDocument, options: &CompileOptions) -> Result<PluginManifest> {
    let params: Vec<ParamDefinition> = document
        .public_parameters
        .iter()
        .map(|p| ParamDefinition {
            id: p.id.clone(),
            name: format!("{} / {}", p.name, p.english_name),
            kind: param_kind(p.value_type).unwrap(),
            default: p.default,
            min: p.min,
            max: p.max,
            step: p.step,
            units: p.units.clone(),
            animatable: p.animatable,
            center_default: false,
            relative_default: None,
            implemented: true,
            options: vec![],
            reference_match_name: String::new(),
        })
        .collect();
    let effect_id = options.effect_id.as_ref().unwrap_or(&document.id);
    if effect_id.len() > 96 {
        return Err(Error::invalid(
            ErrorCode::Manifest,
            None,
            None,
            "effect ID exceeds 96 bytes",
        ));
    }
    let mut capabilities = vec!["single_frame", "color_profile"];
    let editor = if params.is_empty() {
        None
    } else {
        capabilities.push("native_plugin_editor");
        Some(motion_effects::NativeEditorDefinition {
            id: format!("{effect_id}.editor"),
            protocol: 1,
            title: document.name.clone(),
            sections: vec![
                motion_effects::NativeSection {
                    id: "preview".into(),
                    title: "预览 / Preview".into(),
                    slots: vec![
                        motion_effects::NativeSlot::Preview { max_size: 512 },
                        motion_effects::NativeSlot::Timeline,
                    ],
                },
                motion_effects::NativeSection {
                    id: "parameters".into(),
                    title: "参数 / Parameters".into(),
                    slots: vec![motion_effects::NativeSlot::Parameters {
                        params: params.iter().map(|p| p.id.clone()).collect(),
                    }],
                },
            ],
        })
    };
    let manifest: PluginManifest = serde_json::from_value(
        serde_json::json!({"format_version":1,"sdk_version":6,"id":options.plugin_id,"version":options.version,"name":format!("{} / {}",document.name,document.english_name),"author":options.author,"license":options.license,"effects":[{"id":effect_id,"name":document.name,"english_name":document.english_name,"category":"Graph","params":params,"passes":[{"shader":"shaders/graph.wgsl","entry":"graph_fx"}],"resources":[],"working_space":document.working_space,"alpha_mode":document.alpha_mode,"edge_mode":"transparent","required_capabilities":capabilities,"native_editor":editor,"compatibility":"approximate","compatibility_profile":"motion-graph-n1-desktop-tested","known_differences":["Android device image comparison remains pending; N1 produces validated GLSL ES 300."]}]}),
    )?;
    manifest
        .validate()
        .map_err(|cause| Error::effect(ErrorCode::Manifest, "", cause))?;
    if serde_json::to_vec_pretty(&manifest)?.len() > 512 * 1024 {
        return Err(Error::invalid(
            ErrorCode::Budget,
            None,
            None,
            "generated manifest exceeds the SDK 512 KiB limit",
        ));
    }
    Ok(manifest)
}

#[derive(Clone)]
struct Symbol {
    expr: String,
    constant: Option<Vec<f32>>,
    image: Option<ImageSource>,
}
struct Builder<'a> {
    document: &'a GraphDocument,
    slots: &'a BTreeMap<String, usize>,
    incoming: BTreeMap<PortRef, PortRef>,
    symbols: BTreeMap<PortRef, Symbol>,
    cache: BTreeMap<(ValueType, String), Symbol>,
    code: String,
    values: usize,
    reused: usize,
    folded: usize,
    samples: u64,
    data: Option<std::cell::RefCell<crate::runtime_data::NumericData>>,
}
impl<'a> Builder<'a> {
    fn new(document: &'a GraphDocument, plan: &'a ValidationReport, editable: bool) -> Self {
        Self {
            document,
            slots: &plan.parameter_slots,
            incoming: document
                .connections
                .iter()
                .map(|c| (c.to.clone(), c.from.clone()))
                .collect(),
            symbols: BTreeMap::new(),
            cache: BTreeMap::new(),
            code: String::new(),
            values: 0,
            reused: 0,
            folded: 0,
            samples: 0,
            data: editable.then(|| {
                std::cell::RefCell::new(crate::runtime_data::NumericData::new(
                    document.public_parameters.len(),
                ))
            }),
        }
    }
    fn input(&self, node: &str, port: &str) -> Symbol {
        if let Some(source) = self.incoming.get(&PortRef::new(node, port)) {
            return self.symbols[source].clone();
        }
        let value = &self
            .document
            .nodes
            .iter()
            .find(|n| n.id == node)
            .unwrap()
            .input_defaults[port];
        if let InputValue::Bool(value) = value {
            return Symbol {
                expr: self.data.as_ref().map_or_else(
                    || value.to_string(),
                    |data| {
                        data.borrow_mut().expression(
                            node,
                            port,
                            true,
                            ValueType::Bool,
                            &[if *value { 1. } else { 0. }],
                        )
                    },
                ),
                constant: None,
                image: None,
            };
        }
        let mut components = value.components().to_vec();
        if value.value_type() == ValueType::Color
            && self.document.alpha_mode == AlphaMode::Premultiplied
        {
            for index in 0..3 {
                components[index] *= components[3];
            }
        }
        Symbol {
            expr: self.data.as_ref().map_or_else(
                || literal(value.value_type(), &components),
                |data| {
                    data.borrow_mut()
                        .expression(node, port, true, value.value_type(), &components)
                },
            ),
            constant: Some(components),
            image: None,
        }
    }
    fn put(
        &mut self,
        node: &Node,
        port: &str,
        ty: ValueType,
        expr: String,
        constant: Option<Vec<f32>>,
        sample: bool,
    ) {
        let key = (ty, expr.clone());
        let symbol = if let Some(existing) = self.cache.get(&key) {
            self.reused += 1;
            existing.clone()
        } else {
            let variable = format!("ng{}", self.values);
            self.values += 1;
            self.code.push_str(&format!(
                "    // {}.{port}\n    let {variable}:{}={expr};\n",
                node.id,
                ty.wgsl()
            ));
            if sample {
                self.samples += 1;
            }
            let value = Symbol {
                expr: variable,
                constant,
                image: None,
            };
            self.cache.insert(key, value.clone());
            value
        };
        self.symbols.insert(PortRef::new(&node.id, port), symbol);
    }
    fn literal(&mut self, node: &Node, port: &str, ty: ValueType, value: Vec<f32>) {
        let expr = self.data.as_ref().map_or_else(
            || literal(ty, &value),
            |data| {
                data.borrow_mut()
                    .expression(&node.id, port, false, ty, &value)
            },
        );
        self.put(node, port, ty, expr, Some(value), false);
    }
    fn emit(&mut self, node: &Node) -> Result<()> {
        use NodeOp::*;
        let input = |name: &str| self.input(&node.id, name);
        let from_straight = |expr: String| format!("ng_from_straight({expr})");
        match &node.operation {
            Group { .. } | GroupInput { .. } | GroupOutput { .. } => {
                return Err(Error::invalid(
                    ErrorCode::InvalidValue,
                    Some(&node.id),
                    None,
                    "group nodes must be lowered before shader generation",
                ))
            }
            Time => self.put(
                node,
                "value",
                ValueType::Float,
                "fx.clock.x".into(),
                None,
                false,
            ),
            FrameIndex => self.put(
                node,
                "value",
                ValueType::Float,
                "fx.clock.y".into(),
                None,
                false,
            ),
            Resolution => self.put(
                node,
                "value",
                ValueType::Vec2,
                "fx.size.xy".into(),
                None,
                false,
            ),
            Position => self.put(node, "value", ValueType::Vec2, "p".into(), None, false),
            Uv => self.put(
                node,
                "value",
                ValueType::Vec2,
                "p/fx.size.xy".into(),
                None,
                false,
            ),
            Float { value } => self.literal(node, "value", ValueType::Float, vec![*value]),
            Vec2 { value } => self.literal(node, "value", ValueType::Vec2, value.to_vec()),
            Vec3 { value } => self.literal(node, "value", ValueType::Vec3, value.to_vec()),
            Color { value } => {
                let mut value = value.to_vec();
                if self.document.alpha_mode == AlphaMode::Premultiplied {
                    for i in 0..3 {
                        value[i] *= value[3];
                    }
                }
                self.literal(node, "value", ValueType::Color, value);
            }
            Bool { value } if self.data.is_some() => self.literal(
                node,
                "value",
                ValueType::Bool,
                vec![if *value { 1. } else { 0. }],
            ),
            Bool { value } => self.put(
                node,
                "value",
                ValueType::Bool,
                value.to_string(),
                None,
                false,
            ),
            Image { source } => {
                self.symbols.insert(
                    PortRef::new(&node.id, "image"),
                    Symbol {
                        expr: String::new(),
                        constant: None,
                        image: Some(*source),
                    },
                );
            }
            SampleImage { coordinates } => {
                let image = input("image").image.unwrap();
                let coord = input("coordinate").expr;
                let coord = if *coordinates == SampleCoordinates::Uv {
                    format!("({coord}*fx.size.xy)")
                } else {
                    coord
                };
                let function = if image == ImageSource::Input {
                    "sample_input"
                } else {
                    "sample_source"
                };
                self.put(
                    node,
                    "color",
                    ValueType::Color,
                    format!("{function}({coord})"),
                    None,
                    true,
                );
            }
            Parameter { parameter } => {
                let index = self.slots[parameter];
                let p = self
                    .document
                    .public_parameters
                    .iter()
                    .find(|p| p.id == *parameter)
                    .unwrap();
                let slot = format!("fx.params[{index}]");
                let expr = match p.value_type {
                    ValueType::Float => bounded(ValueType::Float, format!("{slot}.x")),
                    ValueType::Vec2 => bounded(ValueType::Vec2, format!("{slot}.xy")),
                    ValueType::Vec3 => bounded(ValueType::Vec3, format!("{slot}.xyz")),
                    ValueType::Bool => format!("({slot}.x>=0.5)"),
                    ValueType::Color => {
                        from_straight(format!("clamp({slot},vec4<f32>(0.0),vec4<f32>(1.0))"))
                    }
                    _ => unreachable!(),
                };
                self.put(node, "value", p.value_type, expr, None, false);
            }
            Math {
                operation,
                value_type,
            } => {
                let ports = node_ports(node, &self.document.public_parameters)?;
                let args: BTreeMap<_, _> = ports
                    .inputs
                    .keys()
                    .map(|name| (name.as_str(), input(name)))
                    .collect();
                let out = ports.outputs["value"];
                if let Some(value) = fold_math(*operation, *value_type, &args) {
                    self.folded += 1;
                    self.literal(node, "value", out, value);
                } else {
                    let expr = math_expr(*operation, *value_type, &args);
                    self.put(node, "value", out, bounded(out, expr), None, false);
                }
            }
            Splat { value_type } => {
                let value = input("value");
                if let Some(c) = value.constant {
                    self.literal(
                        node,
                        "value",
                        *value_type,
                        vec![c[0]; value_type.dimensions()],
                    );
                } else {
                    self.put(
                        node,
                        "value",
                        *value_type,
                        format!("{}({})", value_type.wgsl(), value.expr),
                        None,
                        false,
                    );
                }
            }
            CombineVec2 | CombineVec3 => {
                let names = if matches!(node.operation, CombineVec2) {
                    vec!["x", "y"]
                } else {
                    vec!["x", "y", "z"]
                };
                let ty = if names.len() == 2 {
                    ValueType::Vec2
                } else {
                    ValueType::Vec3
                };
                let args: Vec<_> = names.iter().map(|name| input(name)).collect();
                if args.iter().all(|a| a.constant.is_some()) {
                    self.literal(
                        node,
                        "value",
                        ty,
                        args.iter()
                            .map(|a| a.constant.as_ref().unwrap()[0])
                            .collect(),
                    );
                } else {
                    self.put(
                        node,
                        "value",
                        ty,
                        format!(
                            "{}({})",
                            ty.wgsl(),
                            args.iter()
                                .map(|a| a.expr.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        ),
                        None,
                        false,
                    );
                }
            }
            CombineColor => {
                let args: Vec<_> = ["r", "g", "b", "a"].iter().map(|n| input(n).expr).collect();
                self.put(
                    node,
                    "color",
                    ValueType::Color,
                    from_straight(format!(
                        "clamp(vec4<f32>({}),vec4<f32>(0.0),vec4<f32>(1.0))",
                        args.join(",")
                    )),
                    None,
                    false,
                );
            }
            SplitColor => {
                let c = input("color").expr;
                for channel in ["r", "g", "b", "a"] {
                    self.put(
                        node,
                        channel,
                        ValueType::Float,
                        format!("ng_to_straight({c}).{channel}"),
                        None,
                        false,
                    );
                }
            }
            ColorMath { operation } => {
                let expr = match operation {
                    ColorOp::Mix => format!(
                        "mix({},{},clamp({},0.0,1.0))",
                        input("a").expr,
                        input("b").expr,
                        input("factor").expr
                    ),
                    ColorOp::Multiply | ColorOp::Screen => {
                        let a = input("a").expr;
                        let b = input("b").expr;
                        let rgb = if *operation == ColorOp::Multiply {
                            format!("ng_to_straight({a}).rgb*ng_to_straight({b}).rgb")
                        } else {
                            format!("vec3<f32>(1.0)-(vec3<f32>(1.0)-ng_to_straight({a}).rgb)*(vec3<f32>(1.0)-ng_to_straight({b}).rgb)")
                        };
                        from_straight(format!("vec4<f32>({rgb},{a}.a*{b}.a)"))
                    }
                    _ => {
                        let c = input("color").expr;
                        let rgb = match operation {
                            ColorOp::Invert => format!("vec3<f32>(1.0)-ng_to_straight({c}).rgb"),
                            ColorOp::SrgbEncode => format!("srgb_encode(ng_to_straight({c}).rgb)"),
                            ColorOp::SrgbDecode => format!("srgb_decode(ng_to_straight({c}).rgb)"),
                            _ => unreachable!(),
                        };
                        from_straight(format!("vec4<f32>({rgb},{c}.a)"))
                    }
                };
                self.put(node, "color", ValueType::Color, expr, None, false);
            }
            Blend { mode } => {
                self.put(
                    node,
                    "color",
                    ValueType::Color,
                    format!(
                        "ng_blend({}u,{},{},{})",
                        *mode as u32,
                        input("backdrop").expr,
                        input("source").expr,
                        input("factor").expr
                    ),
                    None,
                    false,
                );
            }
            Noise { mode, octaves } => {
                let mode = u32::from(*mode == NoiseMode::Perlin);
                let position = format!(
                    "(({}+{})*clamp({},0.0,4096.0))",
                    input("coordinate").expr,
                    input("offset").expr,
                    input("scale").expr
                );
                let seed = format!("u32(floor(clamp({},0.0,65535.0)))", input("seed").expr);
                // Statically bounded work: no dynamic octave loop in exported shaders.
                let terms: Vec<_> = (0..*octaves)
                    .map(|i| {
                        let frequency = 1u32 << i;
                        let weight = 1. / frequency as f32;
                        format!(
                            "ng_noise({position}*{frequency}.0,({seed}+{}u),{mode}u)*{weight:?}",
                            u32::from(i) * 7919
                        )
                    })
                    .collect();
                let total: f32 = (0..*octaves).map(|i| 1. / (1u32 << i) as f32).sum();
                self.put(
                    node,
                    "factor",
                    ValueType::Float,
                    format!("(({})/{total:?})", terms.join("+")),
                    None,
                    false,
                );
                let factor = self.symbols[&PortRef::new(&node.id, "factor")].expr.clone();
                self.put(
                    node,
                    "color",
                    ValueType::Color,
                    format!("vec4<f32>(vec3<f32>({factor}),1.0)"),
                    None,
                    false,
                );
            }
            Gradient { mode } => {
                let mode = match mode {
                    GradientMode::Linear => 0,
                    GradientMode::Radial => 1,
                    GradientMode::Conic => 2,
                };
                let factor = format!(
                    "ng_gradient({},{},{},{},{mode}u)",
                    input("coordinate").expr,
                    input("start").expr,
                    input("end").expr,
                    input("falloff").expr,
                );
                let a = input("a").expr;
                let b = input("b").expr;
                self.put(node, "factor", ValueType::Float, factor, None, false);
                let factor = self.symbols[&PortRef::new(&node.id, "factor")].expr.clone();
                self.put(
                    node,
                    "color",
                    ValueType::Color,
                    format!("mix({a},{b},{factor})"),
                    None,
                    false,
                );
            }
            ApplyMask => {
                let c = input("color").expr;
                let mask = format!("clamp({},0.0,1.0)", input("coverage").expr);
                let expr = if self.document.alpha_mode == AlphaMode::Premultiplied {
                    format!("({c}*{mask})")
                } else {
                    format!("vec4<f32>({c}.rgb,{c}.a*{mask})")
                };
                self.put(node, "color", ValueType::Color, expr, None, false);
            }
            Select { value_type } => {
                let condition = input("condition").expr;
                let a = input("true_value").expr;
                let b = input("false_value").expr;
                let expr = if *value_type == ValueType::Bool {
                    format!("(({condition}&&{a})||(!{condition}&&{b}))")
                } else {
                    format!("select({b},{a},{condition})")
                };
                self.put(node, "value", *value_type, expr, None, false);
            }
            SdfCircle => self.put(
                node,
                "distance",
                ValueType::DistancePixels,
                format!(
                    "length({}-{})-max({},0.0)",
                    input("position").expr,
                    input("center").expr,
                    input("radius").expr
                ),
                None,
                false,
            ),
            SdfRoundedRectangle => self.put(
                node,
                "distance",
                ValueType::DistancePixels,
                format!(
                    "ng_rounded_rect({}-{}, {}, {})",
                    input("position").expr,
                    input("center").expr,
                    input("half_size").expr,
                    input("radius").expr
                ),
                None,
                false,
            ),
            SdfLine | SdfCapsule => {
                let radius = if matches!(node.operation, SdfLine) {
                    format!("({}*0.5)", input("width").expr)
                } else {
                    input("radius").expr
                };
                self.put(
                    node,
                    "distance",
                    ValueType::DistancePixels,
                    format!(
                        "ng_segment({}, {}, {})-max({radius},0.0)",
                        input("position").expr,
                        input("a").expr,
                        input("b").expr
                    ),
                    None,
                    false,
                );
            }
            SdfUnion | SdfIntersection | SdfDifference => {
                let a = input("a").expr;
                let b = input("b").expr;
                let expr = match node.operation {
                    SdfUnion => format!("min({a},{b})"),
                    SdfIntersection => format!("max({a},{b})"),
                    _ => format!("max({a},-{b})"),
                };
                self.put(
                    node,
                    "distance",
                    ValueType::DistancePixels,
                    expr,
                    None,
                    false,
                );
            }
            SdfSmoothUnion => self.put(
                node,
                "distance",
                ValueType::DistancePixels,
                format!(
                    "ng_smooth_union({}, {}, {})",
                    input("a").expr,
                    input("b").expr,
                    input("smoothness").expr
                ),
                None,
                false,
            ),
            SdfFill => self.put(
                node,
                "coverage",
                ValueType::Float,
                format!("ng_fill({})", input("distance").expr),
                None,
                false,
            ),
            SdfStroke => self.put(
                node,
                "coverage",
                ValueType::Float,
                format!(
                    "ng_stroke({}, {})",
                    input("distance").expr,
                    input("width").expr
                ),
                None,
                false,
            ),
            ColorOutput => {}
            Unsupported { .. } => unreachable!(),
        }
        Ok(())
    }
}
fn literal(ty: ValueType, value: &[f32]) -> String {
    if ty.dimensions() == 1 {
        format!("{:?}", value[0])
    } else {
        format!(
            "{}({})",
            ty.wgsl(),
            value
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}
fn bounded(ty: ValueType, expr: String) -> String {
    format!("clamp({expr},{}(-1.0e12),{}(1.0e12))", ty.wgsl(), ty.wgsl())
}
fn math_expr(op: MathOp, ty: ValueType, args: &BTreeMap<&str, Symbol>) -> String {
    let get = |n: &str| args[n].expr.as_str();
    use MathOp::*;
    match op {
        Add | Subtract | Multiply => format!(
            "({}{}{})",
            get("a"),
            match op {
                Add => "+",
                Subtract => "-",
                _ => "*",
            },
            get("b")
        ),
        Divide => format!("ng_div{}({}, {})", ty.dimensions(), get("a"), get("b")),
        Min | Max | Dot => format!(
            "{}({}, {})",
            match op {
                Min => "min",
                Max => "max",
                _ => "dot",
            },
            get("a"),
            get("b")
        ),
        Abs | Sin | Cos | Floor | Fract | Length => format!(
            "{}({})",
            match op {
                Abs => "abs",
                Sin => "sin",
                Cos => "cos",
                Floor => "floor",
                Fract => "fract",
                _ => "length",
            },
            get("value")
        ),
        Sqrt => format!("sqrt(max({},{}(0.0)))", get("value"), ty.wgsl()),
        Normalize => format!("({}/max(length({}),1.0e-6))", get("value"), get("value")),
        Clamp => format!(
            "clamp({},min({},{}),max({},{}))",
            get("value"),
            get("min"),
            get("max"),
            get("min"),
            get("max")
        ),
        Lerp => format!("mix({},{},{})", get("a"), get("b"), get("factor")),
        Smoothstep => format!(
            "ng_smooth{}({},{},{})",
            ty.dimensions(),
            get("edge0"),
            get("edge1"),
            get("value")
        ),
    }
}
fn fold_math(op: MathOp, ty: ValueType, args: &BTreeMap<&str, Symbol>) -> Option<Vec<f32>> {
    if args.values().any(|s| s.constant.is_none()) {
        return None;
    }
    let at = |n: &str, i: usize| args[n].constant.as_ref().unwrap()[i];
    let mut values = vec![];
    use MathOp::*;
    if matches!(op, Dot | Length | Normalize) {
        let names = if op == Dot {
            ("a", "b")
        } else {
            ("value", "value")
        };
        let sum = (0..ty.dimensions())
            .map(|i| at(names.0, i) * at(names.1, i))
            .sum::<f32>();
        return Some(if op == Dot {
            vec![sum.clamp(-NUMBER_LIMIT, NUMBER_LIMIT)]
        } else if op == Length {
            vec![sum.sqrt().min(NUMBER_LIMIT)]
        } else {
            (0..ty.dimensions())
                .map(|i| {
                    (at("value", i) / sum.sqrt().max(DIV_EPSILON))
                        .clamp(-NUMBER_LIMIT, NUMBER_LIMIT)
                })
                .collect()
        });
    }
    for i in 0..ty.dimensions() {
        let value = match op {
            Add => at("a", i) + at("b", i),
            Subtract => at("a", i) - at("b", i),
            Multiply => at("a", i) * at("b", i),
            Divide => {
                if at("b", i).abs() <= DIV_EPSILON {
                    0.0
                } else {
                    at("a", i) / at("b", i)
                }
            }
            Min => at("a", i).min(at("b", i)),
            Max => at("a", i).max(at("b", i)),
            Abs => at("value", i).abs(),
            Sin => at("value", i).sin(),
            Cos => at("value", i).cos(),
            Sqrt => at("value", i).max(0.0).sqrt(),
            Floor => at("value", i).floor(),
            Fract => at("value", i) - at("value", i).floor(),
            Clamp => at("value", i).clamp(
                at("min", i).min(at("max", i)),
                at("min", i).max(at("max", i)),
            ),
            Lerp => at("a", i) + (at("b", i) - at("a", i)) * at("factor", 0),
            Smoothstep => {
                let lo = at("edge0", i).min(at("edge1", i));
                let hi = at("edge0", i).max(at("edge1", i));
                if hi - lo <= DIV_EPSILON {
                    if at("value", i) >= lo {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    let t = ((at("value", i) - lo) / (hi - lo)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                }
            }
            _ => unreachable!(),
        };
        values.push(value.clamp(-NUMBER_LIMIT, NUMBER_LIMIT));
    }
    Some(values)
}
fn helpers(alpha: AlphaMode) -> String {
    let mut code = String::new();
    for (n, ty) in [(1, "f32"), (2, "vec2<f32>"), (3, "vec3<f32>")] {
        code.push_str(&format!("fn ng_div{n}(a:{ty},b:{ty})->{ty}{{let d=select(max(b,{ty}(1.0e-6)),min(b,{ty}(-1.0e-6)),b<{ty}(0.0));return select(a/d,{ty}(0.0),abs(b)<={ty}(1.0e-6));}}\nfn ng_smooth{n}(a:{ty},b:{ty},v:{ty})->{ty}{{let lo=min(a,b);let hi=max(a,b);let t=clamp((v-lo)/max(hi-lo,{ty}(1.0e-6)),{ty}(0.0),{ty}(1.0));return select(t*t*({ty}(3.0)-{ty}(2.0)*t),select({ty}(0.0),{ty}(1.0),v>=lo),hi-lo<={ty}(1.0e-6));}}\n"));
    }
    code.push_str(if alpha==AlphaMode::Premultiplied{"fn ng_to_straight(c:vec4<f32>)->vec4<f32>{return straight(c);}\nfn ng_from_straight(c:vec4<f32>)->vec4<f32>{return vec4<f32>(c.rgb*c.a,c.a);}\n"}else{"fn ng_to_straight(c:vec4<f32>)->vec4<f32>{return c;}\nfn ng_from_straight(c:vec4<f32>)->vec4<f32>{return c;}\n"});
    code.push_str(r#"fn ng_rounded_rect(p:vec2<f32>,half_size:vec2<f32>,radius:f32)->f32{let h=max(half_size,vec2<f32>(0.0));let r=clamp(radius,0.0,min(h.x,h.y));let q=abs(p)-h+vec2<f32>(r);return length(max(q,vec2<f32>(0.0)))+min(max(q.x,q.y),0.0)-r;}
fn ng_segment(p:vec2<f32>,a:vec2<f32>,b:vec2<f32>)->f32{let ba=b-a;let h=clamp(dot(p-a,ba)/max(dot(ba,ba),1.0e-12),0.0,1.0);return length(p-a-ba*h);}
fn ng_smooth_union(a:f32,b:f32,k:f32)->f32{if k<=1.0e-6{return min(a,b);}let h=clamp(0.5+0.5*(b-a)/k,0.0,1.0);return mix(b,a,h)-k*h*(1.0-h);}
fn ng_fill(d:f32)->f32{let w=max(fwidth(d),1.0e-5);return 1.0-smoothstep(-w*0.5,w*0.5,d);}
fn ng_stroke(d:f32,width:f32)->f32{let w=max(fwidth(d),1.0e-5);let edge=abs(d)-max(width,0.0)*0.5;return 1.0-smoothstep(-w*0.5,w*0.5,edge);}
"#);
    code
}
