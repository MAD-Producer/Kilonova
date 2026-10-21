# 当前工程格式契约验收

本轮仅修改工程 JSON 的读写契约。保留必填 `Project.version`，使用公开的 `CURRENT_PROJECT_VERSION = 23`；版本 0–22 和未来版本均拒绝，不做迁移。版本增加用于标识必填字段变化，不表示渲染语义变化。新建、保存、加载和命令编辑共用同一版本；删除编辑时按特性抬升版本和矢量 `required_format()`。

## 范围与协作边界

工作分支 `refactor/project-format-contract` 从 #388 的 `3ef29ab9` 创建。开始前核对工作树和远端 PR：精度工作在 `feat/semantic-resource-contracts`，其模型曲线与渲染改动由其他工作树处理。本分支仅删除 `effects.rs` 的两个反序列化默认，不采用该分支的曲线计算或渲染改动。仓库内工程 fixture 主要通过 Rust 构造并写入临时目录；此基点没有受 Git 管理的 `project.json` / `.msproj`。

未改变 `.msfx`、字体、视频元数据、依赖、着色器、合成、蒙版或栅格算法。渲染测试中只将旧的工程版本赋值改为当前常量；不更新像素期望、包字节断言或图像基线。工程编码变化与像素基线成本分开记录；不声称重跑了所有 artifacts。

## 默认值逐项处置

本基点 `motion-model/src` 共 96 个 `serde(default...)` 属性，删除 40 个，保留 56 个（统计单位为属性，不按源代码行数；一行可能有两个属性）。必填字段同步取消 `skip_serializing_if`，避免程序写出的文件自己读不回来。

| 处置 | 字段与语义 |
| --- | --- |
| 改为必填 | Project.composition_id；Layer.three_d/sampling/rotation_order/blend/audio_source_channel；Camera.created/film_width_mm |
| 改为必填 | Keyframe.ease；Curve.space；Quadratic/Cubic.start/end；Easing.ease；CurveObject.interpolation；EffectImageInput.Layer.stage；PropertyExpression.seed |
| 改为必填 | AudioClip.volume/muted；VideoClip.source_offset_us/volume/muted；CompositionClip.source_start_frame/volume/muted；AudioMarker.generated/corrected |
| 改为必填 | LayerBlend.mode/space；TrackMatte.mode/hide_source；LayerMask.mode/enabled/inverted/opacity/feather/expansion；LayerSampling.frame_blend/continuous_rasterization |
| 保留可选引用 | 无父级、无轨道遮罩、无单独图层范围/音量包络、无自定义曲线/空间切线/表达式维度、无虚线/修剪、无运动模糊、无场景/节点/图像覆盖时，None 即对应功能缺席 |
| 保留可选集合 | 无子合成、字体、音视频资产、效果、蒙版、标记、插件依赖、表达式时，空集合即没有该内容；非空依赖仍与实例精确校验 |
| 保留可选 LUT | CurveObject.channel_luts 缺失代表各通道均无覆盖；sampled_lut 缺失代表使用已显式保存的通道与插值，不重新推断插值 |
| 保留命令策略 | Commands/MaskAction/AudioLevelsAction/VectorAction 的 Option 表示只修改指定字段或沿用当前动画状态；摄影机命令 object=0 是现有明确目标；CompositionSettings 的时间与裁剪策略、Reference.at_frame、Precompose.range 是命令缺参语义，不是工程迁移 |

以下为全部保留属性，便于后续审查而不是仅报告数量：

| 文件 | 字段 / 属性 |
| --- | --- |
| `crates/motion-model/src/animation.rs:87` | `pub incoming: Option<T>,` |
| `crates/motion-model/src/animation.rs:89` | `pub outgoing: Option<T>,` |
| `crates/motion-model/src/animation.rs:99` | `pub curve: Option<Curve>,` |
| `crates/motion-model/src/animation.rs:102` | `pub spatial: Option<SpatialTangents<T>>,` |
| `crates/motion-model/src/audio_levels.rs:63` | `animated: Option<bool>,` |
| `crates/motion-model/src/camera.rs:18` | `pub parent: Option<crate::ParentLink>,` |
| `crates/motion-model/src/commands.rs:48` | `SetLayerBlend {object:u64,#[serde(default)]mode:Option<crate::compositing::BlendMode>,#[serde(default)]space:Option<crate::compositing::BlendSpace>},` |
| `crates/motion-model/src/commands.rs:60` | `volume: Option<f32>,` |
| `crates/motion-model/src/commands.rs:62` | `muted: Option<bool>,` |
| `crates/motion-model/src/commands.rs:122` | `axis: Option<crate::Axis>,` |
| `crates/motion-model/src/commands.rs:146` | `axis: Option<crate::Axis>,` |
| `crates/motion-model/src/commands.rs:177` | `axis: Option<crate::Axis>,` |
| `crates/motion-model/src/commands.rs:185` | `axis: Option<crate::Axis>,` |
| `crates/motion-model/src/commands.rs:193` | `axis: Option<crate::Axis>,` |
| `crates/motion-model/src/commands.rs:200` | `axis: Option<crate::Axis>,` |
| `crates/motion-model/src/commands.rs:234` | `object: u64,` |
| `crates/motion-model/src/commands.rs:241` | `object: u64,` |
| `crates/motion-model/src/commands.rs:246` | `object: u64,` |
| `crates/motion-model/src/commands.rs:252` | `object: u64,` |
| `crates/motion-model/src/composition.rs:28` | `pub expressions: Vec<PropertyExpression>,` |
| `crates/motion-model/src/composition.rs:132` | `pub timing: String,` |
| `crates/motion-model/src/composition.rs:134` | `pub shorten: String,` |
| `crates/motion-model/src/composition.rs:151` | `at_frame: u32,` |
| `crates/motion-model/src/composition.rs:157` | `range: String,` |
| `crates/motion-model/src/curve.rs:47` | `pub curve: Option<Curve>,` |
| `crates/motion-model/src/effects.rs:19` | `pub channel_luts: [Option<Vec<f32>>;5],` |
| `crates/motion-model/src/effects.rs:21` | `pub sampled_lut: Option<Vec<[f32; 4]>>,` |
| `crates/motion-model/src/effects.rs:174` | `pub curve: Option<CurveTrack>,` |
| `crates/motion-model/src/effects.rs:318` | `pub image_input: Option<EffectImageInput>,` |
| `crates/motion-model/src/effects.rs:320` | `pub scene: Option<motion_effects::SceneSettings>,` |
| `crates/motion-model/src/effects.rs:322` | `pub node_graph: Option<serde_json::Value>,` |
| `crates/motion-model/src/expressions.rs:12` | `axis: Option<Axis>,` |
| `crates/motion-model/src/masks.rs:229` | `#[serde(default)] mode: Option<MaskMode>,` |
| `crates/motion-model/src/masks.rs:230` | `#[serde(default)] inverted: Option<bool>,` |
| `crates/motion-model/src/masks.rs:231` | `#[serde(default)] enabled: Option<bool>,` |
| `crates/motion-model/src/masks.rs:242` | `#[serde(default)] animated: Option<bool>,` |
| `crates/motion-model/src/model.rs:155` | `pub parent: Option<ParentLink>,` |
| `crates/motion-model/src/model.rs:157` | `pub effects: Vec<crate::EffectInstance>,` |
| `crates/motion-model/src/model.rs:159` | `pub masks: Vec<crate::masks::LayerMask>,` |
| `crates/motion-model/src/model.rs:162` | `pub track_matte:Option<crate::compositing::TrackMatte>,` |
| `crates/motion-model/src/model.rs:164` | `pub timeline: Option<LayerTimeline>,` |
| `crates/motion-model/src/model.rs:166` | `pub subframe_range:Option<crate::ClipRange>,` |
| `crates/motion-model/src/model.rs:168` | `pub audio_levels: Option<crate::AudioLevels>,` |
| `crates/motion-model/src/model.rs:170` | `pub audio_markers: Vec<crate::AudioMarker>,` |
| `crates/motion-model/src/model.rs:267` | `pub compositions: Vec<crate::Composition>,` |
| `crates/motion-model/src/model.rs:276` | `pub fonts: Vec<FontAsset>,` |
| `crates/motion-model/src/model.rs:278` | `pub audio_assets: Vec<crate::AudioAsset>,` |
| `crates/motion-model/src/model.rs:280` | `pub video_assets: Vec<crate::VideoAsset>,` |
| `crates/motion-model/src/model.rs:285` | `pub plugin_dependencies: Vec<crate::PluginDependency>,` |
| `crates/motion-model/src/model.rs:287` | `pub expressions: Vec<crate::PropertyExpression>,` |
| `crates/motion-model/src/render_sampling.rs:66` | `pub motion_blur: Option<Shutter>,` |
| `crates/motion-model/src/vector.rs:274` | `pub dashes: Option<StrokeDashes>,` |
| `crates/motion-model/src/vector.rs:299` | `pub trim: Option<TrimPaths>,` |
| `crates/motion-model/src/vector.rs:324` | `value: groups::ParameterValue, #[serde(default)] animated: Option<bool> },` |
| `crates/motion-model/src/vector.rs:337` | `animated: Option<bool>,` |

## 验收记录

| 编号 | 分类 | 证据与状态 |
| --- | --- | --- |
| PF-01 | 功能契约变更 | 只接受当前版本；移除 Project.migrate 与逐特性版本门槛；不改变版本字段本身。测试覆盖过去每个版本、0、未来版本和 u32::MAX，失败的 load/save 不改原文件。`project-format-model-core-host-final.log` 完整组合通过；覆盖每个历史版本、未来版本、原文件保护与当前格式保存重开。 |
| PF-02 | 功能契约变更 | 必填字段缺失明确反序列化失败；默认构造值也必须写出；可选项缺失仍等价原值。5 项字段契约测试包含本次收紧的 40 个字段、原有必填 version 和可选集合/引用/LUT/命令语义，已纳入最终组合通过。 |
| PF-03 | 已有观察路径 / 已实际验证 | 格式错误继续作为原生 Model/JSON 错误向上传播，不增加重试或吞错；通过真实串行 worker 验证默认 Info 文件、Error stderr、stdout 隔离、请求起始记录、工程状态与原文件保留，以及显式修复后的正常打开。没有新增日志机制。 |
| PF-04 | 正常分支 | None、空集合和命令缺参不是故障；不加逐帧日志，不改变 Present 热路径。 |
| PF-05 | 协议边界 | 快照内 project_format 使用同一常量；效果包、SDK、MCP 与独立协议 version 各自保留，不混用工程版本。八个相关包带 FFmpeg 的 all-targets 检查已通过，宿主 111 项已纳入最终组合通过。 |
| PF-06 | 基线边界 | 未修改渲染实现或基线资产。已检查最终改动边界：渲染测试差异仅含 version fixture 行；不将编译或少量测试代替全部像素验收。 |
| PF-07 | 测试契约更新 | 旧迁移与逐特性门槛断言因用户明确更改格式政策而改为拒绝旧版本；投影、动画、蒙版、历史原子性、采样值及包身份断言保留。首次全量测试发现 5 处旧门槛或缺字段 fixture，已按新契约修正；第二轮仅新增测试漏写既有 InComposition 命令包装的断言失败，修正测试后 5 项字段契约测试通过。保留两轮失败日志，不修改生产命令路由。 |

当前验证日志位于 `artifacts/pr-ci-followups/project-format-*.log`；这些是本机证据，不把 CI 待运行写成通过。

## DESIGN 基线处置

本轮没有改变面板绘制、菜单、尺寸、颜色或命中区域。面板文件的差异仅为测试 fixture 的版本赋值；生产截图 fixture 删除了旧版本覆写，使用 Project::new 的当前版本。本轮按用户明确要求修改工程格式，不将 DESIGN §12 的 UI 重绘清单误当作禁止该任务的要求，也不将本次编译或测试声称为视觉整改完成。

| §0.7 项目 | 状态 / 理由 |
| --- | --- |
| path_controls 六处纵向步进 | retained：无绘制改动，UI 由其他任务后续优化 |
| path_controls 两列尺寸 | retained：同上 |
| path_controls 控件高度 | retained：同上 |
| path_controls 数字字段自绘 | retained：同上 |
| path_controls 布尔按钮 | retained：同上 |
| effect_controls 行高与步进 | retained：同上 |
| effect_controls 布尔文本按钮 | retained：同上 |
| effect_controls 枚举全宽按钮 | retained：同上 |
| effect_controls 悬停 alpha | retained：同上 |
| effect_controls 层级缩进 | retained：同上 |
| timeline 列坐标与标签栏 | retained：不涉及布局 |
| timeline 吸附线颜色 | retained：遵守既有保留要求，没有改令牌或画线 |

### 初期验证失败与修复边界

- `project-format-core-tests.log`：5 个目标失败，分别为 audio_levels、effect_image_inputs、layer_anchor_animation、layer_masks、render_sampling。旧版本特性门槛改为显式旧版本拒绝，JSON 中补齐已有语义的 stage=effects、space=progress；没有修改计算值或像素断言。
- `project-format-core-final.log`：其余模型/编辑器用例通过，新增字段矩阵中的部分命令测试未考虑 parse_commands 的 InComposition 包装，导致 1 项失败。仅修正测试解包及主合成断言；`project-format-schema-final.log` 的 5 项全部通过。
- `project-format-all-targets.log`：截图 fixture 引用了仅 dev-dependency 可见的 motion_model，普通二进制检查失败。删除 fixture 多余的版本覆写，让 Project::new 提供当前版本；未新增依赖。`project-format-all-targets-final.log` 锁定离线、FFmpeg、八个相关包的全部 targets 检查通过。已有面板未使用变量、导入与括号以及 graph fixture 的 dead_code 警告单列为既有工作，不在本轮更改 UI 代码处理。

### 最终后端组合

`cargo test -p motion-model -p motion-core -p motion-host --no-fail-fast --locked --offline -j 1`：**396 项通过、0 失败、1 项既有手工性能测量忽略**（`editor::history_accounting_tests::history_accounting_large_document_measurement`）。包含全部模型、编辑器、宿主测试和宿主 doctest；不将此前单独的 5 项字段矩阵重复累加。

最终 all-targets 检查覆盖 motion-desktop、motion-host、motion-render、motion-core、motion-model、motion-media、motion-graph、motion-desktop-media，启用 motion-desktop/ffmpeg。依赖使用锁定离线模式；来源守卫和面板样式消费守卫通过。桌面真实故障测试与程序构建证据见下段。

### 桌面故障信道与构建闭环

- `project-format-desktop-failure.log` 首次 20 项通过、1 项新增测试失败：临时目录名不符合宿主已有的 project-<数字> 规则，格式加载尚未执行。只修正受控 fixture 为 project-101/102/103，保留生产目录校验规则。
- `project-format-schema-worker-final.log`：新增真实故障用例通过。三种实际磁盘文件（旧版本、缺 version、缺 three_d）经 Engine.open_project 返回原始原因；默认 Info 文件有请求起始与 Error，Error 同时到 stderr，stdout 不含诊断。旧版本原因含实际/期望版本。工程、修订、撤销历史、原文件和失败文件保持原值；显式修复后打开成功。
- `project-format-desktop-failure-final.log`：桌面完整故障组 **21 项通过、0 失败、0 忽略**；包含真实 IO、GPU、模型错误、受控阻塞、panic、超时、清理与恢复。未增加重试、清除错误、替换错误原因或调整已有故障断言。
- `project-format-desktop-build.log`：带 ffmpeg、锁定离线、单任务的 motion-studio Debug 构建通过。既有 UI/测试 dead_code、unused 等警告保留，不声称零警告。
- 本轮合计 **417 个不同测试通过、0 失败、1 项既有手工性能测量忽略**。不重复累加单独重跑的字段矩阵或 schema worker 测试；其他桌面、渲染、媒体和图形测试的 all-targets 编译不计为运行通过。
- 本地可运行目录：`artifacts/project-format-contract/windows-ffmpeg-debug`。使用仓库打包器的 runtime_payload 校验现有 FFmpeg 运行库的架构、每文件 SHA-256、作者、来源、修订与许可证后，连同原许可证、NOTICE 与 ATTRIBUTION 一起复制；没有伪造 Release 构建报告。清除 PATH 内的 FFmpeg 路径及 FFMPEG_* 环境变量后，`--command-list` 和实际 `--video-smoke silent-24fps.mp4` 均以 0 退出且 stdout 为有效 JSON。证据为该目录下 `local-build-validation.json` 和 stdout/stderr 文件。

边界：没有在本机重新运行全部渲染图像基线、全部桌面用例或其他操作系统；远端 CI 须以本次新提交独立验证，不沿用 #388 的绿色结果。上述 UI 基线保持 retained；本次验收不能视为视觉问题已整改。

### 首轮跨平台 CI 的 fixture 补验

首轮 run `39547823282` 的 Linux job `119049897084` 在完整桌面测试中发现 `panels::observation_tests::real_panel_fallbacks_are_visible_bounded_and_preserve_normal_optional_fields` 失败：健康关键帧 fixture 漏写本轮已改为必填的 ease，导致 track 解码返回 None，随后测试 unwrap 失败。这是本轮遗漏的 fixture 更新；此前本机 417 项专项通过不能代替完整桌面运行。

扫描手写关键帧后，为 observation_tests、properties/tree_tests、modified_properties 三处补齐先前默认语义的 `ease: "linear"`。没有恢复生产补值、修改解码返回值、删除故障观察或放宽日志阈值/恢复断言。原始日志保留为 `pr-389-linux-failure.log`。修正后的完整桌面测试和跨平台结果须分别记录，不沿用首轮结果。

相同失败也从 Windows job `119049896872` 和 macOS Intel job `119049897147` 的原始日志确认，均仅此一项失败。修正后的本机完整桌面命令 `cargo test -p motion-desktop --bin motion-studio --features ffmpeg --locked --offline -j1 -- --test-threads=1` 为 **709 项通过、0 失败、1 项既有物理音频设备测试忽略**，463.53 秒；记录为 `project-format-desktop-complete-fixed.log`。此前 396 项模型/编辑器/宿主组合与此次完整桌面互不重复，合计 1105 项通过、2 项既有忽略；不再另加被包含的 21 项故障测试。
