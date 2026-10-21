# 宿主原生错误边界验收

本次推进 #138 的宿主错误类型迁移，并补充 #135 涉及的真实故障回归；两个聚合 issue 均仍有未完成内容，不据此关闭。实现继承 #382 的类型化合成输出边界，不重复归属该 PR 的工作。

## 问题与改动

Session/ops 原先把已知错误立即转换为 String，调用方无法检查原始类型，合成详情还依赖显示文本前缀。本次保留 Model、Render、Media、Effects、JSON、IO、Image、时钟及 GPU poll 原因，按变体生成结构化合成详情。Context、Frozen Session 和主错误容器明确保留 Box<Error>；检查 source 时须按声明的容器下钻。

资源准备回调仍遵守模型事务协议：先恢复未提交的工程，再返回暂存的原生宿主错误；未增加重试。主操作与清理同时失败时分别保留原因。独立导出接口、响应信道、last_error 快照和旧文本协议在明确边界维持现有文本格式。

## 逐项处置

| 项目 | 分类与结果 | 证据与剩余限制 |
|---|---|---|
| HE-01 已知原生错误身份 | 新增类型保留 | failure_boundaries 验证真实 JSON、IO、缺失对象和冻结会话失败；原始 OS 码及 JSON 行列可检查。仍返回 String 的供应方尚未全部迁移。 |
| HE-02 合成错误详情 | 功能边界修正 | 原生 Model、Media、Frozen、Context 包装保留详情；普通文字和 panic 伪造前缀不会获得合成错误身份。显式旧文本适配器继续兼容。 |
| HE-03 资源准备失败 | 功能边界修正与真实故障验证 | 真实 headless renderer 遇到缺失 PNG 返回 RenderError，工程、revision、历史和 renderer 目录未提交；创建文件后注册成功并可撤销。 |
| HE-04 工程清理 | 新增类型保留、复用既有观察 | 同时失败时保留主错误和次级 IO 原因。NotFound 清理仍是正常分支；未增加告警或重试。 |
| HE-05 合成打包诊断 | 复用既有观察、修正原因长度约束 | 锁释放后记录原生错误；默认级别实际故障到达文件与 Error stderr，stdout 隔离，恢复后正常。原因最多 2048 个字符，返回错误保持完整。 |
| HE-06 串行命令与 Present | 既有覆盖、行为保留 | 桌面完整回归包含开始信号、在途状态、默认日志与命令故障；未新增计数器、热路径 IO、Present 日志、超时或重启。没有据此宣称所有未返回操作均已审计。 |
| HE-07 文本与协议边界 | 明确保留 | respond 仍向原信道发送 String，接收方退出的 best-effort 语义不变；ui_paint、部分视频/图像供应方与独立导出接口仍有原有文本边界，待分别迁移。 |
| HE-08 聚合验收 | 待办 | #138 其他层的类型迁移、#135 其余故障注入，以及跨平台 CI 仍分别验收；本地 Windows 通过不代表所有平台已通过。 |
| HE-09 命令与启动错误长度 | 修正既有观察的载荷边界 | 复用有界格式化器，将错误原因限制为 2048 字符并清理控制字符，启动失败记录中的路径独立受限；完整原生原因仍返回原信道。实际 edit 与工程加载失败验证文件、Error stderr、stdout 隔离、原工程字节、回滚及后续成功编辑。锁、初始快照和内存配置的每种启动失败未另行逐一注入，不能据此宣称全部故障已验收。 |

## 实测

环境：Windows，便携 MSVC，官方 wgpu/Naga 30，FFmpeg，debug assertions 开启；独立工作区及专用 target，单测试线程。无新增依赖或素材。以下是发布前本地实现的证据；如后续合入上游有源码变化，须另列集成验证，不能把旧结果算作新提交结果。

- `cargo test -p motion-host --features diagnostics --locked --offline -j2 -- --test-threads=1`：109 项通过，无失败或忽略，含真实 GPU 准备回滚和全部宿主集成测试。
- `cargo test -p motion-desktop --bin motion-studio --features ffmpeg,motion-host/diagnostics --locked --offline -j2 -- --test-threads=1`：676 项通过，0 失败，454.61 秒；1 项既有忽略要求真实原生音频输出设备，未改动该条件。
- motion-desktop-media：29 项单元及 6 项 libav 集成测试通过。
- 上述完整运行之后发现 typed Display 不保证遵守格式精度；新增负面对照以旧写法产生 3856 字符，准确触发 2048 上限断言。改为显式按字符截断后，同一断言通过，完整返回错误不变。
- 最后修正之后，长度用例 1 项及真实默认日志故障/恢复用例 1 项均通过；工作区 all-targets（排除 Android，启用 FFmpeg 和 host diagnostics）锁定离线检查通过，18.09 秒。完整 109/676 项未在这项局部修正后重跑。
- `git diff --check`、面板语义样式扫描通过。旧测试仅适配错误显示比较，预期文字、次数、阈值、回滚及输出条件未放宽。

首轮新增 IO source 断言曾失败：测试把派生的 Box<Error> source 当成裸 Error。检查 thiserror 的真实生成实现后，按声明容器检查内部 Io 并核对原始 OS 码；随后故障边界 10 项通过。不把该失败记录成通过，也未放宽既有断言。

### 主线集成补验

独立实现提交 `fc3e387d` 之后，正常合入 main `ec6763d4`，无冲突。继承主线效果退役的测试资源迁移与图层导航改动；未修改他人分支。对合并后的源码另行验证：全工作区 all-targets 锁定离线检查通过（53.52 秒），宿主效果恢复 7 项、真实故障边界 10 项、图层导航及原生菜单 8 项、退役效果原生故障输出 1 项、合成打包默认日志故障/恢复 1 项，共 27 项通过。没有把早先的完整回归当成此次集成后的完整回归。

对应日志为 `host-error-main-integration-check.log`、`host-error-integration-effects-followup.log`、`host-error-integration-boundaries.log`、`host-error-integration-layer_navigation.log`、`host-error-integration-retired_effect_tests.log` 和 `host-error-integration-logging-composition_bundle_tests.log`。首次效果恢复筛选误用了文件名而非模块名，匹配 0 项，不计入通过数；改用实际模块路径后 7 项全部通过。

跨平台首轮 CI `39532775622` 的 Linux 编译发现 Unix 专有清理测试遗漏一处字符串比较适配。该断言补充显式 `to_string()`，继续检查相同的拒绝原因和外部目录原字节；没有删掉测试或放宽条件。Windows 本地不会编译该 `cfg(unix)` 分支，修复后的 Unix 编译与实际符号链接回归须由新提交的 Linux/macOS CI 验证，不以 Windows 通过代替。

修复提交 `5f8b44f1` 的 run `39533175416` 已实际通过 Linux job `119003938215`，其原始日志明确包含 `substituted_symlink_cannot_redirect_cleanup ... ok`；原始文件为 `pr-388-unix-fixed-linux-job.log`。这是该提交的证据，不替代后续扩展提交的跨平台门禁。

随后并入设备无关音频播放头测试（详见 audio-playhead-clock.md），并同步 main `e76d4a3e` 的预览预算与摄影机实现。唯一冲突位于 Session 的预览目标初始化：保留主线的一像素无效占位及预算策略，同时保留 Error::from 原生错误转换，未恢复旧的 64 像素占位。其他主线改动原样继承。

该组合源码另行通过 all-targets 锁定离线检查（57.12 秒），宿主故障边界 10 项及内存策略 3 项、真实 GPU 预览预算 5 项、完整分辨率冻结 PNG 导出 1 项、音频 19 项，共 38 项通过；音频仍有 1 项既有物理设备忽略。日志为 `host-error-budget-main-check.log`、`host-error-budget-main-host.log`、`host-error-budget-main-render.log`、`host-error-budget-main-export.log`、`host-error-budget-main-audio.log`。完整 109/676 项没有在该组合后重跑，不将其当作此组合的完整结果。

### 父 PR 合入与帧纹理台账

继续同步 main `0c069663` 的帧纹理容量观测。自动合并无冲突；all-targets 锁定离线检查通过（74 秒），合成打包边界 6 项、帧边界 2 项、提交台账单元 7 项、宿主故障/内存策略 14 项、默认打包日志 1 项、公开音频播放头与实际消费 2 项，共 32 项通过，无失败或忽略。原始日志为 `host-error-ledger-main-*.log`；不是完整工作区测试。

必要依赖 #382 的当前提交 `54a3d74` 四项 CI 均成功，实际合并预览仅涉及 8 个后端及验收文件。审查后按依赖顺序合入，合并提交 `3dfa5e1e`，标题及正文手写中文并从 GitHub 读回确认，实际树与预览一致。未处理其他 PR 队列。

上述 32 项运行对应树 `a56b3ca03a6e4d554624712f74d7d9873cf65afd`。将自己的分支改为同步已合入父 PR 的 main `3dfa5e1e` 后，加入本段记录之前的整个树仍精确相同；不为无源码变化重复计数或把文档修改称为新的运行。证明文件为 `host-error-ledger-parent-tree-proof.json`。#388 的最终跨平台门禁仍以之后的当前提交为准。

原始日志保留在开发者工作区 `artifacts/pr-ci-followups/`，不作为仓库内可复现附件：`host-error-host-complete-tests.log`、`host-error-desktop-complete-tests.log`、`host-error-native-tests.log`、`host-error-boundary-followup.log`、`host-error-bounded-cause-negative.log`、`host-error-bounded-cause-positive.log`、`host-error-final-log-channels.log` 和 `host-error-final-workspace-check.log`。上面的命令和已提交测试可供复验。

### 命令与启动错误日志补验

最终边界复核发现 `respond` 与启动失败记录仍直接格式化完整原因。新增真实 Engine 子进程用例，以未知编辑操作和包含未知字段的工程触发原生 JSON 错误；旧实现在命令原因长度断言实际得到 3900，而上限为 2048，因此负面对照失败。子进程在该断言之前已验证完整信道回复、失败前后工程与 revision 不变、下一次编辑成功，以及无效工程文件的原字节保留。

修复只影响既有日志的格式化：在写入过程中限制原因长度，控制字符变为空格；启动外层重复记录也应用限制。返回错误、在途发布、启动返回、best-effort 信道和正常 Present 行为保持原协议；未新增重试、计数器或日志站点。新用例在默认 Info 下分别核对文件及 stderr 的长度，确认尾部测试标记未泄漏，stdout 没有错误日志。单独运行通过后，纳入完整日志模块再次通过，不重复计数。

修复后的专项运行：`logging::` 18 项、`engine::failure::` 19 项、`fallback_observation::` 2 项，共 39 项通过，无失败或忽略；包含真实故障恢复、panic、超时、不重复执行、锁释放及日志通道回归。之后工作区 all-targets 锁定离线检查（排除 Android，FFmpeg 和 host diagnostics）通过，用时 2 分 09 秒。并未在此次局部修改后重跑全部工作区测试，跨平台结果仍须核对当前提交。

日志为 `host-command-error-unbounded-negative.log`、`host-command-error-bounded-positive.log`、`host-command-bounded-logging.log`、`host-command-bounded-engine-failure.log`、`host-command-bounded-fallback_observation.log`、`host-command-bounded-workspace-check.log`。测试入口为 `logging::command_error_tests::long_command_and_startup_errors_are_bounded_without_truncating_replies`；其他错误站点和全部敏感载荷治理不在本项完成范围内。

## DESIGN 基线

只调整 layer_sampling/tests.rs 的错误文字比较，未修改面板生产代码、布局、绘制、令牌、命中范围或右键菜单；没有新增视觉表现，不声称通过新的视觉整改验收。按 §0.7 逐项记为 retained：

| 基线项 | 保留原因 |
|---|---|
| path_controls 六处纵向步进 | 绘制未改，UI 整改按用户要求后续处理 |
| path_controls:893 两列尺寸 | 同上，无新视觉整改证据 |
| path_controls 控件高度 | 同上 |
| path_controls 数字字段自绘 | 同上 |
| path_controls 布尔按钮 | 同上 |
| effect_controls 行高与步进 | 同上 |
| effect_controls 布尔文本按钮 | 同上 |
| effect_controls 枚举全宽按钮 | 同上 |
| effect_controls 悬停 alpha | 同上 |
| effect_controls 层级缩进 | 同上 |
| timeline 列坐标与标签栏 | 无布局改动，维持兼容基线 |
| timeline 吸附线颜色 | 保持 DESIGN 的现有保留要求 |
