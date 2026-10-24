# 图层剪贴板：核心事务阶段

关联 #204，本文件是阶段记录，不是桌面功能完成声明。分支 `feat/layer-clipboard` 从已合入 #475 的主线 `19d69a1f` 创建；实施前已核对本地分支及开放 PR 的范围，未发现其他图层剪贴板实现。未修改其他工作树，未审查其他人的 PR。

## 已实现的边界

- `LayerClipboard` 冻结当前版本的工程元数据及选中 ID，上限 16 MiB；只导入选中图层及可达嵌套合成的资源闭包。元数据不持有素材文件，也不把完整用户内容写入日志。
- `PasteLayers`、`RemoveLayers` 复用现有命令注册、工程校验、事务回滚和历史记账。连续粘贴产生新的图层/子合成 ID，保留堆叠顺序；返回的新 ID 按原选择顺序排列。粘贴出的新图层解锁，沿用已有复制命令的行为。
- `at_frame: None` 保留原帧位置；给定帧将最早入点的整数覆盖边界移至该帧，整组平移同一整数偏移。不同 FPS 仍保留帧编号，不隐式重采样。小数边界、关键帧的图层局部时间和源媒体偏移保留。
- 内部父级、遮罩和效果图层输入重新映射。外部引用只有在调用方确认同工程、同合成且目标仍存在时才保留；其他情况明确报错，要求一起复制依赖图层，不静默解除关系。
- 字体、文本栅格、效果图像、视频及其可恢复音频按闭包导入；相同已登记资源复用，路径元数据冲突明确失败。宿主仍须负责冻结素材字节并指定自有目标路径，核心不能证明不同工程中相同路径代表相同字节。
- 表达式目标映射到新 ID，源码、种子和配置保留；沿用既有表达式编译/求值，按目标上下文解释名称及索引，不重写任意脚本。表达式失败回滚全部修改。
- 剪切命令只执行删除事务；宿主必须先成功缓存再调用。未选中的子级沿用现有重新绑定保留当前世界变换；不允许破坏未选中图层的遮罩依赖。锁定、丢失对象或其他失败不产生部分删除。

没有新增依赖或第三方素材，没有改变工程版本、效果包格式或渲染语义。保存重开测试复用仓库已有的 PNG 测试夹具，不新引入图像素材。

## 验证与失败记录

`E:/Dev/aem/artifacts/pr-ci-followups/` 保留实际日志：

- `layer-clipboard-core-first.log`：首轮 4 通过、1 失败，保存测试缺少真实目标素材文件。补齐测试目录中的已有/导入图像文件后，`layer-clipboard-core-second.log` 为 5 通过。保存重开现在调用完整 `storage::load`，保留资源校验。
- `layer-clipboard-core-resources.log`：扩展后 8 通过、1 失败。视频夹具带 250 ms 源偏移，却仍覆盖完整四秒，违反既有可恢复区间校验；将该测试夹具的可见范围明确设为 90 帧，未改变产品校验。`layer-clipboard-core-resources-final.log` 为 9 通过。
- `layer-clipboard-core-integration.log`：最终 11 项新测试及 38 项既有集成回归全部通过、无忽略。涵盖命令目录、资源/嵌套合成、父级、遮罩、效果图像输入、小数帧和音频引用；11 项新测试还覆盖静音视频、字体 ID 冲突、连续粘贴隔离、表达式失败、ID 耗尽、失效目标、非活动合成路由及单步撤销。
- `layer-clipboard-core-regression.log`：核心库单元回归 22 通过、0 失败、1 忽略。忽略的专项中断测试不计为通过。这是资源测试扩展之前、核心实现相同的阶段结果。

没有删除或放宽既有断言，没有增加忽略项，也没有修改 CI、等待期限、预算或重试策略。未执行桌面/GPU 组合验收，不以核心通过替代它们。

## 错误与可观测性台账

| ID | 分类 | 处置与证据 |
| --- | --- | --- |
| LC-01 | 功能/协议边界 | 缺失依赖、路径元数据冲突、ID 耗尽、锁定及失效对象返回原有 `Result` 信道；未吞掉错误或改成成功。集成测试检查原始原因、修订号、工程不变及无新增撤销记录。 |
| LC-02 | 已有覆盖 | 核心事务沿用已有 panic 回滚、历史和表达式编译路径；上述核心库回归实际通过。不声称本次新增了核心 panic 观察。 |
| LC-03 | 正常分支 | 相同资源复用、不相关资源排除、空历史和正常取消不应作为故障。前两项已实际测试，宿主取消路径尚待接线。 |
| LC-04 | 新增观察 | 此阶段没有新日志或新的后台任务；核心失败继续向调用方传播。不能据此宣布用户提示或默认日志信道已验收。 |
| LC-05 | 待完成 | 宿主请求 ID、Info 开始/在途状态、文件日志/Error stderr/stdout 隔离、真实 IO 失败/阻塞、取消/过期请求及清理失败主因保留，需要随资源缓存与桌面接线验证。 |

## 尚未完成

桌面层的剪切先缓存后删除编排、应用剪贴板存活期、Ctrl+C/X/V 与原位粘贴、Edit 菜单、文本/关键帧/路径/节点焦点隔离、主窗和浮窗中英文实测、UI 实际截图，以及当前完整组合的 CI 均未完成。没有创建或关闭 #204 的完成 PR。

## 宿主资源与异步提交阶段

新增桌面 `layer_clipboard` 模块，但尚未接入 Shell 或面板。它提供真实后台资源冻结和暂存，以及类型化的引擎读快照/提交命令，不将大剪贴板塞入 256 KiB 的普通 JSON 请求。元数据仍受 16 MiB 上限约束，素材使用既有单文件/包预算及传输缓冲区。

资源缓存由本次操作独占的文件 guard 持有，关闭源工程或删除原素材后仍可粘贴；视频与音频的同一源只冻结一份。重复粘贴逐字节验证已登记目标素材后复用；未登记的同名文件拒绝覆盖。复制中检查取消及源大小/修改时间变化。失败、取消、失效目标和回包接收端退出释放本次自有文件，不递归删除目录，也不删除碰撞文件。正常退出覆盖清理；强制终止进程后的缓存遗留回收未在本阶段实现，不能声称进程崩溃也会执行 Drop。

目标目录按组件解析、检查后才创建下一层，避免先通过目录联接在工程外创建字体子目录。Windows 实际创建 Junction，并用对照 mkdir 证明它确实指向工程外；产品路径拒绝该目的地，外部目录保持为空。测试通过环境变量传递路径，未将路径插入 PowerShell 代码。

Session 增加不进入工程 JSON 的单调文档 generation。重新打开相同路径且修订号重复仍会使旧任务失效；最终提交同时核对 generation、root、合成和修订号。正常取消/被取代返回明确 `clipboard_status=superseded`，不当成故障。工程内外部引用还比较原依赖元数据，避免已变化或被复用的 ID 静默指向另一对象；无关图层编辑不阻止保留未变化的父级。变化的依赖明确要求一起复制，不猜测重绑关系。

暂存文件所有权随提交命令交给串行引擎。编辑拒绝时删除本次暂存文件；若实际事务已推进修订号，即使之后构造回包出错或 panic，仍先保留被工程引用的文件，再传播原故障。受控故障测试执行真实命令后注入错误/panic，并用实际保存重开证明素材没有误删；这项注入是所有权边界验证，不冒充自然发生的回包故障。

### 本阶段验证

日志仍位于 `E:/Dev/aem/artifacts/pr-ci-followups/`：

- `layer-clipboard-host-first-compile.log`、`layer-clipboard-host-first-tests.log`：初始宿主版本 6 项通过。之后继续补充所有权和路径边界，并非完成后无理由重复测试。
- `layer-clipboard-host-owned-compile.log`、`layer-clipboard-host-owned-tests.log`：提交后故障和默认信道增加后 8 项通过。
- `layer-clipboard-host-final-compile.log`、`layer-clipboard-host-final-tests.log`：最终 10 项实际通过、0 失败、0 忽略，30.87 秒；测试程序 SHA-256 `a39a1650e4d32d40d998702c9bbdc92c48e6bf2e9be0eaaa31b084532963ece7`。编译保留 22 项既有测试警告。范围包括源删除后的跨工程粘贴、连续粘贴资源复用、完整撤销/重做、真实保存重开、源缺失、部分暂存失败、非本操作文件碰撞、取消/失效目标、同根重开身份、外部父级变化、真实目录联接和默认错误信道。
- `layer-clipboard-host-regression.log`：既有素材登记 8 项、失败边界 10 项、panic 边界 1 项、会话并发 3 项，全部实际通过，共 22 项，不包含上述 10 项。不以它们宣称完整桌面/GPU 组合通过。
- `layer-clipboard-default-channel-final/`：实际默认级别文件日志、Error stderr 与 stdout 原件。缺失对象、缺失文件、非法目标帧及后台 panic 的原原因进入文件和 stderr，stdout 无这些标记；Info 开始只进入文件。受控阻塞时检查到在途 `io` 状态，并在释放阻塞前实际读取已落盘的 Info 开始。panic 沿用现有全局 hook 记录栈，后台任务另返回带请求 ID 关联的故障，不自动重启。

一次未加载 FFmpeg DLL 环境的裸测试程序发现命令没有产生列表，不作为测试通过证据；补载正常环境后实际列出 56 项已有引擎失败测试，但没有执行，不计入上述通过数。

### 台账更新

| ID | 分类 | 本阶段结果与剩余边界 |
| --- | --- | --- |
| LC-01 | 功能/协议边界 | 继续传播原错误；取消/被取代单独返回显式状态。源缺失、非法帧、失效提交和部分暂存失败均实际测试。Shell 用户提示尚未接线。 |
| LC-02 | 已有覆盖 | 复用串行引擎的 Info 开始、在途状态、错误回包和 panic 信道，真实类型化命令已验证；既有宿主 22 项回归通过。 |
| LC-03 | 正常分支 | 相同资源先比较字节再复用；取消和 superseded 不记录 Error。相关新测试通过。应用输入重复键和焦点还未验证。 |
| LC-04 | 新增观察 | 后台请求提供可查询的请求号、阶段与耗时；每个冷操作开始记录 Info，实际 IO 错误记录 Error，原因日志限制 1024 字符。不存在每帧轮询日志或热路径计数器。完成阶段清理在途状态，取消维持正常结果。 |
| LC-05 | 待完成 | 新后台任务的默认文件日志/Error stderr/stdout 隔离、真实 IO 失败、受控阻塞已证明；Shell 取消/过期输入和 UI 提示、原生多窗验证、完整组合 CI，以及进程强杀后的遗留缓存策略仍未完成。 |

本阶段未修改 UI 样式或交互面板，DESIGN §0.7 的既有基线均保留，理由为未进入任何相关绘制代码；也未以编译或核心测试宣称 DESIGN §12 的视觉验收完成。

## 同步当前主线与工作范围

本线程的 #479 在五项必需 CI 通过后合入为 `1d1f284245fcd50391f672ef79653d09d432e219`，关闭 #205、#241。合入前另将最新主线 `28880872` 与 #479 组合，5 项工作范围测试（包含实际原生主窗/浮窗）和 1 项图像资源集合测试通过；GitHub 合并树与实际测试树 `5af95b154422af821d85f649ae12b086c814d36f` 完全一致。未审查其他人的 PR。

随后将该主线同步到剪贴板分支。`editor.rs` 的命令分派有一处相邻新增冲突，保留 `SetWorkArea`、`PasteLayers` 和 `RemoveLayers` 三个分支。工程格式 25 和必填工作范围由已合入的 #479 提供，本剪贴板功能没有另加迁移或版本分支；嵌套合成复制沿用完整结构克隆，也保留各自的工作范围。

独立的功能修正：批量剪切删除加入既有事务手势保护。未完成的拖动手势中，剪切与粘贴都返回原来的 `Error::Gesture`，不改变工程、修订号或撤销历史；新增真实开始/取消手势测试，避免剪切被并入另一操作的历史。

组合验证：`layer-clipboard-current-main-core.log` 为 12 项剪贴板、3 项命令目录、8 项工作范围测试，合计 23 项实际通过。`layer-clipboard-current-main-compile.log` 重新编译带 FFmpeg 的桌面测试程序，22 项既有测试警告；`layer-clipboard-current-main-host.log` 的 10 项宿主测试全部通过、0 忽略，27.83 秒。该程序 SHA-256 为 `7a1663600f4606cc561ad83b21589324c5fff2ea806acf94137513998a3a6c9d`。默认日志原件保存在 `layer-clipboard-current-main-channel/`，再次实际验证文件日志、Error stderr、stdout 隔离与阻塞前开始信号。

上述为当前组合的 33 项相关验证，不是完整 959 项桌面验收；此前阶段的 22 项宿主回归也不冒充本次新组合重跑。尚未接入 Shell、快捷键和菜单，未创建完成 PR 或关闭 #204。

## Shell 调度、取消与提交确认

本阶段把宿主基础接入实际 `Shell::action` 和应用轮询，尚未提供用户可点击的 Edit 菜单及快捷键。图层剪贴板属于应用而非工程；主窗与浮窗后续共用同一实例。读取、后台 IO、待提交、提交确认分别保留状态，同时最多一个操作；取消不会丢掉尚未返回的任务再启动替代线程，也未添加超时、重启或重试。关闭和切换工程会发送取消，字段析构顺序保证剪贴板先于引擎取消。应用等待策略把剪贴板列为在途工作，避免空闲窗口停止轮询。

复制不产生历史；剪切先完成冻结，再提交原子删除；粘贴只在实际提交成功后更新状态和选择。原位粘贴保留帧位置，播放头粘贴使用读取目标时的帧。多选保留主图层和原有堆叠次序；剪切后立即清理已删除对象的选择。空剪贴板是正常 miss，不暂停播放。缓存失败不替换旧剪贴板，已缓存的素材在源文件删除、切换工程后仍可粘贴并保存重开。

提交必须等待既有编辑、工程切换、窗口附着/调整、素材导入和节点状态回包处理完毕；提交确认期间暂缓普通编辑派发、播放推进与新的节点轮询安装，避免旧确认覆盖较新的工程快照。取消从普通动作队列单独取出，能够到达已经排队的提交；若提交实际已经完成，仍确认真实改动，不能把成功伪装成取消。

### 观察与协议台账

| ID | 分类 | 本阶段实现与证据边界 |
| --- | --- | --- |
| LC-01 | 功能/协议边界 | Shell 展示宿主/后台返回的原始错误；取消与目标失效展示各自的正常状态，不清空错误、吞掉 panic 或改写已提交的结果。 |
| LC-02 | 已有覆盖 | 读取/提交继续使用既有串行引擎的请求号、Info 开始、在途状态、Error 回包；后台 IO 沿用上一阶段的线程信道。没有另建日志系统。 |
| LC-03 | 正常分支 | 空剪贴板、已有待处理请求、取消、superseded 均不视为故障。真实阶段测试覆盖空缓存不暂停播放、重复请求不替换在途槽、取消清理暂存与保留已提交结果。 |
| LC-04 | 新增观察 | 调度器保留 reading / queued / io / ready / committing / cancelling 状态；健康轮询不记录日志、不执行文件 IO。仅“应用缓存意外丢失”的内部协议断裂新增一条 Error；该不变量路径未伪造为可由正常操作触发。 |
| LC-05 | 仍待完成 | Ctrl+C/X/V、Edit 菜单、用户可见取消入口、多窗输入和实际截图、完整组合 CI、强杀进程后的遗留缓存策略尚未完成。当前原生输入回归不能证明新剪贴板 UI 已验收。 |
| LC-06 | 新功能验证 | 使用真实 Shell、Engine、素材文件和保存重开验证复制/剪切/粘贴、跨工程缓存、单步撤销、取消与失效，不以成功 mock 返回值代替命令执行。 |
| LC-07 | 默认信道验证 | 独立子进程在不设置 MOTION_LOG 时运行实际 Shell 缺失对象与缺失文件操作，检查原原因出现在 UI 提示、文件日志、Error stderr，未进入 stdout；取消不产生 Error。 |
| LC-08 | 调度边界 | 新增受控旧状态回包测试，回包来自真实 Engine::State；证明 pending surface acknowledgement 未处理时不提交，提交确认前不启动节点编译。它是快照调度验证，不冒充真实 GPU resize 验收。 |

### #204 完成性检查

| 需求 | 当前权威证据 | 结果 |
| --- | --- | --- |
| 工程内/跨工程依赖、全新 ID、资源/父级/效果/关键帧 | 核心 12 项与宿主 10 项；Shell 实际跨工程保存重开 | 已有已验证实现，完整 UI 入口仍待完成 |
| 剪切缓存成功后删除；重复粘贴无别名；单步撤销 | Shell 中英文命令、取消/失效、资源复用及主选择测试 | 已接入 Shell 并验证命令路径 |
| Ctrl+C/X/V、Edit、原位/播放头粘贴 | 原位/播放头 typed intent 已接入；菜单/键盘未接 | 未完成 |
| 文本、关键帧、路径、节点独立剪贴板 | 节点已有实现；当前 TextEditor 没有通用 C/X/V，关键帧仅有局部复制/缓动缓冲，路径只有部分拖动复制 | 未完成；不能把屏蔽图层快捷键当作这些能力已存在 |
| 主窗/浮窗、焦点与中英文一致 | 已验证中英文 Shell；既有原生输入回归与新 UI 验收分开 | 新剪贴板多窗 UI 未验证 |
| 成功、取消、失效、保存重开与撤销 | 真实 Shell 及默认信道测试；IO 阻塞前状态发布由宿主现有测试覆盖 | 命令路径已有证据，交互路径待完成 |

本阶段不修改面板绘制、token、属性行或菜单布局，没有新增依赖或外部资产。DESIGN §0.7 的十二项分别保留：路径六处行步长、双列划分、按钮高度、数值字段、布尔字段；效果行步长、布尔字段、枚举字段、悬浮色、层级；时间轴列坐标、吸附线。保留理由均为本阶段未触及这些绘制位置；未宣称它们已修复。DESIGN §12 的新剪贴板视觉、多尺寸/DPI、浮窗交互及性能验收仍待执行。尚未创建完成 PR 或关闭 #204。

### 本阶段最终验证

最终代码基于 `4c5c644a`；编译后仅更新本证据文档。验证产物均位于 `E:/Dev/aem/artifacts/pr-ci-followups/`：

| 日志 | 实际结果 |
| --- | --- |
| `layer-clipboard-shell-ordering-compile.log` | 带 FFmpeg 的桌面测试程序编译成功，2 分 07 秒，22 项既有测试警告 |
| `layer-clipboard-shell-ordering-tests.log` | 宿主/后台既有 10 项与新增 Shell 8 项，共 18 项通过、0 失败、0 忽略，47.77 秒 |
| `layer-clipboard-shell-ordering-playback.log` | 4 项播放、曲线和原生属性手势回归通过、0 忽略，35.56 秒 |
| `layer-clipboard-shell-ordering-native.log` | 1 项真实原生主窗/浮窗输入回归通过、0 忽略，17.43 秒；不作为新剪贴板快捷键的完成证据 |
| `layer-clipboard-shell-ordering-project.log` | 16 项工程切换、确认、保存失败、撤销及默认错误信道回归通过、0 忽略，42.11 秒 |
| `layer-clipboard-shell-ordering-node-save.log` | 1 项实际节点源码保存后再替换图的回归通过、0 忽略，13.80 秒 |
| `layer-clipboard-shell-ordering-node-exit.log` | 2 项节点退出保存/取消及过期确认回归通过、0 忽略，15.99 秒 |

以上合计 42 项实际通过，来自同一个测试程序，SHA-256 为 `10196e39efb5208b778cc9b48a86f40455ae9889cee08771708786102649012c`；不是全部 967 项桌面测试，也不与早期同名重跑重复累计。默认信道原件分别位于 `layer-clipboard-shell-ordering-foundation-channel/` 和 `layer-clipboard-shell-ordering-default-channel/`。宿主测试中的 `clipboard-post-commit-panic` 是预期的故障注入，测试确认原 panic 与已提交文件均得到保留，不是被忽略的失败。

早期 `layer-clipboard-controller-compile.log`、`layer-clipboard-shell-compile.log`、`layer-clipboard-shell-first-tests.log`、`layer-clipboard-shell-tests.log`、`layer-clipboard-shell-final-*`、`layer-clipboard-shell-verified-*` 及 `layer-clipboard-shell-*-regression.log` 保留渐进实现的编译/测试原件，不冒充最终组合。之后因主选择清理、空缓存播放行为、空闲轮询与状态回包顺序的实际变化继续验证；未修改既有断言来换取通过。

新模块定向 rustfmt、`git diff --check` 和 `verify_style_consumers()` 通过，未全局重排已有紧凑源文件。当前已发现远端 main 推进到 `7c0cd60b`，该新主线尚未组合；完成 #204 PR 前仍需同步当时最新主线并执行相关验收，不以本阶段测试代替该步骤。

## 图层快捷键、编辑菜单与实际窗口截图

本节更新前面阶段的未接线状态。时间轴片段模式和合成视口现可使用 Ctrl+C / Ctrl+X 复制、剪切图层，Ctrl+V 在读取目标时的播放头粘贴，Ctrl+Shift+V 保留源帧位置。编辑菜单提供相同命令和取消入口，快捷键标签使用真实可定制绑定。剪贴板忙碌、空缓存、锁定图层和不适用的编辑上下文分别控制菜单可用性。文本字段、输入法、搜索、节点、路径编辑、曲线和显式关键帧选择仍优先，不能把这项隔离当作它们自己的剪贴板已经实现。

独立功能修正：点击主窗菜单栏和弹出菜单不再重分配文档焦点，避免打开编辑菜单后丢失时间轴所有权。菜单打开前捕获可用上下文。菜单宽度改读 `dark.json` 的 `menu.popup.min_width`，编辑菜单还按实际字宽及既有 padding 扩展，保留原有行高和点击范围；不新增颜色或另一套菜单控件。没有修改右键菜单。

### 本阶段验证与原始失败

所有下列日志均位于 `E:/Dev/aem/artifacts/pr-ci-followups/`，对应同一测试程序，SHA-256 为 `cb9823c7f974cb690eba8f94f090ee518a40865874ea99ee5e6d203974d12ee2`：

| 日志 | 实际结果 |
| --- | --- |
| `layer-clipboard-ui-menu-compile.log` | 带 FFmpeg 的测试程序编译成功，2 分 24 秒，保留 22 项既有测试警告 |
| `layer-clipboard-ui-native-tests.log` | 1 项实际原生窗口剪贴板测试通过、0 忽略，35.03 秒 |
| `layer-clipboard-ui-menu-tests.log` | 2 项菜单状态及键盘上下文隔离测试通过、0 忽略 |
| `layer-clipboard-ui-shortcut-verified-tests.log` | 9 项绑定、解析与文档一致性测试通过、0 忽略 |
| `layer-clipboard-ui-host-tests.log` | 18 项宿主/后台/Shell 测试通过、0 忽略，53.76 秒 |
| `layer-clipboard-ui-input-tests.log` | 1 项已有真实主窗/浮窗输入隔离回归通过、0 忽略，15.87 秒 |
| `layer-clipboard-ui-playback-tests.log` | 4 项播放、曲线及原生属性手势回归通过、0 忽略，27.16 秒 |
| `layer-clipboard-ui-keyboard-tests.log` | 24 项键盘回归通过、0 忽略 |
| `layer-clipboard-ui-timeline-keyboard-tests.log` | 48 项时间轴快捷键/关键帧回归通过、0 忽略 |

合计 107 项实际通过，不与初次运行或文档生成重复累计，也不是全部 970 项桌面测试。首轮 `layer-clipboard-ui-shortcut-tests.log` 保留 8 通过、1 失败：新增四项绑定后，生成的 `DEFAULT-KEY-BINDINGS.md` 未更新。使用既有 `MOTION_UPDATE_SHORTCUT_DOCS=1` 生成入口更新文档，`layer-clipboard-ui-doc-generation.log` 保留生成成功记录，再运行原有全部快捷键断言通过；没有修改断言或增加忽略项。最终编译后仅改文档和生成截图裁剪，没有再修改 Rust 源码。

### 实际画面与交互证据

`layer-clipboard-ui-native/` 保留八张原始 GPU 截图：`main-en-minimum-edit-menu.png`、`main-en-minimum-pasted.png`、`main-en-minimum-cancel-menu.png`、`main-zh-default-edit-menu.png`、`main-zh-default-pasted.png`、`main-zh-default-cancel-menu.png`、`float-en-minimum-pasted.png`、`float-zh-default-pasted.png`。另外两张 `main-zh-default-*-menu-crop.png` 是原截图的 540×540 左上角裁剪，未重画 UI。

真实隐藏 HWND 和 wgpu surface 分别验证主窗英语 900×600/100%、主窗中文 1600×1000/150%、浮窗英语 900×600/150%、浮窗中文 1600×1000/100%。输入通过生产 `Input` 送入实际 chrome 和快捷键路由，再执行真实 Shell/Engine 命令，不是 OS 键盘注入或完整 `window_event` 分派测试。四种情况实际复制；主窗实际打开编辑菜单并点击播放头粘贴，验证结果在 50 帧且打开/点击后仍聚焦时间轴；浮窗实际 Ctrl+Shift+V，验证结果保留 10 帧。主窗另以受控排队保持 ready 阶段，实际点击取消入口，等待确认后不再 busy。最后实际保存并用完整 storage::load 重开，确认四次成功粘贴后为五图层。

已查看中英文主窗菜单、忙碌取消菜单和中文浮窗原始画面，新增标签/提示可读。900×600 主窗既有右侧属性布局仍有裁切，本次没有修改该布局，不能宣称整个最小窗口符合 DESIGN §12。截图没有伪造合成画面；UI 捕获使用空 composition。

原生截图子进程 stderr 保留本机 Vulkan loader 找不到 Epic EOSOverlay Win32/Win64 JSON 的既有驱动环境 Error；不宣称整个 stderr 为空，也未修改注册表或吞掉这些记录。剪贴板默认信道原件位于 `layer-clipboard-ui-foundation-channel/` 与 `layer-clipboard-ui-shell-channel/`，由上述 18 项真实故障/阻塞/取消测试重新生成。

### 台账与剩余范围

| ID | 分类 | 本阶段处置与边界 |
| --- | --- | --- |
| LC-01 / LC-02 / LC-07 | 已有覆盖 | UI 入口复用原有类型化请求、后台 IO 和 Shell 错误信道；上述 18 项重新验证原始故障、默认文件日志/Error stderr/stdout 隔离。不新增每帧日志。 |
| LC-03 | 正常分支 | 禁用、空缓存、重复键、失焦、输入法和其他编辑器所有权由菜单/路由测试验证；实际取消入口返回正常取消，不作为 Error。 |
| LC-04 | 新增观察 | 面板只镜像应用剪贴板 available/busy 状态；无文件 IO、额外后台任务、计数器或热路径开始/完成日志。 |
| LC-06 / LC-08 | 功能/协议边界 | 实际窗口完成菜单点击、主窗/浮窗粘贴、取消与保存重开。输入是显式组件输入，未冒充 OS 输入服务或 GPU resize 调度的完整验收。 |
| LC-05 | 待完成 | 文本、关键帧、路径独立剪贴板及完整上下文路由；完整 OS 多窗交互、打包程序、最新主线组合与 CI；强杀后的缓存遗留回收。节点独立剪贴板沿用既有实现。本阶段不关闭 #204。 |

DESIGN §0.7 十二项仍分别保留，修改前后均未触及所列位置：path_controls 的六处步长、双列、按钮高度、数值字段、布尔字段；effect_controls 的步长、布尔、枚举、悬浮色、层级；timeline 的列坐标、吸附线。证据为本阶段 diff 仅涉及菜单、键盘及状态接线，截图不作为这些旧问题的修复证据。理由是它们属于独立的属性树/时间轴视觉整改。菜单现有其他行高、边框等硬编码也保留，未宣称全部 token 化。

新模块定向 rustfmt、`git diff --check` 和 `verify_style_consumers()` 通过。没有新增依赖、第三方资产或渲染语义变更，没有审查其他人的 PR。此为已验证的图层 UI 阶段，尚未创建完成 PR，仍需按 #204 全范围继续实施。

## 关键帧剪贴板：核心事务与错误信道

此阶段继续 #204 的独立关键帧剪贴板要求，没有缩成仅屏蔽图层快捷键。实施前再次核对本地近期分支及 GitHub 开放 PR；当时开放列表为空，没有发现重叠实现。沿用 `feat/layer-clipboard`，未修改其他工作树。

### 实现范围

新增 `KeyframeClipboard` 冻结选中通道的真实关键帧数据，不保存对原关键帧的引用。保留数值、时间缓动、曲线和空间切线，也支持非 Copy 的颜色传递曲线对象。显式目标地址包括变换/摄影机及独立轴、效果参数、蒙版参数与节点、普通矢量参数/节点、嵌套形状内容与音频左右声道。只捕获真正存在的关键帧，不把采样结果冒充作者关键帧。

`PasteKeyframes` 和 `RemoveKeyframes` 加入同一 typed 命令、目录、合成路由和历史事务。粘贴目标按通道明确指定，不自动复用另一个工程中的对象 ID。属性签名和数据类型必须匹配；效果还要求插件、效果、版本、hash、参数和 kind 相同，目标参数必须已实现且可动画。不跨锚点/位置等属性转写，不自动创建效果或把不匹配的值截断为另一种类型。左右音频通道和同一属性的独立轴可显式互拷，原值范围仍由原有工程校验约束。

缓存时间统一为合成帧，粘贴时再转换到目标图层的有符号局部帧。原位保留原合成帧；给定播放头时以全部轨道最早选中帧对齐，保留相对间隔，不按 FPS 自动重采样。目标帧占用、范围越界、锁定、类型不符或后续通道失败，都由普通核心事务回滚整组改动、修订号和历史。未完成手势中拒绝这两个批量命令，避免混入别的拖动历史。

剪切先比较缓存与当前各源关键帧的完整内容，过期数据不能删除已被修改的关键帧。完整删除一条轨道后保留最后删除关键帧的值，沿用既有 Track 行为；颜色传递曲线也遵守该规则。节点和 dash 地址先规范化数字部分，避免 `node:010:011` 与 `node:10:11` 指向同一通道却绕过重复检查。捕获用二分查找，删除用一次 retain 与二分成员检查，避免对密集关键帧反复线性查找、逐次移动整个 Vec。

剪贴板元数据受 16 MiB 上限约束；捕获逐轨累计尺寸并在追加前拒绝越界，最终再校验完整序列化尺寸。每条轨道沿用已有键数/数值/曲线校验，通道数量不错误地复用 128 个图层的上限。新错误类型保留原 `Error::source()`，加入 phase、object 和有界通道描述；描述中的参数名最多 128 字符，不记录关键帧数据或完整用户内容。

### 观察台账与剩余边界

| ID | 分类 | 实现与证据边界 |
| --- | --- | --- |
| KC-01 | 新功能/协议边界 | 冻结多轨数据、严格目标匹配、原位/播放头时间转换、原子剪切/粘贴和手势隔离。不是只用 CopyKey 再读原轨道。 |
| KC-02 | 新增错误上下文 | capture / validate / paste / cut 失败保留 typed 原因链，并附对象和通道；没有清空错误或把失败改为成功。 |
| KC-03 | 已有覆盖 | 核心事务、回滚、保存和 panic 边界继续复用既有实现；桌面调用使用原来的 Edit 信道，其 Info 开始、在途状态、完成计时及 Error file/stderr 行为不变。没有另建日志器或后台线程。 |
| KC-04 | 正常分支 | 只读复制不产生历史；属性不兼容和目标占用为明确拒绝。这个核心模块没有轮询、热路径日志、重试或新的退化计数器。 |
| KC-05 | 待完成 | 应用私有关键帧缓存、typed 大载荷读取/提交、文档 generation 与取消、主窗/浮窗焦点路由及菜单/快捷键接线、实际 UI 截图仍待实现。普通 JSON Edit 传输的既有大小限制不因模型的 16 MiB 上限而消失。不能据此宣称关键帧剪贴板已经可在 UI 使用。 |
| KC-06 | 待完成 | 文本和路径独立剪贴板、完整 OS 输入/打包验收、最新主线组合及 CI；#204 保持开放。 |

本阶段未修改面板、菜单或样式，没有新增依赖、资产、工程格式或渲染语义。DESIGN §0.7 的十二项继续按上一阶段逐项保留，理由为此处只新增模型、核心命令与测试；不宣称修改了这些属性行的外观，也不需要为没有变化的 UI 伪造新截图。最终验证记录在下节补齐，早期结果不代替最终树验证。

### 关键帧阶段的验证记录

`E:/Dev/aem/artifacts/pr-ci-followups/` 保留全过程：首个 `keyframe-clipboard-core-first-compile.log` 错用了尚未创建的集成测试目标；随后 check 暴露 MaskProperty 缺少相等比较及 serialized_bytes 返回值类型使用错误，已修正。`keyframe-clipboard-core-first-tests.log` 保留测试夹具 API 拼写/保存参数次序错误；`keyframe-clipboard-core-second-tests.log` 实际执行 12 项，10 通过、2 失败，原因分别为音频夹具仍有空间变换、摄影机 FOV 夹具错误使用弧度。只修正新夹具，保留原有音频/FOV 校验和断言。第三次 12 项全部通过。

之后增加地址别名、密集剪切、129 通道、元数据越界和原原因链测试，并加强失败后完整历史诊断不变断言。最终 `keyframe-clipboard-core-verified.log` 在当前生产代码上实际通过 67 项、0 失败、0 忽略：新增关键帧 17 项、音频电平 5 项、命令目录 3 项、效果 9 项、既有占帧保存回滚 1 项、图层剪贴板 12 项、蒙版 3 项、摄影机 9 项、空间路径 5 项和形状内容关键帧 3 项。新增 17 项用时 2.43 秒；其他阶段同名测试不重复累计。新的保存重开调用完整 storage::load，未用 JSON 反序列化替代资源/工程检查。

中间的 `keyframe-clipboard-core-regression.log`、`keyframe-clipboard-core-final.log` 为扩展与诊断补充前的阶段结果，不作为最终实现的额外通过数。一次桌面构建误把 bin 名 motion-studio 当成 package 名，原失败保存在 `keyframe-clipboard-desktop-compile.log`；使用实际 package motion-desktop 修正命令，没有调整产品或 CI。此前桌面编译原件 `keyframe-clipboard-desktop-final-compile.log` 成功，保留 22 项既有测试警告，但不是诊断补充之后的最终验收结果。

最终 `keyframe-clipboard-desktop-verified-tests.log` 使用 `motion-desktop --features ffmpeg --bin motion-studio`，定向执行 `layer_clipboard::`（排除 native），实际 19 项全部通过、0 失败、0 忽略，955 项未执行。编译用时 2 分 59 秒，测试用时 32.40 秒，保留 22 项既有测试警告；不宣称全桌面套件通过。本阶段未重复执行上一阶段的 native 窗口截图测试。

其中新增真实引擎子进程用默认日志级别执行三条失败路径：目标已占用、属性不匹配、剪切源已改变；前两条置于 Rename 后方验证整批回滚。`keyframe-clipboard-default-channel/logs/motion-studio.log` 与 `stderr.log` 均保留原始原因及 phase/object/channel，`stdout.log` 不含这些错误。Info 开始信号仅在文件日志中，Error 同时输出 stderr；该子进程没有设置 MOTION_LOG。既有图层/控制器的失败、取消和阻塞信道测试也在此 19 项中重新通过，证据目录为 `keyframe-clipboard-layer-channel`、`keyframe-clipboard-shell-channel`；未把既有受控阻塞测试冒称新关键帧专用阻塞测试。

最终可执行测试文件位于 `E:/Dev/aem/target/host-error-contract-verified/debug/deps/`。`keyframe_clipboard-46978e2a5fc6c72f.exe` 的 SHA-256 为 `8cb4f6668a804e862f63e1d6acd953eb08e1670affacde245a7758a8b3d57267`，`motion_studio-083034f6d6ffe3a9.exe` 为 `1e6c5db1b40d4402bcd98e1558355470e3d97ae332e3336fb9a4d1201051c282`。验证后未修改生产代码。新增模块定向 rustfmt、`git diff --check` 和 `verify_style_consumers()` 通过；尚未据此创建完成 PR 或关闭 #204。

## 关键帧剪贴板：类型化桌面桥接与独立缓存

继续在同一分支实施，并再次只读核对近期本地分支和开放 PR。此时仅 #483（效果暂存纹理复用）开放，与本阶段无重叠；未审查或修改该 PR。

新增 `KeyframeClipboardRead` 从串行引擎工作线程捕获关键帧和精确的文档 Stamp；粘贴读取只返回目标代次/合成/修订号及播放头，不复制完整工程。关键帧载荷通过 typed 通道和 Arc 传递，不经过既有 256 KiB JSON Edit 入口，也没有放宽该入口上限。提交复用普通核心批量事务；复制缓存不产生历史，剪切/粘贴一次撤销。

图层和关键帧使用两个独立缓存，仍共享一个 Pending、取消标志、Shell 动作屏障和提交确认状态。关键帧复制/剪切在读取成功后先缓存，再允许提交删除；失败复制不破坏前一缓存，取消读取在收到回复前保留待处理槽。提交前检查完整 Stamp，旧文档代次即使复用了目录、对象和修订号也不能被删除或写入。提交完成后的取消保留真实 Applied 回复，不把已发生的事务伪装为取消。

Shell 增加类型化关键帧动作和独立状态消息，共用已有播放停止、工程切换、关闭与异步动作调度。按 Domain 区分完成结果，只有图层粘贴读取 edit_result.objects 更新图层选区；关键帧粘贴保留所选图层。本阶段尚未由键盘或菜单发出该动作，不据此宣称编辑器选区或最终 UI 已经接通。

| ID | 分类 | 本阶段处置与边界 |
| --- | --- | --- |
| KB-01 | 新功能/协议边界 | typed 读取/提交、大载荷、独立缓存和原子事务；替代 KC-05 中对应的待完成项。 |
| KB-02 | 新增信道覆盖 | 新回复类型加入当前命令的 Engine panic 回复匹配；排队请求在工作线程退出后仍沿用断链协议，由现有 poll_reply 附带原始故障，未改成新协议。模型产生的对象/通道/phase 错误仍经既有 Error 文件及 stderr 输出。实际故障注入在 TestPanic，未冒称直接向关键帧 capture 内部注入 panic。 |
| KB-03 | 已有观察覆盖 | 新命令复用非 Present 的 Info 开始、在途发布、完成计时与终止边界；不新增线程、重试、计数器、每帧 IO 或 Present 日志。受控阻塞使用已有 TestWait 验证公共调度器，不能冒称关键帧计算本身的可注入阻塞。 |
| KB-04 | 正常分支 | 空缓存、空选区、已有待处理操作、取消和 Stamp 过期为正常不提交；失败复制保留旧缓存。取消之后已提交的事务仍报告成功。 |
| KB-05 | 待完成 | 各属性编辑器的真实关键帧选区、快捷键和菜单路由、主窗/浮窗及 UI 截图；图层 UI 仍保持原入口，不让未接通的属性动作回退为图层操作。 |
| KB-06 | 待完成 | 文本和路径独立剪贴板、完整 OS 输入、当前主线组合、可运行打包程序和 CI；#204 仍开放。 |

没有新增依赖、资产或工程格式/渲染语义；没有修改控件绘制、布局或样式 token。DESIGN §0.7 的十二项沿用前述逐项保留处置，理由是此阶段只涉及桥接、内部动作和回归测试。状态消息只经内部动作验证，未新增实际 UI 截图，也不冒称完成界面验收。

### 桌面桥接阶段的验证记录

首轮 `keyframe-clipboard-bridge-first-tests.log` 保留新夹具把帧写为 i64、而 Track 实际要求 i32 的编译错误；改正夹具的显式受检转换，没有改生产类型、断言或增加忽略。后续实际结果补记于此。

第二轮 `keyframe-clipboard-bridge-second-tests.log` 实际执行 28 项，26 通过、2 失败。新测试中的 4,000 个简化关键帧未达到 JSON 请求上限，扩为 6,000 个并继续严格断言实际字节数大于 REQUEST_LIMIT，未调小产品上限。另一个新测试错误地直接等待终止后排队请求的原始 Receiver；生产协议本来通过 poll_reply 处理断链。改为调用真实 poll_reply，并精确断言返回值等于带操作上下文的完整主错误，没有修改 Engine 退出协议、清空原始原因或把错误改成成功。该失败的子进程原始 file/stderr/stdout 保留于 `keyframe-clipboard-bridge-default-channel/`。

最终 `keyframe-clipboard-bridge-third-tests.log` 使用 `cargo test -p motion-desktop --features ffmpeg --bin motion-studio layer_clipboard:: -- --test-threads=1 --skip native`，实际 28 项全部通过、0 失败、0 忽略，955 项未执行。包含既有 19 项、新增桥接 7 项和 Shell 2 项；不重复累计既有项，不宣称全桌面套件通过。编译用时 1 分 11 秒，测试用时 58.08 秒，保留 22 项既有测试警告。核心代码自前阶段 67 项通过后未修改。

新增桥接验证覆盖：6,000 键载荷实际超过普通 JSON 限制且 typed 粘贴/一次撤销/重做/完整 storage 保存重开成功；两种缓存互不覆盖并共享忙碌屏障；源改变后的旧剪切不删除但可继续粘贴缓存；读取取消等待回复、队列提交取消不修改、迟到取消保留 Applied；源 Engine 退出后仍可跨工程粘贴；同目录同修订号的新文档代次拒绝旧 cut/paste。Shell 在中英文均保留目标图层选区，验证局部帧转换、剪切一次撤销和现有动作屏障的取消/过期行为。

新增默认级别子进程原件为 `keyframe-clipboard-bridge-default-channel-verified/`，保存文件日志、stderr 与 stdout。真实 typed 读取不存在对象及 typed 提交目标占用均记录 request/operation、phase/object/channel 和原始原因。TestWait 阻塞时可读取在途状态和已落盘的 Info 开始信号；引擎注入终止后，排队读取经真实 poll_reply 返回带操作上下文的完整原始 panic/恢复信息。三种错误均进入文件与 stderr，未泄漏到 stdout；Info start 仅入文件。没有给关键帧 capture 加等待钩子或重启恢复策略。

最终桌面测试文件 `E:/Dev/aem/target/host-error-contract-verified/debug/deps/motion_studio-083034f6d6ffe3a9.exe` 的本阶段 SHA-256 为 `e2410353a32916c79b3fa1be7a50ef7b1388282854a739fbf4076d4e0fee90a5`；同一路径之前的阶段 hash 属于之前构建，不作为当前文件的身份。此后未修改生产代码。新模块与本阶段整理模块定向 rustfmt 检查、`git diff --check`、`verify_style_consumers()` 通过。无新 UI 入口、截图或 CI 完成声明，未关闭 #204。

## 关键帧剪贴板：属性选区、快捷键和菜单

继续 #204，实施前只读检查近期分支及开放 PR，当时只有渲染暂存纹理 #483，与此处不重叠，未审查该 PR。新增选区投影读取既有控制器，不另建关键帧选区：变换、效果、蒙版、嵌套形状及音频电平可复制当前通道全部作者关键帧，显式选择单帧时只复制该帧。效果检查器复制属性，不受时间轴此前留下的单帧选择限制。蒙版路径聚合选择要求各节点存在相应作者关键帧；部分姿态由核心明确拒绝，不用采样值补造关键帧。

快捷键按当前编辑器发出 typed 关键帧动作；失效的显式属性阻止回退为图层操作。Edit 菜单在标题栏改变焦点前保存所有者、目标和修订号，显示对应中英文关键帧标签，并检查忙碌、锁定、缓存和作用域。主窗和浮窗镜像同一应用缓存可用状态；文本、IME、节点、菜单和手势仍优先。快捷键说明同步更新，默认键位目录和偏好设置通用命令名无需变动。没有改右键菜单、行结构、绘制颜色、尺寸或样式 token。

| ID | 分类 | 处置与证据边界 |
| --- | --- | --- |
| KU-01 | 新功能 | 属性/单帧选择、编辑菜单和键盘接入独立关键帧缓存；不跨属性自动转写。替代 KB-05 中对应待完成项。 |
| KU-02 | 新观察 | 非可选轨道缺失 keys 或帧不是整数时，复用现有面板 `observe!` 固定站点 `clipboard.key_frames`，记录对象、阶段及禁用分支，不记录键值。该既有观察器按进程寿命累计，首次 Warn、第 60 次 Error，此后抑制；邻近正常行不重置，不能理解为连续一秒。 |
| KU-03 | 正常分支 | 空轨道、尚未创建的可选音频包络、失效选区、无缓存、只读、重复键、IME 和失焦不作为错误。默认信道测试比较正常分支前后日志，确认没有新增记录。 |
| KU-04 | 已有覆盖 | typed worker 失败、panic、阻塞前发布状态、取消、旧 Stamp 和大载荷沿用桥接阶段信道，本轮 clipboard 回归重新执行。未增加线程、重试、Present 日志或每帧文件 IO。 |
| KU-05 | 待完成 | 选区作用域当前在 UI 路由/菜单激活时检查，尚需把来源绑定贯穿到异步读取，验证组合切换前已排队的旧动作；后读取 Stamp 验证不能代替这一项。文本、路径独立缓存、完整系统事件、当前主线组合、打包及 CI 仍待完成，#204 保持开放。 |

### 实际验证与截图

`E:/Dev/aem/artifacts/pr-ci-followups/` 保留最初编译失败 `keyframe-clipboard-routing-first.log`：新增字段的私有可见性妨碍既有外部 Panels 构造，已改正。第二轮实际 50 项中 48 通过、2 失败：新音频夹具没有设置焦点；新日志夹具错误断言之前已写日志的文件为空。分别补上焦点并改为严格比较正常分支前后内容；没有修改既有产品断言、阈值或忽略测试。第三轮和 verified 轮各 50 项通过，是中间结果，不重复累计。

最终 `keyframe-clipboard-routing-final.log` 使用 `cargo test -p motion-desktop --features ffmpeg --bin motion-studio clipboard -- --test-threads=1 --skip native`，实际 50 项通过、0 失败、0 忽略，944 项未执行，用时 57.38 秒，保留 22 项既有编译警告。覆盖既有桥接/Shell/节点/menu 及新增十项属性路由回归。核心代码自之前 67 项验收后未变。`keyframe-clipboard-selection-default-channel-final/` 保留 malformed 真实退化子进程的文件、stderr、stdout：120 次异常只产生首次 Warn 和第 60 次 Error，Error 同时进入文件及 stderr，stdout 不含错误；默认 MOTION_LOG 未设置。合法空轨道和可选包络另有正常分支断言。

`keyframe-clipboard-native-first.log` 的真实 HWND/wgpu 测试通过，用时 15.78 秒。四组为主窗英语 900×600/100%、主窗中文 1600×1000/150%、浮窗英语 900×600/150%、浮窗中文 1600×1000/100%，分别覆盖普通单帧与曲线整通道。生产组件 Ctrl+C 实际捕获；主窗打开菜单并点击播放头粘贴，浮窗 Ctrl+Shift+V 原位粘贴。验证真实局部帧、目标图层选择保留、完整 storage::load 保存重开和一次撤销。选区通过既有控制器设置，输入经生产 `Input`/组件路由，未覆盖 OS 键鼠注入及完整 `window_event`。

原始六图位于 `keyframe-clipboard-ui-native/`：`main-en-single-edit-menu.png`、`main-en-single-pasted.png`、`main-zh-curve-edit-menu.png`、`main-zh-curve-pasted.png`、`float-en-single-pasted.png`、`float-zh-curve-pasted.png`。已查看中文主窗菜单、英语普通时间轴和中文浮窗曲线；菜单可读且实际目标关键帧存在。空 composition 只绘制 UI，不伪造合成输出。最小窗口右侧属性裁切沿用既有布局，本轮未修复，不能宣称全界面验收通过。原生子进程保留本机 Epic EOSOverlay JSON 缺失的 Vulkan loader 错误，未吞掉或改注册表。

既有图层 native 回归先因未加载 FFmpeg DLL 环境而退出 `-1073741515`，没有进入测试；加载仓库既有 env.ps1 后，`keyframe-clipboard-layer-native-regression-with-env.log` 实际 1 项通过，用时 21.93 秒。没有把失败启动当作产品测试通过。最终测试文件 SHA-256 为 `8eb30659e8860811c19e07eca46cc17d7f6b22d14ef9a3af28e4a30c6f040da7`，对应上述 UI 阶段构建。

DESIGN §0.7 的十二项处置如下。基线行号/旧表达式不能代替当前代码判断；本阶段不宣称这些旧问题已被修好或仍以原表达式存在。截图仅验收新增菜单行为。

| 基线项 | 处置与理由 |
| --- | --- |
| path_controls 六处行步长 | 保留绘制实现；本轮只增加选区读取。 |
| path_controls 双列 | 保留绘制实现；本轮不改属性布局。 |
| path_controls 混合按钮高度 | 保留绘制实现；本轮不改命中区。 |
| path_controls 数值字段 | 保留绘制实现；本轮不改输入字段。 |
| path_controls 布尔字段 | 保留绘制实现；本轮不改控件。 |
| effect_controls 行步长 | 保留绘制实现；本轮只读取既有属性所有者。 |
| effect_controls 布尔字段 | 保留绘制实现；本轮不改控件。 |
| effect_controls 枚举字段 | 保留绘制实现；本轮不改控件。 |
| effect_controls 悬浮色 | 保留绘制实现；本轮不改颜色。 |
| effect_controls 层级 | 保留绘制实现；本轮不改树形布局。 |
| timeline 列坐标 | 保留绘制实现；新增 getter 不改列坐标。 |
| timeline 吸附线 | 保留绘制实现；本轮不改拖动反馈。 |

定向格式、`git diff --check`、`verify_style_consumers()` 通过；无新依赖、第三方资产、工程格式或渲染语义变更。

## 关键帧剪贴板：排队前来源绑定

在 `fc5276f4` 的 UI 接线之后，补齐 KU-05 的异步读取边界。宿主运行时 snapshot 增加已有 `document_generation`，不写入工程 JSON，不更改工程版本或渲染语义。属性选区保存 root、composition、generation、revision，并随 Action、Shell 和 typed 请求传到串行引擎。引擎在查找对象或捕获关键帧之前检查来源；不匹配或标识缺失返回正常 Superseded。控制器收到后不更新缓存、不排队删除/粘贴，并复用既有中英文状态提示。读取成功后的完整 Stamp 提交校验仍保留。

| ID | 分类 | 实现与证据边界 |
| --- | --- | --- |
| KS-01 | 功能边界修复 | UI 生成动作时绑定来源，替代“到引擎读取时才绑定”的窗口；旧请求不能因对象 ID 在新合成中重复而捕获/修改新对象。 |
| KS-02 | 正常分支 | 来源失效/缺失作为正常 Superseded，不记 Error、不重试、不覆盖前一缓存，不把它改为成功复制或成功编辑。 |
| KS-03 | 已有观察 | 真实读取错误、终止 panic、阻塞前 Info/in-flight 和默认 Error 信道继续沿用原桥接；本阶段不新增日志站点、退化计数器、线程或每帧 IO。 |
| KS-04 | 验证边界 | 新真实队列测试以 TestWait 阻塞工作线程，先排合成 open，再排原合成复制；随后旧 Cut/Paste 同样失效，当前工程、修订号和撤销状态不变。继续在新合成粘贴，验证仍是旧缓存的值，并可一次撤销。 |
| KS-05 | 剩余范围 | 文本、路径独立剪贴板，完整系统输入/打包程序、当前主线组合与 CI 尚未完成；#204 继续开放。节点独立缓存沿用既有实现。 |

同目录同修订号重开测试增加读取阶段的 generation 检查，并测试 root、composition、generation、revision 各自缺失时，在解析不存在对象 404 之前就正常拒绝。默认信道子进程另用过期修订号请求 404，确认此时没有 Error；紧接着使用有效来源请求 404 仍产生原始错误，未静默处理真实失败。受控阻塞仍为已有 TestWait，不冒称关键帧计算内部可注入阻塞。

首轮 `keyframe-clipboard-source-first.log` 实际 51 项通过、0 失败、0 忽略，944 项未执行，用时 57.32 秒。该结果早于来源缺失夹具的补充，不代替下述最终验证。没有修改既有断言、日志阈值或超时来使测试通过；既有新夹具只补传实际 snapshot 来源，受控阻塞用例在进入阻塞前捕获来源，避免测试自身同步读取工作线程。

最终 `keyframe-clipboard-source-final.log` 使用相同 FFmpeg 桌面 clipboard 过滤命令，实际 51 项通过、0 失败、0 忽略，944 项未执行，用时 56.15 秒；编译用时 1 分 14 秒，22 项既有警告。`keyframe-clipboard-source-native-final.log` 在同一最终测试文件执行两项真实 native 图层/关键帧回归，均通过，用时 55.86 秒；主窗与浮窗、中英文菜单/快捷键、保存重开与撤销继续成立，输入边界仍是生产组件 Input，未升级为系统事件验收。

`keyframe-clipboard-source-default-channel-final/` 为本阶段默认级别文件日志、Error stderr、stdout 原件；过期来源的 404 正常拒绝，随后有效来源的 404、占帧和受控终止仍进入原错误信道。实际默认信道检查通过，未把产品 panic 隐藏为成功或修改排队请求的断链协议。最终桌面测试文件 SHA-256 为 `098cfa3d0d5f7385f7ff9f9e7ced0c9cc3d1a7e434b878bf15fe38f962bae7a0`。

新原始关键帧截图在 `keyframe-clipboard-source-ui-native/`，图层截图在 `keyframe-clipboard-source-layer-native/`，沿用前述六图和八图文件名；本阶段没有改动外观。DESIGN §0.7 十二项逐项沿用前表的保留绘制实现处置，未冒称视觉整改。定向 rustfmt、`git diff --check` 和 `verify_style_consumers()` 通过；未新增依赖、第三方资产、样式 token、工程格式或渲染语义。#204 尚未关闭、尚未创建完成 PR。

## 文本剪贴板：原生编辑器与应用缓存

继续 #204。实施前再次核对近期本地分支和开放 PR，当时开放列表为空，没有发现文本剪贴板重叠实现；没有审查其他人的 PR，也没有操作其他工作树。

新增 Shell 拥有的 `TextClipboard`，主窗 draw 与浮窗 present 都把同一 Arc 传入 Input；借用 InputView 及显式 to_owned 保留该句柄，不复制正文。缓存存储独立的 Arc<str>，不引用来源编辑器或工程对象，也不与图层、关键帧、节点缓存互相覆盖。此为应用内文本缓存，不接操作系统剪贴板，没有新增平台 API、线程、全局单例、依赖或第三方资产。

现有 `TextEditor` 支持 Ctrl+C/X/V，保留现有全字段选择与字素光标规则；初次编辑或 Ctrl+A 全选，没有新增部分范围选择。剪切仅在缓存成功后清空正文，粘贴沿用单行字段的控制字符过滤，在字素边界插入。每次剪切/粘贴独立记录一份草稿快照，Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y 操作本地历史；字段提交仍复用原来的单次工程命令。普通输入与删除也记录草稿历史，开始新编辑时重置；Esc、失焦、IME、自动重复与正常空缓存沿用原输入所有权。

缓存正文上限 16 MiB；每个编辑器的 undo/redo 栈各自最多 16 MiB 内容和 128 步，达到历史保留上限时丢弃最早快照，不截断单个正文。正文或当前编辑超过 16 MiB 则明确拒绝并保留先前草稿。此为新增文本编辑/历史的资源边界，不能当作纯观测改动隐去。沿用既有字段绘制、选择样式和错误样式，没有改颜色、行高、命中区、菜单或右键菜单。

| ID | 分类 | 实现与剩余边界 |
| --- | --- | --- |
| TC-01 | 新功能 | 独立应用文本缓存、借用输入共享、缓存成功后剪切、字素插入和草稿单步撤销；现有 TextEditor 字段接入。 |
| TC-02 | 新增故障观察 | Cache 仅 try_lock，不等待或重启；busy、poison、超限返回明确错误，保留前一缓存。每个实际用户请求 Info start、Debug 完成计时、Error 原因；不记录正文，不向 stdout 写日志。不新增轮询/退化计数器；单个用户操作错误直接报告，不当作第 60 次轮询故障。 |
| TC-03 | UI 错误反馈 | 缓存缺失、复制/粘贴失败或草稿超限保留正文/选择并设置既有字段 error；已在缓存层观察的错误不重复记一条。日志在锁释放后写入；正常空缓存和空撤销不记 Error。 |
| TC-04 | 已有边界 | 引擎提交、保存、panic hook 和 Error 文件/stderr 分流沿用原实现，不吞 panic、不清 poison、不改变协议返回；缓存不做 OS 调用或阻塞等待。 |
| TC-05 | 待完成 | 时间轴/效果搜索、旧属性字段、旧蒙版参数弹出编辑及旧对话框仍有字符串输入，尚未全部接入。文本 Edit 菜单所有权、路径独立剪贴板、完整系统事件/打包、当前主线组合和 CI 继续待完成。不能据此关闭 #204 或宣称所有文本入口已支持。 |

### 文本阶段的验证过程

`E:/Dev/aem/artifacts/pr-ci-followups/text-clipboard-cache-first.log` 实际通过 2 项缓存测试：冻结数据独立、正常空缓存、受控锁占用立即返回、超限保留旧值及 poison 明确拒绝；poison 的 catch_unwind 仅用于测试注入，产品没有吞 panic 的分支。`text-clipboard-ui-first.log` 通过 33 项：借用输入 6、既有故障边界 4、字素编辑 9、输入所有权 9、新文本剪贴板 5。该轮在方法拆分和新增历史预算测试之前，是阶段结果，不代替最终树验证。

首轮桌面 `text-clipboard-desktop-first.log` 实际 53 项中 52 通过、1 失败：新夹具把主窗时间轴 NameEdit 的 owner 误写为 None，实际主窗和浮窗时间轴均为 Some(Timeline)。改正新夹具后第二轮 `text-clipboard-desktop-second.log` 为 54 项中 52 通过、2 失败：新断言遗漏原有 InComposition 命令包裹；工程切换后的根目录为 Windows canonical 长路径，不能直接与未规范化的 TempDir 字符串相等。修正为严格验证目标合成、内层对象/名称以及两端 canonical 路径相等，未改变生产路由、原命令或既有断言。原始失败日志保留。

### 文本阶段最终验证

最终桌面 `text-clipboard-desktop-third.log` 使用 `cargo test -p motion-desktop --features ffmpeg --bin motion-studio clipboard -- --test-threads=1 --skip native`，54 项通过、0 失败、0 忽略，945 项未执行，用时 80.74 秒，保留 22 项既有编译警告。实际验证草稿剪切后一步撤销、不变名称提交不产生工程命令、粘贴改名的目标合成/对象、保存重开与工程一次撤销；图层缓存不被文本覆盖，真实工程替换后文本缓存仍独立存在。

`text-clipboard-default-channel-final/` 保留默认级别子进程文件日志、stderr 和 stdout。缺失服务与超限均经过真实失败分支，Error 同时写文件和 stderr，stdout 无错误；Info start 仅进文件。私有正文哨兵不出现在文件或 stderr，正常空缓存不产生 Error。busy 与 poison 在缓存单元测试中受控注入；未冒称这两条也完成应用日志子进程验收。

`text-clipboard-ui-library-final.log` 为 4 项通过、0 失败、3 项既有 GPU 测试忽略、58 项过滤，用时 0.02 秒。通过项含两个缓存边界、历史内容/步数预算及既有文字裁剪；本次使用 `--no-default-features`，没有把原有 GPU ignore 当作通过，也没有更改忽略属性。`text-clipboard-ui-integration-final.log` 的借用输入、故障边界、字素编辑、输入所有权与新文本集成共 33 项全部通过。方法拆分与历史预算测试均已纳入这一最终树。`text-clipboard-ui-consumers-check.log` 的 `cargo check -p motion-ui --no-default-features --tests` 通过，用时 1 分 58 秒；它验证其余测试消费者可编译，不代替执行。

`text-clipboard-native-first.log` 的新增真实 Windows HWND/wgpu 测试 1 项通过，用时 13.88 秒。随后在同一桌面测试文件执行 `text-clipboard-native-regression-final.log`：既有图层、关键帧和新增文本三项均通过、无失败或忽略，用时 75.99 秒。主窗→浮窗英语 1280×800/100%，浮窗→主窗中文逻辑 1280×800/150%，覆盖剪切、草稿撤销/重做、跨窗粘贴、精确单次 Rename、保存重开和工程撤销。输入通过生产 Input/组件调用，浮窗测试 helper 与实际 present_float 注入同一缓存；没有验证 OS 键鼠注入、完整 window_event 或系统剪贴板。

本轮最终四张原图位于 `text-clipboard-ui-native-final/`：`main-to-float-en-source-undo.png`、`main-to-float-en-target-paste.png`、`float-to-main-zh-source-undo.png`、`float-to-main-zh-target-paste.png`。已实际查看两张目标粘贴图，字段可读；截图为未提交草稿，属性栏仍显示 Target 属正常状态。截图只捕获 UI，不能代表合成渲染验收。保留 native stderr 中本机 Epic EOSOverlay JSON 缺失的 Vulkan loader 报错，未吞掉或更改注册表。最终桌面测试文件 SHA-256 为 `143bdc87f12b494c302dfb6a5b60886b6328657e27d9bc87aa970110f5499ef9`。

本阶段没有修改面板绘制；DESIGN §0.7 十二项逐项沿用前表的保留处置，未宣称视觉整改。未新增依赖、第三方资产、样式 token、工程格式或渲染语义。TC-05 仍为待完成范围，#204 保持开放，尚未创建完成 PR。
