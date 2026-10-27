# 渲染队列与输出模板（#240）验收记录

分支 `feat/render-task-queue` 接续字符／段落编辑 #517。开始实现前核对了本地分支和开放 PR，没有发现另一项渲染队列实现；本轮不审阅或改动其他人的工作。此前冻结层和 Shell 的分阶段记录保留在 [render-task-queue-progress.md](render-task-queue-progress.md)，不将当时尚未完成的检查倒记为通过。

## 最终行为与边界

渲染队列面板提供加入、开始、暂停、逐任务取消、明确重试、另存重试和移除。暂停停止调度后续任务，当前任务的取消由独立取消控制完成。每个任务持有合成、修订、范围、素材、PCM/PTS 缓存和确切效果包的不可变副本；修改编辑器工程或模板不会改变已接受的任务。

队列与模板保存在工程的 `.motion-render-queue/queue.json`，冻结输入放在该目录的私有 `snapshots/` 子目录。未使用队列的工程不自动建立目录。只接受当前文档／manifest 版本；工程版本字段及当前版本检查保留，没有旧版迁移。正常重新打开可恢复队列、模板和完成状态；上次中途退出的活动任务变为 Interrupted，需用户明确重试。

目前实际支持 MP4 / H.264 / AAC，使用冻结合成的像素尺寸、帧率及整段／工作范围。模板编辑名称、文件名模式和范围；模式仅接受 `{composition}`、`{id}`、`{revision}`。没有展示尚未实现的格式、码率、分辨率和质量选择。缺少 FFmpeg 或观察模式禁用新增／开始；当前合成的非偶数尺寸只禁用新增，不阻止此前冻结的有效任务开始。面板分别解释这些能力条件。原有单次 PNG、MP4、打包及取消导出入口保留。

队列输出使用原子禁止覆盖发布，既存目标与编码途中创建的目标均保留原字节。取消／失败不发布半成品。移除任务只清理经过归属与路径校验的私有冻结输入，不删除输出。冻结、恢复和编码在后台执行；窗口线程提交有界请求、读取发布状态，切换工程及关闭时等待后台句柄实际结束再回收，不在原生窗口回调中直接 join。

没有新增第三方资产、图标或字体。`sha2 0.10` 复用现有锁定版本，仅增加桌面媒体的直接依赖边；添加前已检查 `PROVENANCE.md`、`ATTRIBUTION.md` 和 `NOTICE`。

## Issue 验收对应

| 条目 | 实现与实际证据 |
|---|---|
| 独立任务、范围与输出设置／模板 | 严格队列文档、捕获时的模板副本、任务修订／尺寸／帧率／范围／尝试次数。真实模板保存、删除、空模板库重新建立及非法文件名拒绝测试通过。 |
| 冻结输入及进度／取消／重试 | 真实 worker 连续导出两个不同冻结源；删除原素材后重开仍可导出。实际活动编码取消、失败后另存重试、进度发布均验证。 |
| 重开状态且不覆盖既有输出 | Store 重新打开、活动状态转 Interrupted、模板恢复；实际已有文件和竞争写入的字节不变。 |
| 后续工程修改不污染任务 | 真实 Engine 校验捕获意图的根目录／文档世代／合成／修订；接受后继续修改工程，输出仍使用捕获的修订。 |
| 取消失败不发布半成品；明确能力禁用 | 实际编码的取消和发布竞争测试；UI 能力门控与正常取消分支。能力不足没有伪成功。 |
| 主窗／浮窗、中英文输入一致 | 三项面板交互回归及 Windows 原生 GPU 截图；相同任务 ID 的动作路由、字段编辑／IME 所有权和失焦、空库新增模板均核验。 |
| 保存重开和撤销 | 真实 journal 重开恢复；原生 Shell 撤销／重做当前工程后，冻结任务与队列文件字节保持不变。队列本身不伪装成工程历史中的创作命令。 |

## 可复现检查与原始记录

本地 Windows 使用现有固定的 FFmpeg 动态运行库，构建目标为 `E:/Dev/aem/target/host-error-contract-verified`。测试日志位于 `E:/Dev/aem/artifacts/pr-ci-followups/`；生成证据包另记录逐文件 SHA-256。下面各过滤运行有重叠，不能把通过数相加当成独立测试总量。

| 检查 | 结果与记录 |
|---|---|
| `cargo test -p motion-desktop-media --features ffmpeg durable_movie -- --test-threads=1` | 4 项通过；实际源删除恢复、目录所有权、载荷／包／manifest 拒绝以及两帧 MP4 逐字节解码像素比较。`render-queue-durable-ownership-final.log`。 |
| `cargo test -p motion-desktop-media --features ffmpeg new_movie_publication -- --test-threads=1` | 1 项实际输出发布检查通过；既存目标、竞争目标、取消与成功输出解码。`render-queue-publication-initial.log`。 |
| `cargo test -p motion-desktop --bin motion-studio --features ffmpeg render_queue -- --test-threads=1` | 17 项通过、1 项原生验收显式 ignored；覆盖模型、Store、worker、Controller 和面板。`render-queue-panel-worker-final.log`。Unix 符号链接专项在 Windows 不计为通过。 |
| `cargo test -p motion-desktop --bin motion-studio --features ffmpeg panels::auxiliary -- --test-threads=1` | 13 项通过、2 项原有 ignored，含既有单次导出范围和字符面板检查；原断言保持。`render-queue-auxiliary-final.log`。 |
| 真实 Engine 捕获身份 | 1 项通过，拒绝四种过期身份；有效捕获后后续编辑不改修订与磁盘输入。`render-queue-capture-identity.log`。 |
| 原生队列实际输出 | 显式运行 `render_queue_native_tasks_templates_and_actual_output -- --ignored --test-threads=1`，1 项通过。`render-queue-native-final.log`。两份 3 帧 MP4 均实际解码，另有真实冲突失败；没有仅以模拟成功返回作为证明。 |
| 带 FFmpeg 编译 | `cargo build -p motion-desktop --bin motion-studio --features ffmpeg` 通过。`render-queue-build-final.log`；既有警告保留，不宣称零警告。 |
| 独立预览运行库 | 使用仅预览目录及 System32 的 PATH，`--command-list` 返回有效 JSON 且 stderr 为空；`--video-smoke` 实际使用 libav 解码 3 帧输出。`render-queue-preview-commands.json`、`render-queue-preview-video.json`。 |
| 完整桌面回归 | 正在运行；完成后记录实际汇总与日志哈希。此阶段不声明通过。 |
| 样式及来源 | `verify_style_consumers()` 和 `tools/validate_provenance.py` 通过。只代表各自守卫范围，不等于整个 DESIGN 合规。 |
| 跨平台 CI | 尚待提交当前队列改动后验证；不复用父 PR 的绿色状态作为本 PR 的证明。 |

原生验收命令通过环境变量 `MOTION_RENDER_QUEUE_NATIVE_ARTIFACTS` 指定保存目录，实际截图在 `render-queue-native-final/`：

- `zh-queue.png`、`en-queue.png`：900×600 的主窗、队列任务与选择状态。
- `zh-template.png`、`en-template.png`：名称、文件名和范围的真实模板编辑。
- `zh-floating-dpi15.png`、`en-floating-dpi15.png`：300×460 逻辑尺寸、150% DPI 的原生浮窗。
- `zh-default.png`：1600×1000 主窗；`zh-main-dpi2.png`：900×600 逻辑尺寸、200% DPI。
- `en-completed.png`、`zh-failed.png`：实际成功和已有目标冲突失败。
- `saved-queue.json`、`saved-queue-final.json`、两份 MP4 和原始 stdout／stderr 一并保留。

截图来自原生窗口和实际 GPU 呈现；输入回放使用生产 `Input`，没有冒称前台 OS 键盘／IME 注入或其他平台手动验收。本轮没有改动右键菜单。长路径／原因在窄面板可能省略显示，完整原因保留在 journal 和日志；尚未增加完整文本 tooltip。

## 错误与观测处置

| ID | 类型 | 验证和边界 |
|---|---|---|
| RQ01 | 新增观察 | 冻结持久化、恢复、队列打开和命令在阻塞 IO 前发布 in-flight 并写入、flush Info start；保留请求、操作、任务、路径和修订。正常完成 Debug，真实失败 Error。 |
| RQ02 | 功能与失败 | 实际非法模板、损坏 journal、缺失／变化的载荷及目标冲突返回原始原因。真实重试是明确的用户动作，不靠自动重试掩盖失败。 |
| RQ03 | 资源与协议边界 | 临时输入归属和清理、取消／发布锁、清理次要失败保留主因；接收端退出后允许丢弃 best-effort 回复。取消、旧意图、空轮询与正常未找到不报故障。 |
| RQ04 | 默认信道实际验证 | 独立默认日志子进程触发非法模板、受控原始 IO 错误与损坏 journal；检查文件 Error、stderr Error、stdout 隔离。原始三路输出另保存在 `render-queue-default-final/`。 |
| RQ05 | 不返回操作的可见性 | 在受控阻塞 closure 返回前检查 `observe` 的真实请求／操作／任务／起始时刻及已 flush 的 Info start，释放后检查清理。该测试是观察装饰器的阻塞验证，不是向生产 Queue 命令注入不返回 handler。实际派发复用此装饰器。 |
| RQ06 | 既有覆盖 | MovieExport、Engine、日志 sink、panic hook 继续复用既有默认信道和阈值覆盖；没有改变健康 Present 记录、60 次阈值或恢复重置语义。 |
| RQ07 | 实际线程专项 | 仅测试构建的命令向真实串行 handler 注入 panic，经过实际线程的 catch 边界，核验原 hook、原始 panic 原因、请求／操作上下文、停止状态和 journal 不变；停止后拒绝继续调度。最终运行结果与原始输出另行记录，不将实现本身视为通过。 |
| RQ08 | UI／功能边界 | UI 显示任务失败及明确取消／重试入口；此为独立功能改动，不将日志规则解释为可以豁免布局、焦点和原有导出语义。 |

计数、错误保留、队列容量与 journal／manifest／文件数量／大小均有界；没有逐帧新增文件 IO 或健康 Present 的 Info start/done。清理路径校验只覆盖此轮私有输入，不能当作外部文件系统一切变化的证明。逐项同步与原子 journal 仅证明普通重开，不宣称断电一致性。

保留首轮 durable 夹具误用固定 6 帧助手、队列状态／move 编译错误和面板 ScrollArea／WorkArea API 编译错误原日志。修正新夹具与真实 API 后重跑；没有修改既有主线断言、运行时预算或期限来制造通过。第一轮原生截图误用浅色菜单行样式，最终改用现有深色 `row`；初轮截图作为过程记录，不作为最终样式展示。

新增 panic 夹具首轮错误地要求 Info start 也出现在 stderr，导致本地无 FFmpeg 检查失败；下一轮又把文件 hook 的格式错误地当成原 stderr hook 的格式。两份失败日志 `render-queue-without-ffmpeg.log`、`render-queue-without-ffmpeg-corrected.log` 和真实三路输出 `render-queue-default-panic-initial/` 保留。最终新夹具按既有契约检查：Info start 只在文件；带请求上下文的 worker Error 同时在文件及 stderr；文件含记录 backtrace 的日志 hook，stderr 含原 hook 的 thread/panicked 行。另断言 stderr 不泄漏 Info，stdout 不泄漏诊断。生产 sink 未改。原 hook 的检查兼容 Rust 在 thread 名之后附带线程 ID，仍要求实际原 hook 的 thread/panicked 行。无 FFmpeg 时新面板夹具明确断言开始被禁用，不把缺少后端当成成功。

## DESIGN §0.7 逐项处置及 §12 范围

已重读规范并核对当前面板。此次仅替换 RenderQueue 的单次导出占位区域，新队列复用 row、Dropdown、TextEditor、ScrollArea 和现有语义 token；没有新增颜色、尺寸 token 或缩小既有命中容差。§0.7 的历史行号已漂移，其他面板的现状不当作本轮修复。

| 历史基线项 | 本轮处置 |
|---|---|
| Path 六处字面步进 | retained with reason：不改 Path 行计划；新队列行高来自 token。 |
| Path 双列分割 | retained with reason：不改 Path 双列；队列标签列复用 label.column。 |
| Path 混合按钮高度 | retained with reason：不改 Path 命中范围；队列使用现有控制高度。 |
| Path 自绘数值字段 | retained with reason：不改 Path 数值；队列元数据只读，模板使用 TextEditor。 |
| Path 全宽布尔按钮 | retained with reason：本轮没有新增布尔控制。 |
| Effect 行高及步进 | retained with reason：不修改效果属性行。 |
| Effect fx／— 布尔 | retained with reason：保留当前效果 Checkbox 实现。 |
| Effect 全宽枚举按钮 | retained with reason：保留当前效果 Dropdown；队列范围同样使用 Dropdown。 |
| Effect hover alpha | retained with reason：不改效果 hover；队列使用 row 的状态解析。 |
| Effect 层级缩进 | retained with reason：不改效果属性树层级。 |
| Timeline 列与 gutter | retained with reason：不修改时间轴列、坐标或命中容差。 |
| Timeline snap guide 颜色 | retained with reason：保留既有吸附视觉基线。 |

§12 的本轮检查覆盖队列的主／浮窗、两种语言、最小／默认窗口、DPI、滚动裁切、选择／禁用／编辑状态、字段输入所有权、工程撤销独立性与实际导出。没有宣称 19 个面板、5 个布局、完整 8×7 状态矩阵、OS Reduced Motion、Path/Mask 整改或全局渲染性能都已在本轮重新验收。实际跨平台 CI 仍需完成。
