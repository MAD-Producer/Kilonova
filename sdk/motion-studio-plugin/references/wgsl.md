# WGSL 图像效果

pass 用 `shader` 指定包内文件、`entry` 指定函数。普通图像函数为：

```wgsl
fn main_fx(p: vec2<f32>) -> vec4<f32> {
    return sample_input(p);
}
```

宿主提供入口、纹理、采样器与布局。不要声明额外绑定、顶点/片段/计算入口。shader 最多 256 KiB；循环使用静态边界，每循环最多 1024 次，嵌套静态工作量最多 65536。颜色结果遵循声明的色彩与 Alpha 契约。

`sample_input(p)` 读取上一 pass，`sample_source(p)` 读取本效果的原始输入；`curve_lookup(v)` 读取派生 LUT。边缘采样由宿主处理。

| 字段 | 含义 |
| --- | --- |
| `fx.params[i]` | manifest 第 i 项参数，四浮点槽 |
| `fx.clock` | 局部秒、局部帧、pass 索引、种子 |
| `fx.region` | 输出 `[x,y,width,height]` |
| `fx.input_region` | 上一 pass 输入矩形 |
| `fx.source_region` | 本效果原始输入矩形 |
| `fx.size` | 原图层宽高、输出纹理宽高 |
| `fx.output_mode.w` | 预览采样比例 |

位置、半径和边界使用未变换图层像素，原点在原图层左上角，Y 向下；预览降分辨率不改逻辑单位。输出通过 `padding` 或 SDK 3+ 的 `output_bounds` 声明，不能混用。SDK 4 增加除法、最小值、欧氏长度、平方根、正弦和惰性选择等运算。

空间效果声明完整输出矩形，并检查反向映射是否仍在有限输入平面中。外扩不改锚点、空间属性或关键帧；后续效果保留矩形原点。设备资源超限时宿主明确报错。

粒子和镜头生成器需要对应宿主 renderer/scene 能力。普通图像 pass 不提供任意图层引用、跨时间取帧或可执行原生 UI 代码，先核对能力再实现依赖这些输入的效果。

## Numeric curve LUT contract (ABI 2)

`curve_lookup(v)` clamps each input component to the authored unit interval and
linearly interpolates numeric taps explicitly. It does not apply a color transfer
or premultiply LUT values. Its fourth component is the independent alpha transfer.
This function does not provide HDR extrapolation or general color management.

UNORM8 execution uses 256 RGBA8 taps (1024 bytes per table). UNORM16, Float16
and Float32 execution use 4081 RGBA32Float taps (65296 bytes). Each of the 255
preview intervals has sixteen subdivisions, preserving every preview point when
freezing a curve. Dense values come directly from authored transfers or the
explicit stored grid; upsampling a 256-value frozen grid cannot restore detail.

Native tables require sampled/copy usage without F32 filtering or blending.
GLES uses nearest numeric texels with the same shader interpolation; attachment
extensions required by floating scratch outputs do not apply to sample-only LUTs.
Unsupported dimensions or allocation budgets are errors, never 8-bit substitutes.

Packed frame-plan version 12 keeps the 160-byte header. Word 152 is working
precision; word 156 is LUT encoding (0 = RGBA8/256, 1 = RGBA32Float/4081).
LUT offsets use that encoding's stride. The Host SDK metadata publishes
`curveLutAbi`, `curveLutEncodings` and `sourceTextureBudgetBytes`. Consumers must
validate encoding, precision, ranges and version together before reading data.
