//! Read-only, embedded learning graphs. Loading yields an independent editable
//! document and performs no I/O, installation or Registry mutation.
use crate::{GraphDocument, Result};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExampleCollection {
    Fundamentals,
    SoftSpace,
    GraphicMotion,
    GlowAtmosphere,
}
impl ExampleCollection {
    pub fn title(self) -> (&'static str, &'static str) {
        match self {
            Self::Fundamentals => ("基础节点", "Fundamentals"),
            Self::SoftSpace => ("柔光空间", "Soft Space"),
            Self::GraphicMotion => ("弹性几何", "Graphic Motion"),
            Self::GlowAtmosphere => ("发光氛围", "Glow Atmosphere"),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct BuiltinExample {
    pub id: &'static str,
    pub name: &'static str,
    pub english_name: &'static str,
    pub description: &'static str,
    pub english_description: &'static str,
    pub lesson: &'static str,
    pub english_lesson: &'static str,
    pub collection: ExampleCollection,
    pub limitations: &'static str,
    pub english_limitations: &'static str,
    #[serde(skip)]
    pub source: &'static str,
}
impl BuiltinExample {
    pub fn load(&self) -> Result<GraphDocument> {
        GraphDocument::from_json(self.source)
    }
}

/// Stable, beginner-to-intermediate order. Canvas positions are part of each
/// actual `.msgraph` source; all shape examples use 64×64 logical pixels.
pub fn builtin_examples() -> &'static [BuiltinExample] {
    &EXAMPLES
}
const EXAMPLES: [BuiltinExample; 18] = [
    BuiltinExample {
        id: "drifting_circle", name: "漂移圆形", english_name: "Drifting Circle",
        description: "公开半径、中心、颜色和速度参数，按局部时间移动半透明圆形。默认尺寸为 64×64。",
        english_description: "Move a translucent circle using local time and public radius, center, color and speed parameters. Default size: 64×64.",
        lesson: "Time 是秒；秒乘 px/s 得到像素偏移。Combine Vec2 明确组合 X/Y，Vec2 Add 后接 SDF Circle，Fill 覆盖率通过 Apply Mask 改变 Alpha。",
        english_lesson: "Time is seconds; seconds × px/s gives a pixel offset. Combine Vec2 forms X/Y explicitly; Vec2 Add feeds SDF Circle, whose Fill coverage changes alpha through Apply Mask.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/drifting-circle.msgraph"),
    },
    BuiltinExample {
        id: "uv_gradient", name: "UV 双色分量渐变", english_name: "UV Component Gradient",
        description: "把归一化 UV 的两个分量映射到红、绿通道，生成不依赖图像素材的二维渐变。",
        english_description: "Map normalized UV components to red and green for a two-dimensional gradient that needs no source image.",
        lesson: "UV 在图层范围内为 0..1，区别于像素 Position。Dot 与 (1,0)/(0,1) 取出 U/V 标量；Combine Color 明确连接四个 Float 通道。",
        english_lesson: "UV is 0..1 across the layer, unlike pixel Position. Dot with (1,0)/(0,1) extracts scalar U/V; Combine Color connects four explicit Float channels.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/uv-gradient.msgraph"),
    },
    BuiltinExample {
        id: "animated_stripes", name: "时间驱动条纹", english_name: "Animated Stripes",
        description: "用 UV、Fract、Smoothstep 和颜色混合生成滚动条纹；公开条纹数量和滚动速度。",
        english_description: "Create scrolling bands with UV, Fract, Smoothstep and Color Mix; expose stripe count and scroll speed.",
        lesson: "U×数量控制重复周期，Time×速度控制相位，Fract 回到 0..1。Smoothstep 在 0.45..0.55 过渡；最终 factor 连接 Color Mix，数值不隐式转成颜色。",
        english_lesson: "U × count sets repetitions, Time × speed sets phase, and Fract returns to 0..1. Smoothstep transitions from 0.45 to 0.55; its scalar factor feeds Color Mix without an implicit color conversion.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/animated-stripes.msgraph"),
    },
    BuiltinExample {
        id: "sdf_ring", name: "圆环描边", english_name: "SDF Ring Stroke",
        description: "对圆的有符号距离描边，得到内部和外部透明的圆环；半径与描边全宽可编辑。",
        english_description: "Stroke a circle's signed distance to make a transparent-centered ring with editable radius and full stroke width.",
        lesson: "distance_pixels 与普通 Float 不同。Circle 的零距离是边界，Stroke 围绕边界铺开全宽；覆盖率送入 Apply Mask，中心保持透明。",
        english_lesson: "distance_pixels differs from ordinary Float. Circle's zero distance is the boundary; Stroke places its full width around it. Apply Mask uses coverage while the center stays transparent.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/sdf-ring.msgraph"),
    },
    BuiltinExample {
        id: "rounded_button", name: "圆角按钮轮廓", english_name: "Rounded Button Shape",
        description: "绘制可调半尺寸、圆角和颜色的按钮形圆角矩形。它是图像效果轮廓，不是交互控件。",
        english_description: "Draw a button-shaped rounded rectangle with adjustable half size, corner radius and color. This is an image shape, not an interactive widget.",
        lesson: "half_size 是半尺寸：默认 (24,12) 对应 48×24 像素。圆角限制在半尺寸内；Fill 产生抗锯齿覆盖率，透明背景由 Apply Mask 得到。",
        english_lesson: "half_size is half the dimensions: (24,12) means 48×24 pixels. Corners clamp to the half size; Fill gives antialiased coverage, and Apply Mask provides a transparent background.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/rounded-button.msgraph"),
    },
    BuiltinExample {
        id: "smooth_union", name: "平滑融合距离场", english_name: "Smooth SDF Union",
        description: "两个圆共享半径，通过可调融合宽度形成柔和连接的形状。",
        english_description: "Blend two circles with a shared radius into a softly connected shape using an adjustable blend width.",
        lesson: "两路 Distance Pixels 才能接 Smooth Union。smoothness 单位是像素，设为 0 回到普通并集；先融合距离，再 Fill，而非混合两张颜色图。",
        english_lesson: "Smooth Union requires two Distance Pixels inputs. smoothness is in pixels; zero returns to ordinary union. Blend distances before Fill, rather than mixing colored images.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/smooth-union.msgraph"),
    },
    BuiltinExample {
        id: "image_invert", name: "输入图像反相", english_name: "Image Invert",
        description: "采样当前效果链输入图像并反相 RGB，保留原有透明度；桌面预览使用明确的示例素材。",
        english_description: "Sample the current effect-chain input and invert RGB while keeping its alpha; desktop preview uses the labeled sample image.",
        lesson: "Image 是宿主句柄，不是 Color 或任意外部纹理。UV 必须接 UV 模式 Sample Image；采样得到 Color 后才能反相，Invert 不反转 Alpha。",
        english_lesson: "Image is a host handle, not Color or an arbitrary external texture. UV feeds Sample Image in UV mode. Only the sampled Color can be inverted; Invert does not invert alpha.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/image-invert.msgraph"),
    },
    BuiltinExample {
        id: "parameter_tint", name: "公开参数调色", english_name: "Public Parameter Tint",
        description: "读取原始图像，将着色颜色、强度和启用开关暴露为 SDK 公开参数。",
        english_description: "Read the original source and expose tint color, strength and an enable switch as SDK public parameters.",
        lesson: "Color Multiply 逐通道相乘，Color Mix 用 Float 强度插值，Select 用 Bool 在两路 Color 中选择。修改参数默认值并导出 .msfx，保留稳定 ID 与可动画声明。",
        english_lesson: "Color Multiply multiplies components; Color Mix interpolates using Float strength; Select uses Bool to choose between two Colors. Edit defaults and export .msfx while retaining stable IDs and animation declarations.",
        collection: ExampleCollection::Fundamentals,
        limitations: "单图像 Pass 学习图；示例验收画布为 64×64。",
        english_limitations: "Single image-pass learning graph; reference canvas is 64×64.",
        source: include_str!("../examples/parameter-tint.msgraph"),
    },
    BuiltinExample {
        id: "soft_sphere",
        name: "柔光球体与接触光影",
        english_name: "Soft Sphere and Contact Shading",
        description: "解析半球法线、定向漫反射和椭圆渐变阴影，营造克制的白色空间；半径、光方向、颜色、漂浮速度和阴影强度可调。",
        english_description: "Use analytic hemisphere normals, directional diffuse shading and an elliptical shadow gradient to create a restrained white space. Expose radius, light direction, tone, drift and shadow strength.",
        lesson: "先用 Position 减中心再除半径，得到无量纲法线 XY；sqrt(max(1-r²,0)) 得到 Z。Dot 光方向控制颜色插值，SDF Fill 控制轮廓。阴影是独立二维渐变。",
        english_lesson: "Subtract center from Position and divide by radius for dimensionless normal XY; sqrt(max(1-r²,0)) gives Z. A light-direction Dot controls color interpolation; SDF Fill controls the outline. The shadow is a separate 2D gradient.",
        limitations: "这是屏幕空间的解析着色学习图，未生成深度图，也不是光线追踪、真实接触阴影或碰撞模拟。",
        english_limitations: "An analytic screen-space shading study: no depth map, ray tracing, physical contact shadow or collision simulation.",
        collection: ExampleCollection::SoftSpace,
        source: include_str!("../examples/soft-sphere.msgraph"),
    },
    BuiltinExample {
        id: "soft_sphere_motion",
        name: "柔光球体 · 伪3D运动",
        english_name: "Soft Sphere Pseudo-3D Motion",
        description: "在柔光球体基础上加入循环绕行、近大远小、平滑抬落和斜轴表面细环旋转；阴影跟随落点、尺度和高度变化。运动与材质参数可调。",
        english_description: "Extend the soft matte sphere with a looping orbit, perspective size changes, smooth lift and a rotating tilted surface band. The shadow follows the ground position, scale and height; expose motion and material controls.",
        lesson: "Time×速度生成周期相位；Sin 控制横移，Cos 控制纵深，1/(1+纵深) 同时投影位置和半径。抬起高度控制阴影扩散与浓淡；解析半球法线与旋转轴点乘生成细环，展示表面旋转。速度设为 0 可分别停止绕行或自转，细环强度设为 0 回到纯色球体。",
        english_lesson: "Time × speed gives the phase. Sin drives horizontal travel; Cos gives depth, and 1/(1+depth) projects position and radius together. Lift controls shadow spread and fading. Dot analytic hemisphere normals with a rotating axis to show a surface band. Zero speeds stop orbit or spin independently; zero band strength restores the plain sphere.",
        limitations: "这是解析球面与二维阴影的伪3D学习图，未生成场景深度，不是真实网格、光线追踪、接触阴影或碰撞模拟；参考画布为 64×64。",
        english_limitations: "A pseudo-3D study using an analytic sphere and 2D shadow, without scene depth, meshes, ray tracing, physical contact shadows or collision simulation. Reference canvas: 64×64.",
        collection: ExampleCollection::SoftSpace,
        source: include_str!("../examples/soft-sphere-motion.msgraph"),
    },
    BuiltinExample {
        id: "translucent_card",
        name: "半透卡片与柔和高光",
        english_name: "Translucent Card and Soft Rim",
        description: "漂浮的圆角卡片与白色边缘高光，叠在内置渐变背景上；半尺寸、圆角、颜色和覆盖强度可调。",
        english_description: "Float a rounded card with a pale rim above a generated gradient background; adjust half size, corner radius, tone and coverage.",
        lesson: "Round Rectangle 输出 Distance Pixels，分别送 Fill 和 Stroke。Fill×覆盖强度混合背景与卡片，再用描边覆盖率加高光。Time 控制 Y 偏移。",
        english_lesson: "Rounded Rectangle outputs Distance Pixels for both Fill and Stroke. Fill × coverage blends background and card, then stroke coverage adds the rim. Time drives Y offset.",
        limitations: "半透只针对本图生成的背景，不会模糊或折射外部画面；不是毛玻璃材质或真实薄片 3D。",
        english_limitations: "Translucency applies to this generated background only; it does not blur or refract an external image or create a physical frosted material or 3D sheet.",
        collection: ExampleCollection::SoftSpace,
        source: include_str!("../examples/translucent-card.msgraph"),
    },
    BuiltinExample {
        id: "elastic_capsule",
        name: "阻尼回弹与挤压拉伸",
        english_name: "Damped Bounce and Squash Stretch",
        description: "周期性回弹的胶囊形状；解析振荡和衰减包络驱动位移，尺寸相反联动营造挤压拉伸感。",
        english_description: "Bounce a capsule-like shape: analytic oscillation and a decay envelope drive position, while inverse size coupling creates squash and stretch.",
        lesson: "Fract(Time/周期) 是触发相位。sin(4πt)×(1-t)² 是衰减回弹；宽乘 stretch、高除 stretch，保持二维面积近似不变。动画由 shader 局部时间驱动。",
        english_lesson: "Fract(Time/period) is trigger phase. sin(4πt) × (1-t)² forms a damped bounce; multiply width and divide height by stretch to approximately preserve 2D area. Animation uses local shader time.",
        limitations: "解析动画不是刚体物理，也不是时间轴关键帧缓动；每次周期开始会再次触发回弹。",
        english_limitations: "Analytic animation, not rigid-body physics or timeline-keyframe easing; every loop restarts the bounce.",
        collection: ExampleCollection::GraphicMotion,
        source: include_str!("../examples/elastic-capsule.msgraph"),
    },
    BuiltinExample {
        id: "graphic_cutout",
        name: "动态图形布尔裁切",
        english_name: "Animated Graphic Boolean Cutout",
        description: "高对比圆角几何体被移动圆形裁切，展示距离场差集与鲜明撞色。",
        english_description: "Cut a moving circle out of a high-contrast rounded shape to study distance-field difference and a bold accent palette.",
        lesson: "先分别构造两路 Distance Pixels，Difference 后再 Fill；不是直接混合两张颜色图。Time×速度→Sin 控制裁切中心，半径和颜色可改。",
        english_lesson: "Construct two Distance Pixels branches, apply Difference and then Fill rather than mixing colors. Time × speed → Sin moves the cutter; radius and color are editable.",
        limitations: "只生成本效果的二维图形，不会剪裁工程中的另一图层，也不建立模型网格。",
        english_limitations: "Generates this effect's 2D shape only; it does not crop another project layer or generate a model mesh.",
        collection: ExampleCollection::GraphicMotion,
        source: include_str!("../examples/graphic-cutout.msgraph"),
    },
    BuiltinExample {
        id: "pulse_halo",
        name: "脉动光环与解析光晕",
        english_name: "Pulse Ring and Analytic Halo",
        description: "暗背景上的发光圆环；窄亮核和较宽解析衰减层形成柔和光晕，亮度缓慢脉动。",
        english_description: "A luminous ring against a dark background; a narrow bright core and wider analytic falloff form a soft halo with a slow pulse.",
        lesson: "UV 距离减 radius/64 得到环带距离；1/(1+(d/宽度)²) 的平方形成衰减。SDF Stroke 做亮核，Sin 控制光晕强度。注意 UV 宽度与像素半径不同。",
        english_lesson: "Subtract radius/64 from UV radius for ring distance; squared 1/(1+(d/width)²) gives falloff. SDF Stroke creates the bright core, and Sin controls halo intensity. UV width differs from pixel radius.",
        limitations: "光晕由解析颜色计算得到，不是对输入画面做 Bloom 提取、卷积或真实体积光；参考画布为 64×64。",
        english_limitations: "The halo is analytic color math, not input-image Bloom extraction, convolution or volumetric light; reference canvas is 64×64.",
        collection: ExampleCollection::GlowAtmosphere,
        source: include_str!("../examples/pulse-halo.msgraph"),
    },
    BuiltinExample {
        id: "floating_lights",
        name: "漂浮光斑与层次",
        english_name: "Floating Light Spots",
        description: "三层大小、位置和相位不同的柔和光斑缓慢漂浮；不依赖图片素材，适合作为氛围光背景的起点。",
        english_description: "Three soft light spots with different sizes, positions and phases drift slowly. No source image is required; use it as a starting point for atmospheric light backgrounds.",
        lesson: "每层 UV 减动态中心，Length 得距离，Smoothstep 再平方得到柔和覆盖率；逐层 Color Mix 合成。公开速度、颜色、强度，用相位偏移避免同步运动。",
        english_lesson: "For each layer, subtract a moving center from UV, use Length for distance, and square Smoothstep falloff for soft coverage. Composite with Color Mix; expose speed, tone and strength, and offset phases to avoid synchronized motion.",
        limitations: "光斑是二维解析渐变，不是摄像机景深、粒子模拟、体积雾或次表面散射。",
        english_limitations: "The spots are 2D analytic gradients, not camera depth of field, particle simulation, volumetric fog or subsurface scattering.",
        collection: ExampleCollection::GlowAtmosphere,
        source: include_str!("../examples/floating-lights.msgraph"),
    },
    BuiltinExample {
        id: "solid_color", name: "纯色入门", english_name: "Solid Color",
        description: "两个节点、一条连线：颜色常量直接连接颜色输出。无需素材，适合第一次编辑。",
        english_description: "Two nodes and one wire: a color constant feeds Color Output. No source image is needed for your first edit.",
        lesson: "先选择颜色节点，修改色块或 RGBA 分量，再查看预览。颜色必须连接到 Color Output 才会成为最终图像。",
        english_lesson: "Select Color, change its swatch or RGBA components, then inspect the preview. Connect color to Color Output to make the final image.",
        collection: ExampleCollection::Fundamentals,
        limitations: "输出为均匀纯色，不包含图像素材或动画。",
        english_limitations: "Produces a uniform solid color, without source images or animation.",
        source: include_str!("../examples/solid-color.msgraph"),
    },
    BuiltinExample {
        id: "compact_gradient", name: "三节点渐变", english_name: "Three-node Gradient",
        description: "UV、渐变和输出三个节点，直接调节两端颜色、方向与衰减。",
        english_description: "Three nodes: UV, Gradient and Output. Edit colors, direction and falloff directly.",
        lesson: "渐变同时输出颜色和因子；切换线性、径向或角向模式，使用端口行内默认值，不再铺设常量与混色链。",
        english_lesson: "Gradient outputs both color and factor. Switch linear, radial or conic modes and edit socket defaults instead of adding constant and color-mix chains.",
        collection: ExampleCollection::Fundamentals,
        limitations: "使用归一化 UV，径向不自动校正宽高比；只有双色，无多色标编辑器。",
        english_limitations: "Uses normalized UV without radial aspect correction. Two colors only; no multi-stop editor.",
        source: include_str!("../examples/compact-gradient.msgraph"),
    },
    BuiltinExample {
        id: "compact_noise", name: "三节点噪声", english_name: "Three-node Noise",
        description: "UV、噪声和输出三个节点；切换值噪声与柏林噪声，调节层数、缩放、偏移和种子。",
        english_description: "Three nodes: UV, Noise and Output. Switch Value/Perlin modes and adjust octaves, scale, offset and seed.",
        lesson: "直接输出灰度颜色，或把 0..1 因子连接到颜色混合；最多四层，无需图片素材。",
        english_lesson: "Use grayscale directly, or connect the 0..1 factor to Color Mix. Up to four octaves; no image is required.",
        collection: ExampleCollection::Fundamentals,
        limitations: "二维单 pass 噪声，不是三维体积或随时间自动演变；坐标域受限，种子按整数处理。",
        english_limitations: "2D single-pass noise, not volumetric or automatically time-varying. Bounded coordinate domain and integer-like seeds.",
        source: include_str!("../examples/compact-noise.msgraph"),
    },
];

/// Compact host-facing catalogue. Source documents are opened by stable ID/load(),
/// not copied into every catalogue response.
pub fn builtin_example_catalogue() -> serde_json::Value {
    let collections = [
        ExampleCollection::Fundamentals,
        ExampleCollection::SoftSpace,
        ExampleCollection::GraphicMotion,
        ExampleCollection::GlowAtmosphere,
    ];
    serde_json::json!({"version":1,"reference_canvas":[64,64],
        "collections":collections.iter().map(|c|{let (name,english_name)=c.title();serde_json::json!({"id":c,"name":name,"english_name":english_name})}).collect::<Vec<_>>(),
        "examples":builtin_examples()})
}
