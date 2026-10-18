# Motion Studio · 节点与学习示例

从一张圆形距离场开始，逐步连接运动、颜色、光影与图形裁切。`motion-graph` 提供可编辑的 `.msgraph` 文档、类型与语义校验、便携 WGSL 编译，以及真实的 `.msfx` 导出。

这是共享后端。桌面编辑器可调用这些接口；Android 执行导出的效果包，无需节点编辑器。本 crate 不依赖界面、编辑会话或渲染器，GPU 集成只用于开发工具与测试。

## 三种创作方向

| 示例集 | 先学什么 | 示例 |
| --- | --- | --- |
| **柔光空间 · Soft Space** | 用距离场和解析法线塑造体积，用低对比颜色建立空间感 | 柔光球体、半透卡片 |
| **弹性几何 · Graphic Motion** | 将时间变成阻尼回弹，再用布尔裁切组织图形 | 挤压拉伸、动态图形差集 |
| **发光氛围 · Glow Atmosphere** | 从径向距离得到柔和光晕，通过相位差组织运动 | 脉动光环、漂浮光斑 |

加上八个基础案例，内置库共 **14 个独立、可导出的节点图**。每个案例提供中文与英文的简介、连线教学和能力边界。具体参数与学习顺序见[示例指南](examples/README.md)。

这些案例是原创的技术练习，不包含第三方作品、素材或作者署名授权。它们展示二维程序化近似；真实深度、物理软阴影、图像 Bloom、景深和体积光需要后续渲染能力。

## 提供给前端的接口

```rust
use motion_graph::{builtin_example_catalogue, builtin_examples, compile, CompileOptions};

// 小型 JSON 目录：分类、稳定 ID、双语名称、教学与限制，不携带文档源码
let catalogue = builtin_example_catalogue();

// 每次读取返回独立文档，编辑副本不会改变内置模板
let example = builtin_examples().iter().find(|e| e.id == "soft_sphere").unwrap();
let graph = example.load()?;
let compiled = compile(&graph, &CompileOptions::default())?;
# Ok::<(), motion_graph::Error>(())
```

- `GraphDocument::{load,from_json,save,to_json}`: read/write `.msgraph` versions 1 and 2; typed input defaults require version 2
- `node_ports`：获取引脚的 WGSL 数据类型
- `node_port_specs` / `node_description`：获取双语引脚说明、像素位置、UV、距离、颜色、秒等语义
- `check_connection` / `validate`：检查连线、必须输入、单一颜色输出、循环依赖、参数与预算
- `compile`：生成 SDK 6 单图像 pass 的 WGSL，复用宿主的 Naga 校验与 GLES 3.0 转译
- `export_msfx`：导出可由 `EffectPackage` 和 `Registry` 加载的包，并保留同名可编辑 `.msgraph`

`CompileReport` 区分图内采样点、宿主采样点、绑定数、循环工作量和单 pass 输出大小。它不是 GPU 耗时或总显存测量。

### Stored input defaults

`Node::input_defaults` maps input port IDs to exact typed literals (`InputValue`). A wire takes precedence; removing it restores the saved literal. Missing upstream wires still produce diagnostics, including wires through group boundaries. Values are finite within ±1e6; colors use normalized straight RGBA and follow the graph's alpha contract during compilation. No numeric casts or implicit image/coordinate inputs are introduced.

Existing constructors and examples remain version 1 with an omitted empty map. Authors using stored defaults must set `format_version` to `INPUT_DEFAULTS_FORMAT_VERSION`; the desktop editor upgrades the root automatically, including edits inside groups. Version 1 files containing defaults and unsupported future versions are rejected. Exported SDK 6 effects keep their existing manifest and execution ABI. Defaults on group boundary nodes are not yet supported; internal leaf defaults are supported.

`authoring_input_defaults` supplies independently authored initial values for common numeric inputs. New desktop nodes use these values; loading an older unfinished node does not silently fill its required inputs. Existing examples retain their explicit constant nodes.

## 节点范围

当前包含时间、分辨率、像素位置、UV、宿主图像采样、自定义参数、数值与向量运算、颜色运算，以及圆、圆角矩形、线段、胶囊、距离场布尔与填充描边。

图像采样坐标必须区分像素与 UV。已声明的单位不兼容会拒绝连接，通用数学向量的单位仍需要作者判断；语义检查不是完整的量纲推导。

当前目标是一个颜色输出、一个图像 pass，不支持场景深度、网格材质、AI 推理、体积渲染、多 pass 或任意 WGSL 代码节点。遇到不支持的节点会明确报错，不会自动剔除或悄悄改变效果。

## 运行与验证

在仓库根目录运行：

```sh
cargo test -p motion-graph --locked -- --test-threads=1
cargo run -p motion-graph --locked --example export_demo -- target/node-demo/drifting-circle.msfx
cargo run -p motion-graph --locked --example render_learning_examples -- target/node-learning
```

最后一个命令需要可用的 WGPU 适配器，输出六个风格案例在第 0 与 15.5 帧的 PNG、可编辑源码、效果包、目录和编译报告。测试检查全部 14 个案例的类型、连线、导出与重新加载，并通过真实宿主 GPU 路径检查动画、透明度和公开参数。

参考画布为 **64×64**。部分案例使用固定像素坐标，不能直接把它们当作适配任意画幅的成品预设。改变画布时，应同时调整像素中心、尺寸和 UV 到像素的比例。移动端持续性能与兼容性仍需真机验收。
