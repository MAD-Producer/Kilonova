# 工作区 GPU 测试选择最终验收

对应 #135。验收基线 `ec10c1d772980b8a5e2fe9520966000c6afc7b79`。六包统一 `gpu-tests` 策略已由 `ed233bc4` 合入；本次新增修复只有 `command_timing_markers` 的一个条件 ignore 属性。该用例由后续计时工作引入，实际需要 GPU，遗漏属性导致显式 CPU 模式在 `Adapter::NotFound` 处失败。去掉新增属性后，正文和断言与基线逐字节相同。

## 验收方式与结果

CPU 命令保留 FFmpeg 与宿主诊断功能，关闭整个工作区默认 feature；完整 metadata 确认 render、graph、host、UI、desktop、desktop-media 六包均未启用 `gpu-tests`。反向显式启用后六包均开启。内部 dependency/dev-dependency 没有重新打开默认 feature。`WGPU_BACKEND=noop` 选择本构建未编译的后端，用于环境感知初始化的不可用适配器对照；开发机显卡没有物理关闭，固定原生表面工厂可能自行选择后端，因此还结合逐项调用链和条件属性审计，不能只凭环境变量证明全部路径没有 GPU。

```sh
cargo test --workspace --no-default-features --features motion-desktop/ffmpeg,motion-host/diagnostics --locked --offline -j1 --no-fail-fast -- --test-threads=1
```

实际 Windows 命令另指定桌面 debug=0/codegen-units=256，并设置现有 FFmpeg、MSVC 与 libclang 开发环境。每个阶段前后核对全部受版本控制文件的 SHA-256；运行期间不修改源码。

| 项目 | 真实结果 |
| --- | --- |
| 完整 CPU 目标并集 | 2081 通过、0 失败、477 明确忽略；263 个目标/文档宿主 |
| 新计时用例，CPU 模式/不可用后端 | 0 通过、0 失败、1 忽略 |
| 同一计时用例，显式 GPU/不可用后端 | 退出 101，0 通过、1 失败、0 忽略，原生 NotFound |
| 同一计时用例，真实 Vulkan | 1 通过、0 失败/忽略；64 像素、16 字节实际时间戳回读，未开启可选 encoder 时间戳特性 |
| UI analytic，显式 GPU/不可用后端 | 退出 101，0 通过、1 失败、0 忽略，原生 NotFound |
| UI analytic，恢复正常后端 | 3 通过、0 失败/忽略，真实 GPU 像素断言 |

`tools/test_ci_workflow.py` 的原有 12 项检查通过。另对内存中的配置分别移除六包必需 GPU feature、关闭 UI 默认 GPU feature、重新打开 desktop→render 内部依赖默认 feature，八种错误均被现有断言拒绝；这是八个错误配置对照，不是八项新测试，不修改生产断言。CI 保留软件 Vulkan、三平台桌面、Windows 输出/显存及 FFmpeg 分区的显式 GPU 必测配置。

## 保留的失败和中断

最初完整 CPU 遍历退出 101：桌面与三个媒体目标没有启动，报告 `STATUS_DLL_NOT_FOUND`；另一个失败是计时用例遗漏 GPU 属性。同一媒体可执行文件的 `--list` 对照中，仅配置 PATH 仍失败，将既有五个 FFmpeg DLL 放到忽略的构建输出旁后成功；所有副本与本机既有开发运行库逐字节一致。该本地运行库修正与测试属性修复分别记录，不新增仓库依赖、产品资产或分发配置，不推断未证明的系统加载器根因。

修正后的第一次整仓运行后来中断，没有最终退出状态。Android/core 的 58 个目标已完整结束，291 通过、1 项既有人工测量忽略；未完成的桌面目标不计为通过。逐项验证这些包的目标集合等于 metadata 中的全部可测试目标，且源码仍匹配冻结摘要；恢复时仅排除这两个完整包，运行其余所有包，不使用用例 skip 过滤。两包未变的空文档测试宿主复用最初完整遍历的结果。最终按 Cargo 目标身份合并，要求目标集合与整仓遍历完全一致、无重叠、无遗漏。单项停点诊断通过，但不另外计入通过数量。

这份记录报告多个真实执行阶段的无重叠并集，不声称有一条从头到尾未中断的整仓命令返回零。原始失败、中断和恢复日志分别保留；子进程输出的嵌套摘要不计为额外用例。负面对照的预期失败也不计为正常通过。

## #135 逐项处置

| 编号 | 处置与证据 | 责任边界 |
| --- | --- | --- |
| T1 完整依赖图 | 既有 `motion-model/tests/dependency_boundaries.rs` 使用完整 metadata，覆盖第三方、UI/desktop 及 media→render 禁令；本轮整仓运行验证 | 既有覆盖，无新架构修复 |
| T2 原生事件宿主 | 既有 `native_main_and_floating_input_remain_isolated_and_consume_each_event_once`，真实隐藏窗口 ID/ActiveEventLoop 到生产事件路由 | Windows 注入事件；不等于物理键鼠、平台 IME 或其他平台端到端 |
| T3 metadata 诊断 | 既有启动、非零退出、JSON 与结构诊断已替代无上下文 unwrap；完整边界用例本轮执行 | 既有覆盖，不新增日志点 |
| T4 模型与 Android | 既有碰撞、候选 oracle、采样及 Rust 像素转换断言；本轮完整两包目标 | Android 只含桌面可执行的纯 Rust 适配，不声称 JNI/NDK/MediaCodec/真机 |
| T5 统一 GPU 开关 | 既有六包策略；本轮补齐一个新计时用例，六包关闭/显式开启图与实际负面对照 | 显式测试选择，不是自动适配器失败回退 |
| T6 像素与拒绝断言 | 既有 `presentation.rs` 实际像素及 `resource_rejections.rs` 独立类型/字段断言 | 像素 GPU 正向证据采用既有专项验收及本 PR GPU CI；CPU 模式明确忽略需要 GPU 的用例 |
| T7 沙箱与隔离 | 既有七项真实表达式回归，禁用全局/动态构造器/延后执行与跨工程缓存隔离，本轮 core 完整目标通过 | 既有覆盖；历史错误实现对照见原专项记录 |
| T8 配置隔离 | 既有 `shell_preferences_ignore_external_configuration`，子进程验证临时目录与外部文件不被读写 | 测试构造隔离，正常产品配置语义保持 |
| T9 路径/Session/回调 | 既有路径、Session、无效 WGSL 回调、健康拒绝与默认输出断言 | 正常 Destroy/显式重建不冒充意外物理掉卡；CPU 模式不运行 GPU 回调目标 |
| T10 架构错误对照 | 既有真实添加 wgpu、一致 metadata/锁文件，边界失败 101、恢复后通过；见 `dependency-boundary-control.md` 与原始回执 | 复用未变源码的历史真实负面对照，不称本轮新增执行 |
| T11 一次手势一次副作用 | 既有 `native_property_gesture_commits_once_across_dpi_and_acknowledged_redraws`，英语/中文 × 三种 DPI，真实 worker/history/撤销重做 | 六种输入组合是一项用例；不夸大为六项新测试 |
| T12 无适配器/CI | 完整 CPU 目标并集、明确 ignored、两项 GPU 不可用失败及真实恢复；12 项工作流检查和八种配置错误对照 | 远端五项必需 CI 的最终状态以 PR 当前提交为准 |
| T13 沙箱断言 | 同 T7，检查行为断言而非仅编译或返回成功 | 不宣称无边界的安全证明或完整覆盖率 |

对应原始验收记录：`dependency-boundary-control.md`、`native-window-input.md`、`native-input-gestures.md`、`render-output-contract-completed.md`、`expression-sandbox-completed.md`、`desktop-config-isolation-completed.md`、`workspace-gpu-test-policy.md` 与 `ui-gpu-test-policy.md`。历史已完成项和本轮新增属性分别标明，记录有处置不代表新增修复。

本次没有新增产品日志点、重试、线程重启、错误结构、布局、样式或依赖。DESIGN.md §0.7 的既有整改项全部保留，原因是本次未修改面板；§12 视觉项待相应界面工作验收，不以测试数量代替。没有新增第三方资产或发现来源规则违规。

## 复核范围

分支创建前检查其他引用、工作树和开放 PR，未发现同一计时属性修复。验收冻结后主线合入 #483；其新增两个真实 Renderer 用例已有统一属性，新增规划目标属于 CPU，Cargo/lock/CI 策略未改。此处本地整仓数字仅对应冻结基线，合并组合由当前 PR 的五项必需 CI 验证；不能把旧基线的本地结果冒充最新主线的完整运行。

原始日志、metadata、文件冻结摘要、本地运行库哈希、恢复回执及执行脚本随 [证据归档](data/workspace-gpu-acceptance-ec10c1d7.zip) 提交，不包含 DLL 或第三方资产。`workspace-gpu-acceptance-results.json` 保存命令、退出状态、摘要、阶段来源、计数规则和归档 SHA-256，可解压后逐项复核；本地原始副本保存在 `.tools/workspace-gpu-acceptance/`。
