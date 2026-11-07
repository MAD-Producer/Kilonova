# 效果附件覆盖与逻辑消费者诊断

关联 #63，基线 `5a025a2d23f2cc35ee4d306550861b086e0b2378`。本记录对应首个诊断提交 `2bc4d324`。**产品默认没有启用 Load/Discard，#63 继续保持打开**。后续测试专用的真实操作替换、旧像素负对照及桌面 GPU 计时见 [附件策略资格验证](attachment-policy-qualification.md)，不能把本阶段诊断报告视为性能收益证明。

开始前核对开放 PR 与所有工作树的分支及改动路径：深度、跟踪、预渲染和根目录 UI 工作均保留；未发现附件覆盖分析的在途实现。复用 `desktop-arm-runner` 工作树，新分支为 `feat/attachment-pass-intents`，不增加工作树副本。

## 实现与边界

| 项目 | 处置及证据 | 仍需验证的范围 |
| --- | --- | --- |
| 63-A1 片元覆盖 | 新增 `CompiledShader.may_discard`，扫描现有已验证 Naga IR 中各函数和入口，递归检查 Block/If/Switch/Loop/Kill。注释和相似标识符不作为 discard；未调用的含 Kill 函数也保守拒绝。缺少证明为 `None`。 | 仅是全覆盖候选的必要条件，不是启用 Load 的充分证明；不改变 WGSL/GLSL 文本或管线身份。 |
| 63-A2 逻辑消费者 | 显式 `PlanBuilder::attachment_intents()` 复用既有 scratch 逻辑图，区分逻辑值、重用的物理 allocation、采样 pass、snapshot/合成消费者和最后执行点；同一 pass 的双端口不重复计数。 | 最后执行点为图顺序，不是 GPU 时间。尚未以此启用 Discard；未来必须继续证明所有外部消费者及物理绑定的一致性。 |
| 63-A3 实际操作 | `encode_pass` 与诊断共用 `color_attachment_operations()`，继续透明 Clear/Store。报告 viewport、实际容量、格式、sprite/混合、IR discard、未知证明和容量留白；终端输出始终识别为消费者。 | 不把 `Load` 当 DontCare，不声称消除既有效果绘制或纹理读写。sprite 的同 pass 混合与跨 pass 保留不是同一件事；当前 planner 无后者。 |
| 63-A4 错误边界 | 缺失程序、缺失 allocation、binding 数量不符、无效 viewport 均返回保留 pass/program/allocation/phase 原因的 `RenderError`；负例检查实际错误内容。 | 这是库 API 的 Result 边界，没有新增 worker、回复协议或日志信道。现有调用方必须继续传播错误；不宣称已验证宿主文件日志/stderr 传输。 |
| 63-A5 观察成本 | 新增显式 `Renderer::effect_attachment_intents()`；示例仅在 `--attachments` 时、全部 CPU 计时结束后生成报告。普通示例不产生该字段。 | Present 不调用分析，不增加每帧文件 IO、日志或消费者报告分配；编译时 IR 分析属于新增成本。CPU 示例不是 native 带宽测量。 |

不增加依赖或第三方资源，不修改 UI、工程格式、packed plan ABI、scratch 放置策略或生产预算。没有修改既有验收断言、期限或忽略规则。

## 本地验证

统一使用 `--locked`、单构建任务、关闭 incremental、同一目标目录。原始命令、环境白名单、耗时、stdout/stderr、2457 个源码文件运行前后 SHA-256、改动源码快照均归档。最终各阶段源码集合和内容完全一致，且与提交前源码一致。

| 阶段 | 结果 |
| --- | --- |
| motion-effects 完整 lib 单元测试 | 37 通过，0 忽略；包含新增 IR 分析 3 项和既有 shader 内存测试 2 项，不重复计数。 |
| attachment_intents 定向测试 | 4 通过，0 忽略；逻辑值重用、双端口、终端输出、padding、discard、未知证明、零实例 sprite、无效描述符与真实 GPU。 |
| scratch_ 规划回归 | 32 通过，0 忽略。 |
| device_scratch_budget / effect_plan_cache / scratch_planning | 3 + 4 + 1 通过，0 忽略；包含不同预算、质量/资源变化、随机 seek、curve LUT 和 GPU 输出回归。 |
| effect_plan_probe --attachments 与无参数对照 | 两次均成功；显式报告包含 17 个 pass，既有完整覆盖候选也有必需 Clear；无参数输出无 attachments 字段。 |

共 **81 项不同测试通过**。实际命令与逐项名称见归档，不将单独重跑的 3+2 项重复计入。

真实 GPU 为 NVIDIA GeForce RTX 4070 Laptop GPU，backend Vulkan。透明背景、半透明 solid、blur/tint、动画位置，依次 capture `(frame,size) = (0,32), (20,16), (7,32), (0,32)`；每步有 8 个附件报告，始终 Clear/Store，容量包含 viewport，保留终端消费者。首尾帧全 RGBA 相等，透明角落 alpha=0，图层内容非空，最终 GPU health check 成功。首尾 RGBA SHA-256 均为 `7b1284d93335df04b3236040c4fd81575e0b5ad185ad309f3016d8a29f637687`。

这只证明当前默认操作在该真实 GPU 场景下保持结果；尺寸变化和随机 seek 不等于已经完成所有实际 padding/sprite/matte 组合，更不是 Clear/Load 性能对照。

首次 `attachment-native-v1` 在测试夹具编译时失败：误用不存在的 `Registry::new`，exit 101；改用实际 API `Registry::default`。保留该次 stderr、源码快照与命令；未通过改变生产行为或既有断言掩盖失败。后续 native-v2 与最终 native-v3 均成功。

## 可核验材料

本地目录 `artifacts/issue-goal-current/attachment-pass-intents/`：

- `final-summary.json`：阶段结果、实际 GPU、4 次 capture 的附件报告及完整 RGBA hash。
- `verify.py`：核对终态、源码冻结、不同测试计数、显式报告/默认对照和保留的首次失败。
- `attachment-pass-intents-evidence.zip`：100 个成员，3,085,879 字节；CRC 与逐成员 SHA-256 已验证，包含原始失败、日志、命令及改动源码快照，不含 exe、DLL 或驱动缓存。
- 归档 SHA-256：`57b97782208c61fed4640096f209c185ccbbcba13178da3091cfd8053805eb18`。

## #63 未完成项

1. 在完整覆盖、边界、物理绑定与全部消费者都证明正确后，提供可对照的 Clear/Load、Store/Discard 策略；当前所有实际操作仍保留原策略，包括逻辑死值。
2. 后续已补充 8 类真实 GPU 场景、32 次跳帧/尺寸组合的候选与错误操作像素对照；仍需在其他精度、格式、设备与完整外部消费者范围内继续验证，不能把有限夹具外推为所有组合成立。
3. 测实际桌面 GPU 与移动 tile GPU 的外部带宽和帧时；当前没有 tile 设备测量证据。不为此恢复 Android CI，不把桌面 Vulkan 结果外推到 tile GPU。没有证明收益就保留当前策略。

没有用户可见样式改动，DESIGN.md 面板整改基线本次不涉及。
