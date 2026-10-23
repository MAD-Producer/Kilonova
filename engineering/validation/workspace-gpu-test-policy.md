# 工作区 GPU 测试选择与 CI 必测策略

Refs #135。`gpu-tests` 只控制测试的条件 ignore，不关闭产品绘制，也不移除 GPU 测试正文的编译。

## 使用方式

`motion-render`、`motion-graph`、`motion-host`、`motion-desktop-media`、`motion-desktop` 和 `motion-ui` 默认启用 `gpu-tests`。正常 `cargo test --workspace --locked` 保持 GPU 必测。没有可用 GPU 时，可显式选择 CPU 用例；已配置 FFmpeg 开发运行库的本机验收命令为：

```sh
cargo test --workspace --no-default-features --features motion-desktop/ffmpeg,motion-host/diagnostics --locked -- --test-threads=1
```

恢复全部 GPU 用例，包括关闭默认 feature 的情况下：

```sh
cargo test --workspace --no-default-features --features gpu-tests,motion-desktop/ffmpeg,motion-host/diagnostics --locked -- --test-threads=1
```

单包可以使用 `--features gpu-tests`。CPU 模式报告明确的 ignored，不把忽略计为通过；Rust 测试宿主的 `--ignored` 仍可主动运行这些用例。编译 CPU 模式仍需要各目标原有的原生 SDK 和运行库，这不是移除 wgpu 或 FFmpeg 依赖。

内部依赖与开发依赖显式使用 `default-features = false`，避免工作区选择被依赖默认值重新启用。其他功能 feature 保留。`motion-core`、`motion-media`、`motion-model` 与桌面目标上的 Android 适配器用例没有实际设备初始化，不增加无用的 `gpu-tests` 开关，也不恢复 Android CI。

## 分类与实现边界

| 项目 | 处置 |
| --- | --- |
| W1：真实 GPU 用例 | 为 UI 库之外 159 个 Rust 文件中的 423 项测试增加条件 ignore。逐文件去掉新增属性后，与改动前源码逐字节相同；包括节点子模块、原生表面、捕获和冻结导出中的间接初始化。不是按文件名整组关闭测试。 |
| W2：CPU 用例 | 同文件的规划、解析、整数边界、错误身份、线程与取消测试继续运行；原生窗口事件本身不等于 GPU 初始化。原有手工性能测试的 ignore 不变。 |
| W3：UI 既有策略 | 保留 #440 的 25 项分类。单独修正 `analytic` 中一项找不到适配器却返回成功的用例：现在保留原生适配器错误并失败；使用环境选择后端，支持真实的无适配器负面对照。其余像素正文与断言不变。 |
| W4：CI | Rust/软件 Vulkan 与三平台桌面原分区显式启用各自 GPU feature；Windows 输出、原生显存查询和 FFmpeg 任务同样明确启用。新增工作流检查从 Cargo 清单推导覆盖要求，并阻止内部依赖重新打开默认 feature。没有删作业、缩减平台、添加全局 skip、放宽期限或断言。 |
| W5：设计与范围 | 面板文件只增加测试属性。DESIGN.md §0.7 全部既有视觉整改项保留，本批无呈现改动；§12 的视觉验收不以编译或测试计数代替，也不认领截图验收。无依赖版本、资产、锁文件或渲染语义改动。 |

## 验证记录

初次四个渲染测试目标：CPU 模式 1 通过、7 明确忽略；显式 GPU 模式同八项全部通过。真实 shader 用例的受控 panic 在 CPU 模式被明确忽略，在显式 GPU 模式以 101 失败；按字节恢复后原用例通过。无适配器对照使用未编译的 NOOP 后端，显式 GPU 用例保留原生 `Adapter::NotFound` 并以 101 失败。上述均不是额外的新用例，也不计作正常通过。

工作流 12 项检查通过。仅在内存中移除 core 或 native 的必需 feature，真实工作流断言分别失败；恢复原配置后通过，不修改仓库里的生产断言。

首轮完整工作区使用 `WGPU_BACKEND=noop`，测试前验证依赖图中六包的 GPU feature 均关闭，wgpu 没有编译 NOOP 后端。结果保留在 `workspace-gpu-cpu-validation.log` 和 `-results.json`。该轮发现电影导出和节点子模块遗漏；原生表面工厂还可能使用固定后端，因此不能仅凭环境变量或该轮通过项证明没有 GPU。结合调用链复核补齐 11 项条件属性，保留失败输出，不放宽原断言。原日志含子进程摘要，不能把摘要求和当作不同用例数量。

补齐并衔接 main `6f4dfd2b` 后，节点 GPU 目标明确忽略全部 18 项；完整桌面/宿主测试目标无用例失败。此轮末尾文档测试曾以 E0463 报告依赖失配：两个工作树共用构建缓存发生交错。保留整轮退出 101 的原日志；源码逐项不变，串行重建相同依赖并单独执行同一宿主文档测试后通过，没有重跑已通过的桌面/宿主用例来掩盖失败。

同一 GPU 策略源码的 UI 无适配器对照实际以 101 失败，保留 `NotFound` 原因；恢复正常后端后，同一 analytic 目标的三项真实 GPU 测试全部通过。命令、默认级原始输出、源码摘要和恢复记录见 `workspace-gpu-serial-validation.json`。这些受控失败不算正常通过。该阶段不声称最终合批组合或远端 CI 已通过，组合结果以 PR 当前提交的验证记录为准。

完整渲染包的第一次 CPU 探索也曾遗漏两个正交投影用例；之后已补齐，旧探索结果不充作最终无 GPU 验收。所有本地验证使用开发机真实适配器或明确的后端不可用对照，没有物理关闭显卡，不声称验证了每一种无硬件平台。#135 待完整验收后再关闭。

合批本地 `88a6122d` 的模型、音频编辑/混音、全工作区全目标 FFmpeg/diagnostics 检查、宿主文档、真实电影/PNG 导出及工作流检查均通过，源码逐项保持。验证期间 main 又加入图像解码存储台账；衔接 `cc3d9bdc` 后，只给新增文件中的一项真实 Renderer 用例增加条件属性，保留同文件两个 CPU 用例。不是对该功能的 PR review；新增用例的 CPU 选择和实际 GPU 执行随当前提交单独验证，旧阶段结果不冒充新组合结果。

本批没有新增产品日志点。GPU 测试失败直接保留原始错误；未更改 Present 热路径、错误信道、降级阈值或主次原因协议。对应已有错误信道用例的运行只能算既有覆盖，不算新增可观测性修复。

本地原始证据位于 `artifacts/pr-ci-followups/`：`workspace-gpu-final-source-review.json`、`workspace-gpu-explicit-feature-graph.json`、`workspace-gpu-workflow-controlled-failure.json`、`render-gpu-policy-controlled-failure.json` 与各阶段日志。分支协调检查只读取工作树路径和修改元数据，没有审阅他人的 PR；无活跃分支文件占用冲突。历史暂停工作树的既有改动保持。
