# 字体原生错误与作者窗口回复验收（#138）

基于样式错误 PR #631 的 `62e7c0c8` 接续。本次只补齐主字体选择、塑形初始化和相应桌面回复边界，不代表 #138 全部完成。

| 项目 | 处置 | 实际证据及范围 |
|---|---|---|
| FONT-IO | 新增 `FontError::Candidate`、Open/Read 阶段，保留实际 `std::io::Error`、路径及 face index | 缺失文件和目录作为文件的真实 OS 错误；目录失败阶段由平台决定 |
| FONT-PARSE | 保留原始 `ab_glyph::InvalidFont`，塑形错误保留 `ShapeError` | 无效字体字节和不合法 collection index；通过 `source()` 向下转换，非字符串识别 |
| FONT-EMPTY | 无可用字体是独立 `NoUsableFont`，保留首个实际拒绝原因及尝试次数 | 空目录集合无原因；坏候选后正常回退仍成功，不将缺少汉字覆盖视为来源故障 |
| FONT-SHARED | 保留 Binary/SharedFile 已持有字节的选择语义 | SharedFile 原路径不存在时仍可加载，检查字节、glyph、family 和 face index |
| FONT-DEGRADATION | 新增每个不可变 catalog 的连续降级选择计数，饱和递增，首轮 Debug、第 60 次 Error，其余抑制；没有坏候选的选择重置 | 两轮真实坏候选后正常回退；每轮前 59 次无 Error、第 60 次恰好一条，后续 60 次抑制；正常选择后再次达阈值。计数单位是选择调用，不是帧或秒 |
| FONT-BOUNDS | 首个拒绝原因保留一个，family/requested 分别至多 96/128 字符；降级日志原因至多 512 字符并清理控制字符 | 超长含换行 family 的实际解析失败日志有界，无多行注入；不记录字体字节 |
| FONT-TRANSPORT | 桌面启动、首选项字体准备、作者窗口准备及 worker 回复保留类型；复用 Host NativeSource 和共享 Engine Error | 真实塑形失败通过真实 worker/日志/reply；clone 指向同一 FontError，既有 envelope 文字仍含 face index/字节数 |
| FONT-CHANNELS | 复用已有 worker 的请求、阶段、耗时、Info 开始及飞行状态；不新增重启或重试 | 移除 MOTION_LOG 的子进程，实际失败恰好一次 Error 文件及 stderr，Info 仅文件，stdout 隔离；失败后同一 worker 处理下一条受控阻塞任务，完成前可读飞行状态，回复后清除 |
| FONT-PRESENTATION | 只在原有 locale action、示例反馈及作者窗口 controller 展示边界转成 String | 不引入按文字重建原因的协议；其他尚为 String 的提供者不宣称已迁移 |
| FONT-REMAINING | 保留范围说明 | 未故障注入整个操作系统字体发现、普通应用的字体启动失败、线程耗尽或硬件故障；首选项持久化等 String 提供者及畸形模型总体验收仍开放 |

主字体纯文件候选改为直接 open/read，再交给同一个解析器，代替 `fontdb::with_face_data` 的 Option 边界及临时映射后复制，以取得真正失败的 IO 对象。这是错误采集所需的读取实现调整，不宣称性能提升。未增加字体大小限制、替换字体资源、改变候选顺序、汉字覆盖偏好或缓存 miss 策略；SharedFile 仍使用已经持有的数据。

降级计数测试使用 UI 库的 Info 日志捕获器，证明阈值、抑制、有界和重置；它不是应用 FileLogger 的端到端测试。原生 worker 子进程单独使用真实应用日志器，证明实际 FontError 的文件/stderr/stdout 路由。这两类证据分别记录，不将捕获器当作原生文件输出。

没有新增依赖或第三方资产；系统字体只用于本机测试，不纳入仓库或证据包。没有面板、token、尺寸、交互、工程格式或渲染语义修改；DESIGN.md §0.7 各项既有面板整改保留，原因是本批不改呈现层，不以编译替代 UI 验收。

既有字体选择测试只适配返回值的 `.selected`；所有 family、覆盖、collection index、塑形/光栅一致性断言保留。既有 worker panic 测试仅显式 `to_string()` 后执行相同 contains 检查，没有修改断言范围、超时、预算或忽略条件。

## 本机验证

Windows，Rust 1.97.1，FFmpeg 8.1。每个阶段保存真实命令、输出、退出码、环境白名单及完整源码前后 SHA-256。产品与测试共 14 个 Rust 文件的最终身份另由 `verify.py` 核对。

| 验证 | 结果 | 原始记录前缀 |
|---|---:|---|
| 完整 UI 库，含新增 6 项及既有字体选择、汉字覆盖、collection/塑形一致性 | 141 通过，0 忽略 | ui-lib-v1 |
| 样式集成测试 | 6 通过，0 忽略 | ui-style-v1 |
| 新增实际 worker 字体故障、原生身份、默认信道、阻塞状态与恢复 | 1 通过，0 忽略 | worker-native-v1 |
| 显式执行既有 worker 阻塞、panic、观察锁 poison 故障测试 | 1 通过，0 忽略 | worker-lifecycle-v1 |
| 既有样式重载及实际原生错误回滚 | 3 通过，0 忽略 | shell-styles-v1 |
| 既有首选项、快捷键、重启与失败回滚 | 6 通过，0 忽略 | shell-preferences-v1 |
| 既有默认故障恢复与业务锁释放 | 2 通过，0 忽略 | shell-fallback-v1 |
| UI 全目标编译 | 通过 | ui-all-targets-v1 |
| FFmpeg 桌面全目标编译 | 通过 | desktop-check-v1 |

合计 **160 项不同测试，其中新增 7 项**；字体专项重复执行不累加。显式执行既有 ignored 测试不改变其标记。不声称全工作区、完整桌面或跨平台验收已经完成。

## 负对照与原始失败

仅令 `FontError::source()` 返回 None，保留原错误对象、显示文字和全部断言。真实运行 6 项字体测试：3 通过、3 失败，退出码 101；缺失文件、目录来源和解析失败的原生来源检查均拒绝该回退。随后逐字节恢复文件，SHA-256 为 `ab4af2a2e8142943589756dc06194436576e3b9f983ec9780b7ec5e335714528`，相同 6 项再次通过。

首次负对照驱动误用子 Windows PowerShell 加载未签名脚本，执行策略拒绝，测试没有开始，退出码 1。finally 已恢复原文件；该次不能算有效负对照。改为从已配置的父运行器直接执行 Python 命令包装器后，才取得上述真实的 3 项断言失败。保留首次退出及恢复身份记录 `negative-driver-v1-failure.json`，没有变更系统执行策略或测试断言。

本地证据：`E:/Dev/aem/artifacts/issue-goal-current/font-error-causes`。包含原始成功/失败输出、默认文件日志及 stderr/stdout、故障报告、负对照源码/恢复摘要、最终源码身份、命令包装器及源码快照；不包含系统字体。`verification-summary.json` 记录去重测试范围，归档成员及 SHA-256 另见 `evidence.json`。远端 CI 单独跟踪，不用本机结果代替。
