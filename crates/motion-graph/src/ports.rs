//! Additive port meaning and help. ValueType remains the shader's storage shape;
//! semantic units are known only for declared inputs and explicit source nodes.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortSemantic {
    PositionPixels,
    Uv,
    SizePixels,
    DistancePixels,
    Color,
    Seconds,
    Frame,
    Scalar,
    Vector,
    Bool,
    Image,
}
impl PortSemantic {
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::PositionPixels => ("像素位置", "Position · px"),
            Self::Uv => ("归一化 UV", "Normalized UV"),
            Self::SizePixels => ("像素长度/尺寸", "Length / size · px"),
            Self::DistancePixels => ("有符号像素距离", "Signed distance · px"),
            Self::Color => ("颜色", "Color"),
            Self::Seconds => ("秒", "Seconds"),
            Self::Frame => ("帧（含小数）", "Frames · fractional"),
            Self::Scalar => ("普通标量（单位未声明）", "Scalar · unspecified units"),
            Self::Vector => ("普通向量（单位未声明）", "Vector · unspecified units"),
            Self::Bool => ("开关", "Boolean"),
            Self::Image => ("宿主图像", "Host image"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PortSpec {
    pub id: String,
    pub name: &'static str,
    pub english_name: &'static str,
    pub description: &'static str,
    pub english_description: &'static str,
    pub value_type: ValueType,
    pub semantic: PortSemantic,
}
#[derive(Clone, Debug, Serialize)]
pub struct PortSpecs {
    pub inputs: BTreeMap<String, PortSpec>,
    pub outputs: BTreeMap<String, PortSpec>,
}

/// Metadata is derived from the same node_ports shape contract used by N1.
pub fn node_port_specs(node: &Node, parameters: &[PublicParameter]) -> Result<PortSpecs> {
    let ports = node_ports(node, parameters)?;
    let build = |entries: BTreeMap<String, ValueType>, output| {
        entries
            .into_iter()
            .map(|(id, value_type)| {
                let semantic = semantic(&node.operation, &id, output, value_type, parameters);
                let (name, english_name, description, english_description) =
                    help(&node.operation, &id, output, semantic);
                (
                    id.clone(),
                    PortSpec {
                        id,
                        name,
                        english_name,
                        description,
                        english_description,
                        value_type,
                        semantic,
                    },
                )
            })
            .collect()
    };
    Ok(PortSpecs {
        inputs: build(ports.inputs, false),
        outputs: build(ports.outputs, true),
    })
}

fn generic(value_type: ValueType) -> PortSemantic {
    match value_type {
        ValueType::Float => PortSemantic::Scalar,
        ValueType::Vec2 | ValueType::Vec3 => PortSemantic::Vector,
        ValueType::Color => PortSemantic::Color,
        ValueType::Bool => PortSemantic::Bool,
        ValueType::Image => PortSemantic::Image,
        ValueType::DistancePixels => PortSemantic::DistancePixels,
    }
}
fn semantic(
    op: &NodeOp,
    id: &str,
    output: bool,
    value_type: ValueType,
    parameters: &[PublicParameter],
) -> PortSemantic {
    if let Some(port) = group_port(op, id, output) {
        return port.semantic;
    }
    use PortSemantic::*;
    if output {
        return match op {
            NodeOp::Position => PositionPixels,
            NodeOp::Uv | NodeOp::UvMapping => Uv,
            NodeOp::Resolution => SizePixels,
            NodeOp::Time => Seconds,
            NodeOp::FrameIndex => Frame,
            NodeOp::Parameter { parameter } => {
                let units = parameters
                    .iter()
                    .find(|p| p.id == *parameter)
                    .map(|p| p.units.as_str())
                    .unwrap_or("");
                match (value_type, units) {
                    (ValueType::Float | ValueType::Vec2 | ValueType::Vec3, "px") => SizePixels,
                    (ValueType::Float, "s" | "seconds") => Seconds,
                    (ValueType::Float, "frame" | "frames") => Frame,
                    _ => generic(value_type),
                }
            }
            // Math and component construction do not prove or propagate units.
            _ => generic(value_type),
        };
    }
    match (op, id) {
        (NodeOp::UvMapping, "coordinate" | "translation" | "pivot") => Uv,
        (NodeOp::Gradient { .. }, "coordinate" | "start" | "end") => Uv,
        (NodeOp::Noise { .. }, "coordinate" | "offset") => Uv,
        (
            NodeOp::SampleImage {
                coordinates: SampleCoordinates::Pixels,
            },
            "coordinate",
        ) => PositionPixels,
        (
            NodeOp::SampleImage {
                coordinates: SampleCoordinates::Uv,
            },
            "coordinate",
        ) => Uv,
        (NodeOp::SdfCircle | NodeOp::SdfRoundedRectangle, "position" | "center") => PositionPixels,
        (NodeOp::SdfLine | NodeOp::SdfCapsule, "position" | "a" | "b") => PositionPixels,
        (NodeOp::SdfCircle | NodeOp::SdfRoundedRectangle | NodeOp::SdfCapsule, "radius") => {
            SizePixels
        }
        (NodeOp::SdfRoundedRectangle, "half_size") => SizePixels,
        (NodeOp::SdfLine | NodeOp::SdfStroke, "width") => SizePixels,
        (NodeOp::SdfSmoothUnion, "smoothness") => SizePixels,
        _ => generic(value_type),
    }
}

/// Generic numeric ports accept explicit constants/arithmetic. Pixel position and
/// pixel size share a coordinate unit; their *role* is documented by PortSpec.
/// UV, seconds and frames cannot directly enter a different declared unit.
pub fn port_semantics_compatible(source: PortSemantic, target: PortSemantic) -> bool {
    use PortSemantic::*;
    if source == Scalar {
        return matches!(target, Scalar | SizePixels | Seconds | Frame);
    }
    if target == Scalar {
        return matches!(source, Scalar | SizePixels | Seconds | Frame);
    }
    if source == Vector {
        return matches!(target, Vector | PositionPixels | Uv | SizePixels);
    }
    if target == Vector {
        return matches!(source, Vector | PositionPixels | Uv | SizePixels);
    }
    source == target
        || matches!(
            (source, target),
            (PositionPixels, SizePixels) | (SizePixels, PositionPixels)
        )
}
pub fn ports_compatible(source: &PortSpec, target: &PortSpec) -> bool {
    source.value_type == target.value_type
        && port_semantics_compatible(source.semantic, target.semantic)
}

/// Shared by authoring UI and N1 validation/compilation. Checks endpoints, storage
/// shapes and known units; does not insert an edge or perform DAG cycle checks.
pub fn check_connection(document: &GraphDocument, from: &PortRef, to: &PortRef) -> Result<()> {
    let source_node = document
        .nodes
        .iter()
        .find(|n| n.id == from.node)
        .ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownNode,
                Some(&from.node),
                Some(&from.port),
                "connection source node is missing",
            )
        })?;
    let target_node = document
        .nodes
        .iter()
        .find(|n| n.id == to.node)
        .ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownNode,
                Some(&to.node),
                Some(&to.port),
                "connection destination node is missing",
            )
        })?;
    let source = node_port_specs(source_node, &document.public_parameters)?
        .outputs
        .remove(&from.port)
        .ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownPort,
                Some(&from.node),
                Some(&from.port),
                "unknown output port",
            )
        })?;
    let target = node_port_specs(target_node, &document.public_parameters)?
        .inputs
        .remove(&to.port)
        .ok_or_else(|| {
            Error::invalid(
                ErrorCode::UnknownPort,
                Some(&to.node),
                Some(&to.port),
                "unknown input port",
            )
        })?;
    check_specs(from, to, &source, &target)
}
pub(crate) fn check_specs(
    from: &PortRef,
    to: &PortRef,
    source: &PortSpec,
    target: &PortSpec,
) -> Result<()> {
    if source.value_type != target.value_type {
        return Err(Error::invalid(
            ErrorCode::TypeMismatch,
            Some(&to.node),
            Some(&to.port),
            format!(
                "{:?} from {}.{} cannot connect to {:?}; no implicit conversion",
                source.value_type, from.node, from.port, target.value_type
            ),
        ));
    }
    if !port_semantics_compatible(source.semantic, target.semantic) {
        return Err(Error::invalid(ErrorCode::TypeMismatch,Some(&to.node),Some(&to.port),format!("{} from {}.{} cannot connect to {} at {}.{}; use an explicit unit conversion (arithmetic outputs have unspecified units)",source.semantic.label().1,from.node,from.port,target.semantic.label().1,to.node,to.port)));
    }
    Ok(())
}

type Help = (&'static str, &'static str, &'static str, &'static str);
fn help(op: &NodeOp, id: &str, output: bool, semantic: PortSemantic) -> Help {
    if matches!(op, NodeOp::UvMapping) {
        return match (output, id) {
            (false, "coordinate") => ("采样 UV", "Sample UV", "归一化采样坐标，X 向右、Y 向下；必须显式连接。", "Normalized sample coordinates, X right and Y down; connect explicitly."),
            (false, "translation") => ("平移 UV", "Translation UV", "在缩放和旋转之后加入的归一化偏移。", "Normalized offset added after scale and rotation."),
            (false, "rotation") => ("旋转角度", "Rotation (degrees)", "以度为单位绕轴心旋转采样坐标；正角度在向下的 Y 轴中为顺时针。", "Rotate sample coordinates around the pivot in degrees; positive is clockwise with a downward Y axis."),
            (false, "scale") => ("坐标缩放", "Coordinate scale", "先缩放采样坐标；数值越大纹理越小，负值镜像，零固定该轴。", "Scale sample coordinates first; larger values shrink the texture, negative values mirror, and zero fixes an axis."),
            (false, "pivot") => ("轴心 UV", "Pivot UV", "缩放和旋转的归一化轴心，默认为图像中心。", "Normalized pivot for scale and rotation, defaulting to image center."),
            _ => ("映射 UV", "Mapped UV", "映射后的归一化坐标，可连接 UV 模式的采样图像。", "Mapped normalized coordinates for Sample Image in UV mode."),
        };
    }
    if matches!(op, NodeOp::Blend { .. }) {
        return match (output, id) {
            (false, "backdrop") => ("底色", "Backdrop", "被覆盖的颜色，遵循图的工作空间与 Alpha 模式；运算限制在 0..1。", "Color below the source, in the graph working space and alpha mode; bounded to 0..1."),
            (false, "source") => ("源颜色", "Source", "叠加在底色上的颜色；透明部分保留底色，不读取其他图层。", "Color over the backdrop. Transparent regions preserve it; does not read other layers."),
            (false, "factor") => ("强度", "Strength", "限制为 0..1 并乘到源 Alpha；零保留底色，一使用完整源不透明度。", "Clamped to 0..1 and multiplied by source alpha. Zero keeps the backdrop; one uses full source opacity."),
            _ => ("颜色", "Color", "按当前混合模式进行 source-over 合成；输出遵循图的 Alpha 模式。", "Source-over result using the selected transfer; output follows the graph alpha mode."),
        };
    }
    if matches!(op, NodeOp::Noise { .. }) {
        return match (output, id) {
            (false, "coordinate") => ("坐标", "Coordinate", "连接归一化 UV；噪声域限制为正负一百万以保证整数安全。", "Connect normalized UV. Each octave's noise domain is clamped to +/-1,000,000 for integer safety."),
            (false, "offset") => ("偏移", "Offset", "缩放前加入坐标的 UV 偏移，可连接动画。", "UV offset added before scale; may be animated by a connection."),
            (false, "scale") => ("缩放", "Scale", "每单位 UV 的频率，限制为 0..4096；数值越大图案越细。", "Frequency per UV unit, clamped to 0..4096. Higher values produce finer detail."),
            (false, "seed") => ("种子", "Seed", "限制为 0..65535 后向下取整；相同种子与坐标得到相同结果。", "Clamped to 0..65535, then floored. Equal seed and coordinates produce equal results."),
            (true, "factor") => ("因子", "Factor", "归一化到 0..1；层数 1..4，频率逐层翻倍、权重逐层减半。", "0..1 factor. One to four octaves, doubling frequency and halving weight, normalized by total weight."),
            _ => ("颜色", "Color", "工作空间中的不透明灰度颜色，可直接连接颜色输出。", "Opaque grayscale in the graph working space; connect directly to Color Output."),
        };
    }
    if matches!(op, NodeOp::Gradient { .. }) {
        return match (output, id) {
            (false, "coordinate") => ("坐标", "Coordinate", "连接 UV；X 向右、Y 向下，按归一化坐标计算，不自动校正宽高比。", "Connect UV; X right, Y down. Uses normalized coordinates without automatic aspect correction."),
            (false, "start") => ("起点", "Start", "线性渐变起点，或径向/角向渐变中心；UV 坐标。", "Linear start or radial/conic center, in UV coordinates."),
            (false, "end") => ("终点", "End", "线性终点；径向半径由两点距离确定；角向零度由起点指向终点确定。", "Linear end; radial radius is the endpoint distance; conic zero points from start to end."),
            (false, "falloff") => ("衰减指数", "Falloff", "因子取 0..1 后乘幂；指数限制在 0.001..1000，1 为原始渐变。", "Exponent applied to the clamped factor; bounded to 0.001..1000, with 1 preserving the gradient."),
            (false, "a") => ("起始颜色", "Start color", "因子为零时的颜色，遵循图的工作空间和 Alpha 模式。", "Color at factor zero, in the graph working space and alpha mode."),
            (false, "b") => ("结束颜色", "End color", "因子为一时的颜色，遵循图的工作空间和 Alpha 模式。", "Color at factor one, in the graph working space and alpha mode."),
            (true, "factor") => ("因子", "Factor", "0..1 渐变因子；角向沿顺时针增加，中心和重合端点返回零。", "0..1 gradient factor; conic increases clockwise. The conic center and coincident endpoints return zero."),
            _ => ("颜色", "Color", "按因子混合起始/结束颜色，可直接连接颜色输出。", "Mixes the endpoint colors by the factor; connect directly to Color Output."),
        };
    }
    if output {
        if matches!(op, NodeOp::SplitColor) {
            return match id {
                "r" => (
                    "红通道",
                    "Red",
                    "straight RGB 的红分量，作为普通 Float 输出，不声明坐标单位。",
                    "Red component of straight RGB as a generic Float, without coordinate units.",
                ),
                "g" => (
                    "绿通道",
                    "Green",
                    "straight RGB 的绿分量，作为普通 Float 输出。",
                    "Green component of straight RGB as a generic Float.",
                ),
                "b" => (
                    "蓝通道",
                    "Blue",
                    "straight RGB 的蓝分量，作为普通 Float 输出。",
                    "Blue component of straight RGB as a generic Float.",
                ),
                _ => (
                    "Alpha 通道",
                    "Alpha",
                    "透明度分量，作为普通 Float 输出。",
                    "Opacity component as a generic Float.",
                ),
            };
        }
        return match semantic {
            PortSemantic::PositionPixels => ("像素位置","Position (pixels)","图层左上角为 (0,0)，X 向右、Y 向下；单位是图层逻辑像素，与预览纹理分辨率无关。","Layer top-left is (0,0); X goes right and Y goes down. Units are layer logical pixels, independent of preview texture resolution."),
            PortSemantic::Uv => ("归一化坐标","Normalized UV","UV = Position / Resolution；图层范围内为 0..1。不要直接连接需要像素坐标的输入。","UV = Position / Resolution; it is 0..1 across the layer. Do not directly connect it to pixel-coordinate inputs."),
            PortSemantic::SizePixels if matches!(op,NodeOp::Resolution) => ("图层尺寸","Layer size","(宽度,高度)，单位是原图层逻辑像素；不是节点画布或宿主预览纹理的尺寸。","(width,height) in original layer logical pixels; not the node canvas or host preview texture size."),
            PortSemantic::SizePixels => ("像素值","Pixel value","参数声明的单位为 px；端口不声明它是位置还是尺寸，两者角色由目标输入说明。","The parameter declares px units. Position versus size role is described by the target input."),
            PortSemantic::Seconds if matches!(op,NodeOp::Time) => ("局部秒","Local seconds","效果局部时间，单位为秒；保留小数，不是绝对系统时间。","Effect-local time in seconds; fractional values are preserved, not absolute system time."),
            PortSemantic::Seconds => ("秒值","Seconds value","参数声明以秒为单位；可能表示时长或偏移，不自动代表宿主当前时钟。","Parameter-declared seconds; may represent duration or offset, not automatically the host's current clock."),
            PortSemantic::Frame if matches!(op,NodeOp::FrameIndex) => ("局部帧","Local frame","效果局部帧索引，保留小数帧；不要直接当作秒或像素长度使用。","Effect-local frame index, including fractional frames; do not directly treat it as seconds or pixel length."),
            PortSemantic::Frame => ("帧值","Frames value","参数声明以帧为单位；不自动代表宿主当前帧索引，也不隐式换算为秒。","Parameter-declared frames; not automatically the host's current frame index and not implicitly converted to seconds."),
            PortSemantic::DistancePixels => ("有符号距离","Signed distance","单位是图层逻辑像素，内侧负、边界零、外侧正；连接 SDF 运算、Fill 或 Stroke。","Layer logical pixels: negative inside, zero at the boundary, positive outside. Connect to SDF operations, Fill or Stroke."),
            PortSemantic::Color => ("颜色","Color","当前图声明工作空间/Alpha 模式中的 RGBA；颜色常量及参数本身以 straight RGBA 编辑。","RGBA in the graph's declared working space and alpha mode; color literals and parameters are edited as straight RGBA."),
            PortSemantic::Bool => ("开关","Boolean","true/false，只能接开关条件；不隐式转换为 Float。","true/false; connects to boolean conditions without implicit Float conversion."),
            PortSemantic::Image => ("图像句柄","Image handle","宿主提供的当前 pass 输入或效果原始图像；先通过 Sample Image 得到 Color，不支持任意外部纹理绑定。","Host-provided current-pass input or original source. Sample Image converts it to Color; arbitrary external texture bindings are unsupported."),
            PortSemantic::Vector => ("数值向量","Numeric vector","普通 Vec2/Vec3；常量、组合与数学输出不证明像素/UV 单位，也不代表 3D 位置或网格属性。","Generic Vec2/Vec3. Constants, construction and arithmetic do not prove pixel/UV units or imply 3D positions or mesh attributes."),
            PortSemantic::Scalar if matches!(op,NodeOp::SdfFill|NodeOp::SdfStroke) => ("覆盖率","Coverage","0..1 的抗锯齿覆盖率，适合 Apply Mask 的 coverage 输入；不是像素距离。","Antialiased 0..1 coverage for Apply Mask's coverage input; not a pixel distance."),
            _ => ("数值","Value","普通 Float，单位未声明；数学结果不会自动继承输入单位。","Generic Float with unspecified units; arithmetic results do not automatically inherit input units."),
        };
    }
    match (op,id) {
        (NodeOp::SampleImage{coordinates:SampleCoordinates::Pixels},"coordinate") => ("采样像素位置","Sample position (pixels)","图层左上为原点，X 右、Y 下，单位是逻辑像素；例如连接 Position。","Layer top-left origin, X right/Y down, in logical pixels; connect Position, for example."),
        (NodeOp::SampleImage{coordinates:SampleCoordinates::Uv},"coordinate") => ("采样 UV","Sample UV","归一化采样坐标；图层范围内为 0..1，例如连接 UV。","Normalized sample coordinates, 0..1 across the layer; connect UV, for example."),
        (_,"position") => ("像素位置","Position (pixels)","要评估距离的图层逻辑像素坐标；左上为原点，X 右、Y 下。已知 UV 输入必须先换算。","Layer logical pixel coordinates to evaluate; top-left origin, X right/Y down. Known UV sources must be converted first."),
        (_,"center") => ("像素中心","Center (pixels)","形状中心，单位是图层逻辑像素，与 position 使用同一坐标系。","Shape center in layer logical pixels, using the same coordinate system as position."),
        (_,"radius") => ("半径","Radius (pixels)","非负像素半径；负值按零处理，圆角矩形半径还限制到最小半尺寸内。","Nonnegative pixel radius; negatives act as zero. Rounded-rectangle radius additionally clamps to its smallest half size."),
        (_,"half_size") => ("半尺寸","Half size (pixels)","(半宽,半高)，单位是逻辑像素。例如 (24,12) 得到 48×24 的完整尺寸。","(half width, half height) in logical pixels. For example, (24,12) gives full dimensions of 48×24."),
        (_,"width") => ("全宽","Full width (pixels)","像素全宽而非半宽；负值按零处理。SDF Stroke 以边界为中心向两侧各延伸 width/2。","Full pixel width, not half width; negatives act as zero. SDF Stroke extends width/2 to either side of the boundary."),
        (_,"smoothness") => ("融合宽度","Blend width (pixels)","平滑并集的像素宽度；零或负值退回普通并集。不是颜色混合系数。","Pixel width for smooth union; zero or negative values fall back to ordinary union. Not a color-mix factor."),
        (NodeOp::SdfLine|NodeOp::SdfCapsule,"a") => ("起点","Start (pixels)","线段起点，图层左上为原点，单位是逻辑像素。","Segment start in layer logical pixels, using the top-left origin."),
        (NodeOp::SdfLine|NodeOp::SdfCapsule,"b") => ("终点","End (pixels)","线段终点，与起点/position 使用同一逻辑像素坐标系。","Segment end, in the same logical-pixel coordinate system as start/position."),
        (_,"distance") => ("有符号距离","Signed distance","以逻辑像素为单位的 SDF，内负/外正；不能把普通 Float 隐式转为 Distance Pixels。","SDF in logical pixels, negative inside/positive outside. Ordinary Float does not implicitly convert to Distance Pixels."),
        (NodeOp::SdfUnion|NodeOp::SdfIntersection|NodeOp::SdfDifference|NodeOp::SdfSmoothUnion,"a") => ("距离 A","Distance A","第一个形状的 Distance Pixels 输出；形状需使用同一像素坐标系。","First shape's Distance Pixels output; shapes must use the same pixel coordinate system."),
        (NodeOp::SdfUnion|NodeOp::SdfIntersection|NodeOp::SdfDifference|NodeOp::SdfSmoothUnion,"b") => ("距离 B","Distance B","第二个形状的 Distance Pixels 输出；融合的是距离场而不是颜色。","Second shape's Distance Pixels output; combine distance fields rather than colors."),
        (_,"image") => ("宿主图像","Host image","Image 句柄，只能来自已有宿主图像来源节点，不能直接作为 Color 使用。","Image handle from a supported host image source; cannot be used directly as Color."),
        (_,"coverage") => ("覆盖率","Coverage","0..1 标量，限制后乘到 Alpha；保持 straight RGB，不进行图层合成。","0..1 scalar, clamped and multiplied into alpha; straight RGB is preserved, without layer compositing."),
        (_,"color") => ("颜色","Color","RGBA Color 输入；按图的工作空间/Alpha 模式处理。反相等 RGB 运算不反转 Alpha。","RGBA Color input in the graph's working-space/alpha contract. RGB operations such as Invert do not invert alpha."),
        (NodeOp::CombineColor,"r") => ("红通道","Red","Float 红通道；与其他三个通道显式组成 RGBA。","Float red channel, explicitly combined with the other three RGBA channels."),
        (NodeOp::CombineColor,"g") => ("绿通道","Green","Float 绿通道；常规颜色范围为 0..1。","Float green channel; conventional color range is 0..1."),
        (NodeOp::CombineColor,"b") => ("蓝通道","Blue","Float 蓝通道；常规颜色范围为 0..1。","Float blue channel; conventional color range is 0..1."),
        (NodeOp::CombineColor,"a") => ("Alpha 通道","Alpha","Float 透明度通道；0 为透明、1 为不透明。","Float opacity channel; 0 is transparent and 1 is opaque."),
        (_,"condition") => ("开关条件","Condition","Bool 条件，为真/假时选择对应分支；两路 value_type 必须相同。","Bool condition selecting its true/false branch; both branches require the same value_type."),
        (_,"true_value") => ("为真时的值","True value","condition 为 true 时输出该值，不能与另一分支隐式换型。","Output when condition is true; no implicit type conversion between branches."),
        (_,"false_value") => ("为假时的值","False value","condition 为 false 时输出该值。","Output when condition is false."),
        (_,"factor") => ("插值系数","Factor","Float 插值系数；Color Mix 限制为 0..1，Math Lerp 允许外插。","Float interpolation factor; Color Mix clamps to 0..1, while Math Lerp allows extrapolation."),
        (_,"min") => ("下界","Minimum","与 value 相同数值形状；Clamp 会自动排序上下界，当前不传播单位。","Same numeric shape as value; Clamp orders its bounds, without unit propagation."),
        (_,"max") => ("上界","Maximum","与 value 相同数值形状；上下界可交换。","Same numeric shape as value; bounds may be swapped."),
        (_,"edge0") => ("过渡端点 0","Edge 0","Smoothstep 第一端点，形状与 value 相同；两端点自动排序。","First Smoothstep endpoint, same shape as value; endpoints are sorted."),
        (_,"edge1") => ("过渡端点 1","Edge 1","Smoothstep 第二端点；端点重合时返回阶跃。","Second Smoothstep endpoint; equal endpoints return a step."),
        (_,"x") => ("X 分量","X component","显式 Float 分量；组合后的向量不自动声明像素或 UV 单位。","Explicit Float component; the constructed vector does not automatically declare pixel or UV units."),
        (_,"y") => ("Y 分量","Y component","显式 Float 分量，不进行隐式标量广播。","Explicit Float component without implicit scalar broadcasting."),
        (_,"z") => ("Z 分量","Z component","普通向量的 Float 分量，不代表宿主提供的 3D 位置或深度。","Float component of a generic vector, not host-provided 3D position or depth."),
        (_,"a") => ("输入 A","Input A","第一路同类型数值/颜色；运算本身不证明坐标单位。","First same-type numeric/color input; the operation does not prove coordinate units."),
        (_,"b") => ("输入 B","Input B","第二路同类型数值/颜色；没有隐式广播或类型转换。","Second same-type numeric/color input without implicit broadcast or conversion."),
        _ => ("数值","Value","按端口的 value_type 输入；普通数学输入/输出未声明坐标单位。","Input according to value_type; generic arithmetic inputs/outputs have unspecified coordinate units."),
    }
}

pub fn node_description(operation: &NodeOp) -> (&'static str, &'static str) {
    match operation {
        NodeOp::Blend { .. } => ("使用 15 种可分离模式合成两个颜色；强度控制源不透明度，遵循图的工作空间，数值限制在 0..1。", "Composites two colors with 15 separable modes. Strength controls source opacity in the graph working space; values bounded to 0..1."),
        NodeOp::Noise { .. } => ("生成二维值噪声或梯度噪声，支持 1..4 层；显式 UV，可调缩放、偏移和种子，同时输出因子与灰度颜色。", "Generates 2D Value or Perlin gradient noise with 1..4 octaves. Explicit UV, adjustable scale/offset/seed, factor and grayscale outputs."),
        NodeOp::Gradient { .. } => ("在线性、径向和顺时针角向模式中生成双色渐变与 0..1 因子；需要显式 UV，衰减指数可调。", "Generates a two-color gradient and 0..1 factor in linear, radial or clockwise conic mode; requires explicit UV and supports adjustable falloff."),
        NodeOp::Group { .. } => ("双击或 Tab 进入子图；Ctrl+Tab 返回；Alt+G 解包。分组保留端口类型、单位和原始计算结果。", "Double-click or Tab to enter; Ctrl+Tab returns; Alt+G ungroups. Boundaries preserve types, units and computation."),
        NodeOp::GroupInput { .. } => ("从父图读取节点组输入；边界节点不可删除。", "Reads the group inputs from its parent graph. This boundary node cannot be deleted."),
        NodeOp::GroupOutput { .. } => ("把内部结果交给父图的组输出；边界节点不可删除。", "Routes internal values to the parent group outputs. This boundary node cannot be deleted."),
        NodeOp::Time => ("输出效果局部秒 fx.clock.x，保留小数。用速度换算为位移后再连接像素位置。","Outputs effect-local seconds fx.clock.x, including fractions. Convert using a speed before producing a pixel position."),
        NodeOp::FrameIndex => ("输出效果局部帧 fx.clock.y，保留小数；它不是秒数。","Outputs effect-local frames fx.clock.y, including fractions; it is not a seconds value."),
        NodeOp::Resolution => ("输出原图层逻辑像素尺寸 (宽,高)，不是预览纹理尺寸。","Outputs original layer logical-pixel dimensions (width,height), not preview texture size."),
        NodeOp::Position => ("输出当前片元的图层逻辑像素位置；左上为原点，X 向右、Y 向下。","Outputs the fragment's layer logical-pixel position; top-left origin, X right and Y down."),
        NodeOp::Uv => ("输出 Position / Resolution 的归一化 UV；使用像素距离场时需要显式换算。","Outputs normalized Position / Resolution UV; explicitly convert when using pixel distance fields."),
        NodeOp::UvMapping => ("对归一化 UV 先绕轴心缩放，再按角度旋转，最后平移；这是采样坐标变换，负缩放可翻转图像。","Scale normalized UV around the pivot, rotate in degrees, then translate. This transforms sample coordinates; negative scale mirrors the image."),
        NodeOp::Image{source:ImageSource::Input} => ("当前效果 pass 的宿主输入图像句柄；通过 Sample Image 读取像素。","Host input-image handle for the current effect pass; read pixels with Sample Image."),
        NodeOp::Image{source:ImageSource::Source} => ("效果链原始图层图像句柄；与当前 pass 输入来源不同。","Original layer-image handle before the effect chain, distinct from the current pass input."),
        NodeOp::ImageAsset{..} => ("随节点图携带的 sRGB 图片；通过采样图像读取，像素坐标使用图片自身尺寸。","An sRGB image carried with the graph. Read with Sample Image; pixel coordinates use the image's own dimensions."),
        NodeOp::SampleImage{coordinates:SampleCoordinates::Pixels} => ("按图层逻辑像素位置采样宿主图像，输出图契约中的 RGBA Color。","Samples a host image at layer logical-pixel positions and returns RGBA Color in the graph contract."),
        NodeOp::SampleImage{coordinates:SampleCoordinates::Uv} => ("按归一化 UV 采样宿主图像，输出 RGBA Color；不能直接输入已知像素位置。","Samples a host image with normalized UV and returns RGBA Color; known pixel positions cannot directly feed this mode."),
        NodeOp::Float{..} => ("有限 Float 常量，范围 ±1e6；数值自身未声明单位。","Finite Float literal within ±1e6; the value itself declares no units."),
        NodeOp::Vec2{..} => ("普通二维数值向量。它可用作明确的坐标常量，但不会自动被标为像素位置或 UV。","Generic two-component numeric vector. It can supply an explicit coordinate literal but is not automatically labeled pixel Position or UV."),
        NodeOp::Vec3{..} => ("普通三维数值向量；N1 未提供网格位置、法线、深度或 3D 场景执行上下文。","Generic three-component numeric vector; N1 supplies no mesh position, normal, depth or 3D scene execution context."),
        NodeOp::Color{..} => ("所声明工作空间中的 straight RGBA 常量，各通道在 0..1；图的预乘契约由编译器处理。","Straight RGBA literal in the declared working space, with channels in 0..1; the compiler handles the graph's premultiplied contract."),
        NodeOp::Bool{..} => ("布尔开关常量，用于 Select 等条件输入；不隐式变为 Float。","Boolean literal for conditions such as Select; does not implicitly become Float."),
        NodeOp::Parameter{..} => ("通过稳定参数 ID 读取 SDK 公开参数；默认值、范围和动画声明来自文档参数定义。","Reads an SDK public parameter by stable ID; default, range and animation declaration come from the document definition."),
        NodeOp::Math{operation,..} => match operation {
            MathOp::Add => ("相同数值形状逐分量相加；无隐式广播，结果不传播或证明坐标单位。","Adds same-shape values componentwise; no implicit broadcast or unit propagation/proof."),
            MathOp::Subtract => ("逐分量计算 A−B；输入形状相同，输出单位未声明。","Subtracts B from A componentwise; input shapes match and output units are unspecified."),
            MathOp::Multiply => ("相同形状逐分量相乘；Float 乘 Vec2 需先 Splat。输出不证明像素/UV 单位。","Multiplies same-shape values componentwise; Float × Vec2 needs Splat first. Output does not prove pixel/UV units."),
            MathOp::Divide => ("逐分量计算 A/B；分母绝对值 ≤1e−6 时返回零。输出不传播单位。","Divides A by B componentwise; denominator magnitude ≤1e−6 returns zero. Units are not propagated."),
            MathOp::Min => ("逐分量取较小值，输入形状相同；输出单位未声明。","Takes componentwise minima of matching shapes; output units are unspecified."),
            MathOp::Max => ("逐分量取较大值，输入形状相同；输出单位未声明。","Takes componentwise maxima of matching shapes; output units are unspecified."),
            MathOp::Abs => ("逐分量取绝对值；普通数值运算不传播距离或坐标单位。","Takes componentwise absolute values; generic numeric operations do not propagate distance/coordinate units."),
            MathOp::Sin => ("逐分量计算正弦，数值按弧度解释；输出为普通标量/向量。","Computes componentwise sine, interpreting values as radians; output is a generic scalar/vector."),
            MathOp::Cos => ("逐分量计算余弦，数值按弧度解释；输出为普通标量/向量。","Computes componentwise cosine, interpreting values as radians; output is a generic scalar/vector."),
            MathOp::Sqrt => ("逐分量开平方；负数返回零，输出单位未声明。","Takes componentwise square roots; negative inputs return zero and output units are unspecified."),
            MathOp::Floor => ("逐分量向下取整；输出单位未声明。","Rounds each component down; output units are unspecified."),
            MathOp::Fract => ("输出 value−floor(value) 的重复小数部分，用于周期图案；不传播单位。","Returns the repeating fractional part value−floor(value) for periodic patterns; units are not propagated."),
            MathOp::Clamp => ("将 value 限制到排序后的 min/max 之间；形状相同，输出单位未声明。","Clamps value between sorted min/max bounds of matching shape; output units are unspecified."),
            MathOp::Lerp => ("用 Float factor 在同形状 A/B 之间插值，允许超出 0..1 外插；不传播单位。","Interpolates matching-shape A/B using Float factor, allowing extrapolation outside 0..1; units are not propagated."),
            MathOp::Smoothstep => ("排序两个端点后产生平滑过渡；端点重合时返回阶跃，输出单位未声明。","Produces a smooth transition between sorted endpoints; equal endpoints return a step and output units are unspecified."),
            MathOp::Dot => ("对同形状 Vec2/Vec3 求点积，返回普通 Float；未跟踪相乘后的单位。","Takes the dot product of matching Vec2/Vec3 inputs, returning generic Float; product units are not tracked."),
            MathOp::Length => ("计算 Vec2/Vec3 的欧氏长度，返回普通 Float；未跟踪输入单位。","Returns the Euclidean length of Vec2/Vec3 as generic Float; input units are not tracked."),
            MathOp::Normalize => ("归一化 Vec2/Vec3；零向量返回零，输出为普通向量且不证明坐标语义。","Normalizes Vec2/Vec3; zero vectors return zero, and output remains a generic vector without proven coordinate meaning."),
        },
        NodeOp::Splat{..} => ("显式把一个 Float 复制到 Vec2/Vec3 各分量；结果不自动声明坐标单位。","Explicitly replicates a Float into all Vec2/Vec3 components; the result does not automatically declare coordinate units."),
        NodeOp::CombineVec2|NodeOp::CombineVec3 => ("显式组合各个 Float 分量，得到普通数值向量；不假设位置或尺寸单位。","Explicitly combines Float components into a generic numeric vector without assuming position or size units."),
        NodeOp::CombineColor => ("显式组合 R/G/B/Alpha 四个 Float 通道得到 Color。","Explicitly combines R/G/B/Alpha Float channels into Color."),
        NodeOp::SplitColor => ("拆分 Color 的 straight R/G/B/Alpha 通道，输出普通 Float。","Splits Color's straight R/G/B/Alpha channels into generic Floats."),
        NodeOp::ColorMath{operation:ColorOp::Invert} => ("反相 straight RGB 并保留 Alpha，再恢复图声明的 Alpha 契约。","Inverts straight RGB, preserves alpha and restores the graph's declared alpha contract."),
        NodeOp::ColorMath{operation:ColorOp::Mix} => ("按限制到 0..1 的 Float 系数逐通道混合两路 Color；不是图层合成器。","Mixes two Colors componentwise using a Float factor clamped to 0..1; not a layer compositor."),
        NodeOp::ColorMath{operation:ColorOp::Multiply|ColorOp::Screen} => ("逐通道执行正片叠底/滤色；输出 Alpha 为两输入 Alpha 的乘积。","Performs componentwise multiply/screen; output alpha is the product of input alphas."),
        NodeOp::ColorMath{operation:ColorOp::SrgbEncode|ColorOp::SrgbDecode} => ("显式 sRGB 编码/解码 RGB；Alpha 不参与色彩传递函数。","Explicitly sRGB-encodes/decodes RGB; alpha does not participate in the transfer function."),
        NodeOp::ApplyMask => ("将限制后的覆盖率乘到 Alpha，保留 straight RGB；不执行图层混合。","Multiplies clamped coverage into alpha, preserving straight RGB; does not perform layer blending."),
        NodeOp::Select{..} => ("根据 Bool 选择两路同类型值；Image 选择需要额外 pass 规划，N1 不支持。","Selects between two same-type values using Bool. Image selection needs additional pass planning and is unsupported by N1."),
        NodeOp::SdfCircle => ("在像素坐标中计算圆的有符号距离：内侧负、边界零、外侧正。","Computes a circle's signed distance in pixel coordinates: negative inside, zero at the boundary, positive outside."),
        NodeOp::SdfRoundedRectangle => ("使用中心、半尺寸和圆角像素半径计算圆角矩形距离；圆角自动限制到半尺寸内。","Computes rounded-rectangle distance using center, half size and pixel corner radius; corners clamp to the half size."),
        NodeOp::SdfLine|NodeOp::SdfCapsule => ("使用同一像素坐标系的两端点计算线段/胶囊距离；Line 使用全宽，Capsule 使用半径。","Computes segment/capsule distance from endpoints in the same pixel coordinates; Line uses full width, Capsule uses radius."),
        NodeOp::SdfUnion|NodeOp::SdfIntersection|NodeOp::SdfDifference => ("组合两个相同像素坐标系的 Distance Pixels 距离场；不是 Float 或颜色混合。","Combines two Distance Pixels fields in the same pixel coordinates; not a Float or color blend."),
        NodeOp::SdfSmoothUnion => ("用像素融合宽度平滑合并两路距离场；宽度为零时回到普通并集。","Smoothly merges two distance fields using a pixel blend width; zero width returns to ordinary union."),
        NodeOp::SdfFill|NodeOp::SdfStroke => ("把 Distance Pixels 转成 0..1 覆盖率，使用片元导数抗锯齿；Stroke 的宽度是像素全宽。","Converts Distance Pixels to 0..1 coverage using fragment derivatives for antialiasing; Stroke width is full pixel width."),
        NodeOp::ColorOutput => ("唯一 RGBA Color 输出，N1 融合成一个 image fragment pass。","The unique RGBA Color output; N1 fuses the graph into one image fragment pass."),
        NodeOp::Unsupported{..} => ("规划功能未实现，N1 会拒绝编译并报告缺失的上下文或 ABI。","Planned capability is unimplemented; N1 rejects compilation and reports the missing context or ABI."),
    }
}
