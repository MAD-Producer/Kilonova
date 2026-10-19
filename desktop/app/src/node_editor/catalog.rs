use super::*;
/// Labels are presentation metadata; stable node and parameter IDs stay intact.
pub(super) fn node_title(
    node: &Node,
    parameters: &[PublicParameter],
    language: Language,
) -> String {
    if !node.display_name.is_empty() {
        return node.display_name.clone();
    }
    if let NodeOp::Parameter { parameter } = &node.operation {
        if let Some(parameter) = parameters.iter().find(|p| p.id == *parameter) {
            return language.text(&parameter.name, &parameter.english_name).to_owned();
        }
    }
    operation_title(&node.operation, language)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Input,
    Value,
    Math,
    Color,
    Sdf,
    Output,
}
impl Category {
    pub fn title(self, language: Language) -> &'static str {
        match self {
            Self::Input => language.text("输入", "INPUT"),
            Self::Value => language.text("值与参数", "VALUES & PARAMETERS"),
            Self::Math => language.text("数学", "MATH"),
            Self::Color => language.text("颜色", "COLOR"),
            Self::Sdf => language.text("距离场", "2D SIGNED DISTANCE"),
            Self::Output => language.text("输出", "OUTPUT"),
        }
    }
}
#[derive(Clone, Debug)]
pub struct LibraryItem {
    pub category: Category,
    pub operation: NodeOp,
    pub parameter_type: Option<ValueType>,
}
impl LibraryItem {
    pub fn title(&self, language: Language) -> String {
        if let Some(ty) = self.parameter_type {
            format!(
                "{} {}",
                language.text("公开参数", "Parameter"),
                type_label(ty)
            )
        } else {
            operation_title(&self.operation, language)
        }
    }
}
pub fn catalog() -> Vec<LibraryItem> {
    let mut items = vec![];
    let mut add = |category, operation| {
        items.push(LibraryItem {
            category,
            operation,
            parameter_type: None,
        })
    };
    for op in [
        NodeOp::Position,
        NodeOp::Uv,
        NodeOp::Time,
        NodeOp::FrameIndex,
        NodeOp::Resolution,
        NodeOp::Image {
            source: ImageSource::Input,
        },
        NodeOp::Image {
            source: ImageSource::Source,
        },
        NodeOp::SampleImage {
            coordinates: SampleCoordinates::Pixels,
        },
    ] {
        add(Category::Input, op);
    }
    for op in [
        NodeOp::Float { value: 1. },
        NodeOp::Vec2 { value: [0.; 2] },
        NodeOp::Vec3 { value: [0.; 3] },
        NodeOp::Color {
            value: [0.12, 0.65, 1., 1.],
        },
        NodeOp::Bool { value: false },
    ] {
        add(Category::Value, op);
    }
    for op in [
        MathOp::Add,
        MathOp::Subtract,
        MathOp::Multiply,
        MathOp::Divide,
        MathOp::Min,
        MathOp::Max,
        MathOp::Abs,
        MathOp::Sin,
        MathOp::Cos,
        MathOp::Sqrt,
        MathOp::Floor,
        MathOp::Fract,
        MathOp::Clamp,
        MathOp::Lerp,
        MathOp::Smoothstep,
        MathOp::Dot,
        MathOp::Length,
        MathOp::Normalize,
    ] {
        add(
            Category::Math,
            NodeOp::Math {
                operation: op,
                value_type: if matches!(op, MathOp::Dot | MathOp::Length | MathOp::Normalize) {
                    ValueType::Vec2
                } else {
                    ValueType::Float
                },
            },
        );
    }
    for op in [
        NodeOp::Splat {
            value_type: ValueType::Vec2,
        },
        NodeOp::CombineVec2,
        NodeOp::CombineVec3,
        NodeOp::Select {
            value_type: ValueType::Float,
        },
    ] {
        add(Category::Math, op);
    }
    for mode in [GradientMode::Linear, GradientMode::Radial, GradientMode::Conic] {
        add(Category::Color, NodeOp::Gradient { mode });
    }
    for mode in [NoiseMode::Value, NoiseMode::Perlin] {
        add(Category::Color, NodeOp::Noise { mode, octaves: 1 });
    }
    add(Category::Color, NodeOp::Blend { mode: ColorBlendMode::Normal });
    for op in [NodeOp::CombineColor, NodeOp::SplitColor, NodeOp::ApplyMask] {
        add(Category::Color, op);
    }
    for op in [
        ColorOp::Invert,
        ColorOp::Mix,
        ColorOp::Multiply,
        ColorOp::Screen,
        ColorOp::SrgbEncode,
        ColorOp::SrgbDecode,
    ] {
        add(Category::Color, NodeOp::ColorMath { operation: op });
    }
    for op in [
        NodeOp::SdfCircle,
        NodeOp::SdfRoundedRectangle,
        NodeOp::SdfLine,
        NodeOp::SdfCapsule,
        NodeOp::SdfUnion,
        NodeOp::SdfIntersection,
        NodeOp::SdfDifference,
        NodeOp::SdfSmoothUnion,
        NodeOp::SdfFill,
        NodeOp::SdfStroke,
    ] {
        add(Category::Sdf, op);
    }
    add(Category::Output, NodeOp::ColorOutput);
    for ty in [
        ValueType::Float,
        ValueType::Vec2,
        ValueType::Vec3,
        ValueType::Color,
        ValueType::Bool,
    ] {
        items.push(LibraryItem {
            category: Category::Value,
            operation: NodeOp::Parameter {
                parameter: String::new(),
            },
            parameter_type: Some(ty),
        });
    }
    items.sort_by_key(|item| match item.category {
        Category::Input => 0,
        Category::Value => 1,
        Category::Math => 2,
        Category::Color => 3,
        Category::Sdf => 4,
        Category::Output => 5,
    });
    items
}
pub(super) fn type_label(ty: ValueType) -> &'static str {
    match ty {
        ValueType::Float => "Float",
        ValueType::Vec2 => "Vec2",
        ValueType::Vec3 => "Vec3",
        ValueType::Color => "RGBA",
        ValueType::Bool => "Bool",
        ValueType::Image => "Image",
        ValueType::DistancePixels => "Distance · px",
    }
}
pub(super) fn operation_title(op: &NodeOp, language: Language) -> String {
    if let NodeOp::Group { graph } = op {
        return language.text(&graph.name, &graph.english_name).to_string();
    }
    let titles = match op {
        NodeOp::Math { operation, .. } => match operation {
            MathOp::Add => ("相加", "Add"),
            MathOp::Subtract => ("相减", "Subtract"),
            MathOp::Multiply => ("相乘", "Multiply"),
            MathOp::Divide => ("相除", "Divide"),
            MathOp::Min => ("最小值", "Minimum"),
            MathOp::Max => ("最大值", "Maximum"),
            MathOp::Abs => ("绝对值", "Absolute"),
            MathOp::Sin => ("正弦", "Sine"),
            MathOp::Cos => ("余弦", "Cosine"),
            MathOp::Sqrt => ("平方根", "Square Root"),
            MathOp::Floor => ("向下取整", "Floor"),
            MathOp::Fract => ("小数部分", "Fraction"),
            MathOp::Clamp => ("限制范围", "Clamp"),
            MathOp::Lerp => ("线性插值", "Lerp"),
            MathOp::Smoothstep => ("平滑阶跃", "Smoothstep"),
            MathOp::Dot => ("点积", "Dot"),
            MathOp::Length => ("长度", "Length"),
            MathOp::Normalize => ("归一化", "Normalize"),
        },
        NodeOp::Noise { mode, .. } => match mode {
            NoiseMode::Value => ("值噪声", "Value Noise"),
            NoiseMode::Perlin => ("柏林噪声", "Perlin Noise"),
        },
        NodeOp::Gradient { mode } => match mode {
            GradientMode::Linear => ("线性渐变", "Linear Gradient"),
            GradientMode::Radial => ("径向渐变", "Radial Gradient"),
            GradientMode::Conic => ("角向渐变", "Conic Gradient"),
        },
        NodeOp::ColorMath { operation } => match operation {
            ColorOp::Invert => ("反相", "Invert"),
            ColorOp::Mix => ("颜色混合", "Color Mix"),
            ColorOp::Multiply => ("正片叠底", "Multiply"),
            ColorOp::Screen => ("滤色", "Screen"),
            ColorOp::SrgbEncode => ("sRGB编码", "sRGB Encode"),
            ColorOp::SrgbDecode => ("sRGB解码", "sRGB Decode"),
        },
        NodeOp::Image {
            source: ImageSource::Input,
        } => ("当前图层输入", "Layer Input"),
        NodeOp::Image {
            source: ImageSource::Source,
        } => ("效果原始图像", "Original Source"),
        _ => op.title(),
    };
    language.text(titles.0, titles.1).into()
}
