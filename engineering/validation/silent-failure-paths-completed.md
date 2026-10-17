# 静默失败路径处理完成记录

审计源：`silent-failure-paths-exhaustive.md`；辅助定位：`silent-failure-paths.md`。分支：`fix/silent-failure-paths`，当前基线为合并 #305/#306 后的 main `1750d8d`。原始清单保留，本文只登记已核实的结论。

全集机械枚举共 4443 行，其中 832 行未标记 no-loss/test-only/guarded。这个数量是待审候选数，不等于已确认缺陷数；既有标签也不能替代源码核对。以下记录未覆盖的条目仍待审，不声明全量完成。

## 01 · worker 响应断开与启动快照

状态：已修复并通过针对性测试。

- `desktop/app/src/main.rs`：attach / present / resize / freeze_movie 的轮询区分 Empty 与 Disconnected，断开时进入既有错误处理并清空等待槽位，避免无限等待
- `desktop/app/src/engine/failure.rs`：集中提供 poll_reply，保留原始成功/失败结果，断开错误携带 operation 及 worker failure
- `desktop/app/src/engine.rs`：启动快照失败直接通过 ready 返回原始错误，避免以 initial=None 继续启动
- 新增测试覆盖四类 operation 的 pending、成功、失败与断开；既有真实 worker panic、恢复副本、资源释放及关闭测试仍通过
- 验证：`cargo test -p motion-desktop --offline engine::failure::tests -- --test-threads=1`，9 passed
- 关联辅助清单：0.6、1.1–1.4、2.31、10.5

## 已核实应保留的行为

| 位置 | 结论与源码证据 |
| --- | --- |
| `crates/motion-render/src/effect_plan.rs:314` | fingerprint 同时写入低 32 位和高 32 位（下一行），属于完整拆分编码，不是丢失高位 |
| `crates/motion-model/src/model.rs:649`（辅助 0.1） | migrate 在升级前 validate，validate 明确拒绝 1..=18 之外版本；4..8 无须重写内容，已有视频/旧工程迁移回归测试。升级后复验可作为额外防线，不支持“未知版本被盖成 8”的断言 |
| `crates/motion-core/src/expressions/mod.rs:15` | 序列化的是 ExpressionTarget 错误定位信息，并非表达式体；该枚举仅含整数、字符串和可序列化枚举。清单所述“表达式体被清空”不成立 |
| `crates/motion-model/src/vector/groups/sampling.rs:188,189,198` | 饱和乘法后与小于 usize::MAX 的严格资源上限比较；溢出会触发拒绝，不能改为溢出回绕或记为静默通过 |
| `crates/motion-core/src/history_accounting.rs` | JSON 长度累加饱和为 usize::MAX 后被预算拒绝，是保守计数，不是漏算 |

## 前置观测改动

`4fbba52` 已完成文件日志、panic hook、worker Flight/排队深度、图片等待与 surface 失败计数及 trim 记账检查。对应 PR 为 #306；它仅补可观测性，不代表清单中的行为问题已全部修复。详见 `engine-observability.md`。

## 验证与样式边界

编译环境：`. .\.tools\msvc-portable\env.ps1`。原始测试输出保存到 `target/silent-failure-validation/`。

本轮不做视觉重排。涉及 panel 错误处理时仍遵循 DESIGN.md；§0.7 的既有视觉整改项按实际情况记录，不将编译成功当作完整 UI 验收。

## 02 · 异步任务保留 panic 原因

状态：已修复并通过媒体与桌面测试。

- `motion-media` 增加有界 panic_reason，保留 String / &str 原因，未知载荷有明确 fallback，按字符截断到 2048，避免 UTF-8 切断
- 音频导入/缓存、视频导入/缓存、工程包导出、host 视频解码、桌面音频混音、节点任务、movie freeze/export 均保留 panic 原因及原有操作前缀
- 图片解码保留相同有界原因，不为此给 renderer 新增 media 运行时依赖
- 新增真实 source worker panic 测试：失败终态包含注入原因、任务可释放、没有发布残留；补强混音 panic 测试的原因断言
- 验证：`cargo test -p motion-media --offline` 共 56 passed，0 failed；原有两个大文件验收仍按仓库配置忽略（其中一个需要外部 >512MiB MP4），未改其断言或忽略属性
- 验证：`cargo test -p motion-desktop --offline -- --include-ignored`，322 passed，0 failed，0 ignored
- 关联辅助清单：3.2–3.12、10.7、10.10、10.12、10.20、10.25、10.27、10.30、10.32、10.36

核实保留：`AudioJobs::cancel` 与 `VideoJobs::cancel` 在任务锁内先写入 cancelled 终态，再设置/观察取消标志；worker 的 cancel → return 不会把可查询任务留在 opening/decoding。现有阻塞 source 打开后取消、取消后不提交、任务释放等测试通过。辅助 10.24、10.26、10.31 关于取消后非终态的断言不成立。

## 03 · 诊断服务自身的静默路径

状态：已修复；诊断测试 9 passed。

- context/GPU 元数据与时序环形缓冲区分锁，report/record 争用不会再返回伪造的默认 Context 或丢掉 set_context/set_gpu
- 保留 record 的 try_lock 丢弃计数策略，避免记录线程等待报告生成
- reporter spawn 失败增加 localWriteErrors 并写日志；无效 DSN 记录明确提示，不输出可能含凭据的原值
- 超预算/序列化失败向 export/checkpoint 的 reply 明确返回 Err，并增加错误计数，reporter 继续处理后续任务
- 临时文件仅在本次成功 create_new 后清理，清理失败记录路径；不删除 create_new 碰撞时不属于本次的文件
- 新测试在持有 report 状态锁时更新并读取 Context/GPU；超预算测试验证收到具体 Err，而非断开/超时，且清除大记录后可正常导出
- 验证：`cargo test -p motion-desktop --offline diagnostics::tests`，9 passed

已落地提交：01 为 `1d2909b`，02 为 `1d57076`；后续提交可按各节标题在 git log 中定位。

## 04 · 控件失败边界与文本裁切

状态：已修复；motion-ui 全套测试 78 passed，0 failed，0 ignored。

- TextEditor、NumberEditor、状态按钮和全局 key match 使用 Input.key_presses 的 IME 过滤；按钮和确认动作排除重复按键，文字删除/光标移动保留 repeat
- 数字字段配置变为非法时，取消已有拖拽并返回原始值的 Cancel，避免丢失已开始事务的闭合事件；保留错误提示
- ellipsize 在连省略号都放不下时返回空串；原文已能放下时仍直接保留原文，正无穷宽度可正常使用
- 新增 4 项集成测试，覆盖 IME 提交文本、重复键、按钮激活、非法配置拖拽取消、极窄宽度和 NaN
- 验证：`cargo test -p motion-ui --offline`，78 passed
- 关联辅助清单：6.4、6.7、9.17

核实保留：原生 apply_event 本身已过滤 IME confirmation keys，新增控件保护同时覆盖直接构造 Input 的库调用方；clipped_input 保留 held/released 是为了完成离开控件后的指针捕获手势，不能机械裁剪，否则会制造未闭合事务。preview_binding 会调用 check_node，后者已检查 RGBA 0..1，辅助 2.22 的拖拽/键入范围不一致断言不成立。

本节未编辑 panels、颜色、尺寸 token 或控件布局；DESIGN.md §0.7 的既有视觉整改项继续保留。原生窗口截图/行为总体验收留待本轮最终集成验证。

## 05 · 生成器与 CI 漏检

状态：已修复；新增 5 项回归测试，工具测试共 46 项，45 passed、1 项既有 Rust effect validator 外部工具测试未运行。

- creative/spatial 生成器对关键模板替换检查匹配数量，模板漂移时明确失败，避免无效 replace 仍生成看似成功的包
- 保留可选参数占位符的替换语义；真实 creative 模板构建和 spatial 临时目录完整生成通过，未重新发布或改写库资源
- panel 样式检查改为递归，新增嵌套目录的违规与正常样例验证；不修改任何 panel 源码
- 发布文本不能 UTF-8 解码时明确报错；图片/字体二进制按显式后缀排除文本检查，仍受资产来源约束
- 两个新增工具已登记 validation_inventory，未降低工具登记或运行时断言检查
- 验证在跟踪源码快照中运行全部 test_ci_*.py，日志为 target/silent-failure-validation/tools-batch05.log。本机 tools/package_macos.py 是用户原有未跟踪文件，会触发完整性检查；保留它原样，未为测试通过放宽检查或纳入提交
- 关联全集 B9：模板 replace 与 validate_provenance 非 UTF-8 跳过；辅助 7.1 的嵌套 panel 扫描

## 已合并的前置 PR

- #305：0d5b5bd，修复工作区切换黑屏与视口越界
- #306：1750d8d，补齐引擎命令与静默降级观测
- 两个 PR 均在 CI 全绿后，按用户授权以中文标题和正文 squash 合并

## 06 · host 导出与音频状态的真实失败

状态：已修复；cargo test -p motion-host --offline 全套 41 passed，0 failed，0 ignored。

- 效果资源解码失败携带资源路径返回 Err，不再把失败编造成 0×0；有效 PNG 尺寸与坏 PNG 回归通过
- export render_plan_info 的 audio_voices 失败明确拒绝导出并保留 audio plan 错误
- 快照正常情况下继续返回布尔 has_audio；音频规划失败时 has_audio 为 null，并提供 audio_error，避免把未知状态声称为静音；同一 session 同一错误只 warn 一次，恢复成功后重置
- 新增真实边界测试：16 层合法合成使用不同质数帧率与时间偏移，触发有理数计算预算溢出；验证状态包含原因、导出返回具体 Err，以及改回正常工程后恢复 false/null
- file_opener 拒绝非普通文件，metadata 错误明确返回并保留路径；普通文件保留准确长度和原始内容
- legacy 切换工程仍保留既有“资产不可验证时不自动保存”的兼容策略，但明确 warn 记录 root 和原因；显式 Save/Discard 契约不变
- 核实：原 4.15 所称 bytes=None 会无限接受大文件不成立，worker 还会在流式复制中独立限制实际字节数；改动解决 metadata 错误被隐藏的问题
- 核实：包加载已经校验全部图片资源并完整解码，4.4 并非普通损坏包的绕过通道；仍移除导出端的伪造尺寸，作为真实失败边界保护
- 关联辅助：0.8、4.4、4.15；原始测试输出：target/silent-failure-validation/host-batch06.log

## 07 · 插件包发布与卸载事务

状态：已修复；cargo test -p motion-effects --offline 全套 38 passed，0 failed，0 ignored。

- 内置包、用户安装包和 disabled.json 使用独占创建的临时文件；写入、sync 或 rename 失败及 unwind 时清理本次文件，不碰其他事务的文件
- 临时名含进程与原子序号，碰撞重试有明确上限；清理失败记录文件路径和 IO 原因
- uninstall 先提交禁用集合，再移除包文件；metadata 提交失败时包字节与内存集合均保留
- 包文件删除失败时恢复原禁用状态；恢复本身失败则明确返回两次错误，不伪称卸载成功
- 新增 4 项回归覆盖：原子发布失败的自有文件清理、foreign 临时文件保留、metadata 失败不删包、删除失败恢复禁用状态、正常卸载完成（部分断言同属一项测试）
- 使用的 log 是工作区已有依赖，未增加新第三方资产或复制内容
- 关联辅助：11.23–11.26；原始日志：target/silent-failure-validation/effects-batch07.log

## 08 · 媒体缓存成对发布与关闭清理

状态：已修复；cargo test -p motion-media --offline 共 63 passed，0 failed；2 项既有大型外部素材验收仍按原配置忽略，未改断言。

- audio PCM/waveform、video timestamps/thumbnail 采用同一成对发布事务；第二文件失败时删除本次新文件并恢复先前缓存
- 备份放在独占创建的 recovery 目录中；回滚本身失败时保留旧字节，错误携带恢复目录，不会被导入 Stage 的 Drop 连带删除
- unwind 也触发回滚；成功后删除旧备份，清理失败记录路径与原因；没有引入逐帧日志
- audio/video 导入回滚删除失败加入原始任务 error，并写 warn；Stage 清理失败也可定位
- AudioJobs::Drop 使用独占 get_mut 获取中毒 registry 内部值，仍向所有已有任务发送取消；不恢复正在运行的业务操作、不重启线程
- 新增 7 项回归，覆盖第二文件失败时恢复旧缓存/移除新增文件、正常成对替换、unwind 回滚、回滚失败保留恢复副本、删除失败保留主错误、真实阻塞 source 与中毒 registry 关闭取消
- 核实 11.19 的“覆盖上轮合法导入缓存”断言不成立：导入路径的 stem 来自独占创建、带时间戳与 nonce 的 Stage，不与上轮导入复用；缓存重建仍需要本节的成对发布修复
- 核实 0.3：tfhd/trun 本身可以提供 duration/size，缺 trex 不等于数据损坏；选中 sample 的 duration()/android_sample_end() 明确拒绝 duration=0，不能机械禁止所有无 trex 的片段
- 关联辅助：1.12、11.10–11.20；原始日志：target/silent-failure-validation/media-batch08.log

## 待核验的重要清单断言

- 当前 renderer 的异步 demand 会在 busy 结束后启动非 speculative decode；其错误进入 Err。现有 image_preview_cache 已包含“坏预取资产转为 active 后必须返回 image 9 错误、不能永久 pending”的真 GPU 回归。原“只因没有检查 failed 集合就一定永久 false”推断与当前代码不符；本轮真 GPU 实测已确认，详见 09

## 09 · 图片代理缓存的失败与临时文件所有权

状态：已修复；renderer lib 与图片资源测试通过；image_preview_cache / image_working_set 真 GPU 回归共 4 passed。

- CACHE_LOCK 仅保护磁盘操作，不持有可损坏的内存对象；中毒后恢复锁并 warn，所有缓存读取继续验证 header/长度/hash，不重启 renderer 或跨线程移交 GPU
- 写入、驱逐、目录不可用等失败采用首次 debug、连续 60 次 error 的限频信号，成功后复位
- 替换前保留原代理文件；create_new 失败不安装清理 guard，避免删除另一个写入者的临时文件（原代码在创建失败时也执行 remove_file）
- 自有临时文件有 RAII 清理；cleanup 失败限频 warn，路径可定位；预算统计与正常缓存命中行为保持
- 新增 3 项回归：锁中毒后真实读写恢复、临时名碰撞保留 foreign 文件与原缓存、rename 失败清理本次文件并保留目标
- 原坏预取转 active 的真 GPU 回归已实测通过，返回 image 9 错误，没有永久 pending；无需将失败集合改为 active 资产的永久黑名单
- 图片资源测试中唯一未执行项为既有外部 MOTION_IMAGE_SOURCE 本地图片探针，未更改其 ignore 或断言
- 关联辅助：1.10、11.27–11.28；日志：render-batch09-final.log、render-batch09-gpu.log，位于 target/silent-failure-validation

## 10 · 节点交互取消与能力查询请求

状态：已修复；cargo test -p motion-desktop --offline -- --include-ignored 全套 328 passed，0 failed，0 ignored。

- 能力查询必须通过大小限制、JSON/schema 解码并确认为 media_capabilities；坏 JSON、超限内容、错误命令、错误字段类型和未知字段明确 Err，不再回退为无筛选能力答案
- 节点拖拽的 Escape、失去按键、焦点取消在确有变化时恢复文档并提升非语义 revision，保持语义/preview revision 与 undo 栈不变，使恢复对外可观察
- 画布缩小时取消未闭合的节点拖拽和按住中的 nudge；保留文本字段，不新建 undo 记录；不改布局尺寸、样式或 panel 文件
- 新增 4 项回归包含真实 pointer 输入、按键与 draw 响应；验证恢复后的 changed/semantic_changed、文档、拖拽状态和历史
- 关联辅助：5.20、9 类节点布局取消；日志：target/silent-failure-validation/desktop-batch10.log

## 新核实的保留项与误报

| 位置/辅助编号 | 源码结论 |
| --- | --- |
| animation.rs:spatial_components / 4.6 | 支持 N=2..4 的向量投影到三维；二维 z=0 是有意补齐，并非把缺失的创作轴改零 |
| audio_levels.rs / 4.12 | 默认左右为 0 dB，db_gain(0)=1，是原始音量；字段缺省不等于静音，也不是丢失已有 envelope |
| particle_history.rs / 4.13 | source_layer 是可选外部空间源，未指定时用当前 emitter；显式指定但不存在的 layer 会返回 Missing，不会把坏 id 别名到 emitter |
| vector/groups.rs / 4.11 | 比较时的 MAX fallback 后，move_key/copy_key 仍使用 local(to)?，溢出会拒绝编辑；不是永远不匹配后静默接受 |
| vector/groups.rs / 4.14 | find 是 Option 查询，edit_key 紧接着把 None 转为 group parameter missing；坏 node:path:node 不会成功丢弃编辑 |
| renderer GPU / Engine failure latch / 1.5–1.6 | 出错后终止使用同一资源/worker 是失效边界；不清 latch、不在本任务中重启 GPU 或跨线程搬迁 session |
| shell/lifecycle.rs / 1.11 | 清空等待队列发生在 failed worker 的关闭流程；故障已被记录且提供 recovery，断开的队列不能阻止进程退出 |
| node_editor/view/mod.rs / 0.5 | 允许不完整、暂不能编译的图继续编辑；元数据非法仍拒绝。不能机械地把每个结构验证错误都变成禁止修改字段 |

## 11 · 媒体导入与分析的终态处理

状态：已修复；shell 17 项通过，随后桌面全套 331 passed，0 failed，0 ignored；cargo check 与桌面/effect_tool 构建通过。

- 媒体状态查询失败时清空 importing_media，异步请求取消/释放，不再永远拦住下一次导入
- 只把真实 running 当作进行中；未知/缺失/类型错误的 state 带 request id 返回协议错误，导入与音频分析都会退出等待
- ready/failed 的任务释放失败写 warn，不覆盖 finish import 或 worker 的原始失败原因；实际成功导入也不会被清理错误伪装成没有导入
- 新增 3 项测试：未知状态拒绝、真实不存在任务的查询失败后可继续轮询、真实损坏音频导入的终态释放且工程未改
- 保留 respond 中无人接收异步结果的既有丢弃契约
- 关联辅助：5.21；日志：shell-batch11.log、desktop-batch11-full.log、check-integrated.log、build-integrated.log

## 12 · 获准修复的测试夹具与原生验收

状态：已完成；用户明确授权 panels 禁改规则的这一行例外。

- 递归样式门禁实际找到 mask_inspector.rs:680 的既有 cfg(test) 初始化；仅将 default_stylesheet() 改为 PaintList::default().styles()，全部断言原样保留
- 没有修改该文件的产品绘制函数、颜色、尺寸、布局、控件或交互；本轮 panels 差异仅这一行测试初始化
- 原 narrow-mask 测试通过；完整 tools/ci_desktop.py 通过两种语言、浮动窗口、五种工作区/回切、活跃节点 overlay、独立和绑定节点预览、共享 MCP、样式覆盖
- 已人工查看中英文主窗口截图，内容区正常绘制；DESIGN §0.7 的既有视觉整改项全部 retained with reason：本轮为错误边界与测试夹具修复，不做视觉重排；未宣称修复那些已有视觉问题
- 工具全套带 MOTION_EFFECT_VALIDATOR 重跑：46 passed，0 failed，0 skipped；此前 05 中依赖外部验证器的未执行项已补跑
- 默认 MOTION_LOG 未设置的真实启动：123 条普通命令 start，0 条 Present start；debug 下无健康 Present 的逐帧 done/dispatch；日志轮转、MCP 三条 JSON-RPC 响应和 CLI 命令目录无污染均验证通过
- 原生验收日志：target/silent-failure-validation/native-integrated.log；双语言截图/报告：native-integrated/；日志与协议验收：runtime/runtime-summary.json
- 当前批次基线对应前 11 个业务修复提交；后续新业务改动仍须按影响范围补验，不能用本节覆盖未知改动

## 13 · 日志 sink 自身中毒后的持续可用性

状态：已修复；logging 3 项回归通过。

- 真实 Display::fmt panic 会中毒 writer Mutex，原普通 log()/flush() 的 if let Ok 随后会永久吞掉所有新日志
- 普通写入和 flush 现在恢复 BufWriter/File 的锁，并先补换行分隔被中断的半条记录；正常记录的等级、缓冲和 flush 策略保持
- panic hook 分支继续只用 try_lock；拿不到锁时直接 stderr，不走恢复 helper，不等待自己持有的锁
- 新增真实 formatter panic 测试，验证中毒确实发生，随后 warn 仍完整写入同一文件并恢复锁；已有捕获 panic 与持锁 hook 不死锁回归仍通过
- 本节是对新日志层的复核补漏；日志：target/silent-failure-validation/logger-poison-batch13.log

## 14 · 工程和节点源的临时文件清理

状态：已修复；storage_failure_cleanup 2 passed，motion-graph 实际导出/重开回归 1 passed。

- 工程 save 在成功打开自己的 project.json.tmp 后安装 RAII guard，写入、sync、rename 失败或 unwind 都尝试清理；文件先关闭再清理，避免 Windows 持句柄影响删除
- File::create 之前失败时不安装 guard，不删除阻塞创建的既有目录；原工程文件保持不变
- 工程包与节点源临时文件清理失败记录路径和 IO 原因，不把失败当成已清理
- 节点导出仍保留双文件发布失败后的恢复副本；11.22 描述的 rollback 失败与 recovery copy 路径本来就明确返回，是保全旧字节的设计，不删除恢复副本来掩盖失败
- 未改工作流保存契约、工程序列化或任一既有断言；新增两个真实文件系统失败测试
- 关联辅助：11.9、11.21–11.22；日志：target/silent-failure-validation/storage-batch14.log

## 15 · 撤销历史预算淘汰计数

状态：已完成；历史计账常规回归 12 passed，原手动大文档测量已单独运行并通过；host 边界集成测试 2 passed。

- 保留既有 128 条/32 MiB 的撤销历史策略，实际淘汰时递增运行时 evictions
- Engine::history_diagnostics 暴露 undoEntries、redoEntries、budgetBytes、evictions、gestureActive，host 快照通过 history 字段提供给调用方
- 不把历史压力写入工程文件，不改变 undo/redo、gesture 和预算的语义，不增加逐帧日志
- 新回归实际提交 200 次编辑，再撤销全部可用 128 步，验证 72 个淘汰计数与剩余文档；redo 不虚增淘汰；另验证预算不足的淘汰
- host 集成测试验证快照可读取计数且 project 不含历史诊断字段
- 大文档测量原测试的等价性断言通过；测量日志保留，不将一次本机耗时当成性能保证
- 关联辅助：0.7；日志：history-batch15.log、history-batch15-measurement.log

## 16 · 视频帧代次分配的错误传播

状态：已修复；motion-host 全套 46 passed，0 failed，0 ignored。

- 同一源的视频状态锁中毒、PTS 索引缺失和超出可见时间范围明确返回原始错误，不再转成一次新的 generation
- 相同帧继续复用既有 generation；分配失败不推进计数，计数上限遵守解码器的有符号 sequence 契约
- 图层替换视频源时先退役旧 stream，再解释新源时间，避免把旧源时间基准的错误误施加到新源
- 新增 4 项回归覆盖真实 Mutex 中毒、同帧复用及无效时间基准、更换源与计数耗尽
- 验证：cargo test -p motion-host --offline；日志：target/silent-failure-validation/video-generation-batch16.log

## 17 · Error 同时输出文件与控制台

状态：按用户后续要求完成；桌面全套 333 passed，0 failed，0 ignored；cargo check/build 通过；真实进程验收通过。

- 普通 Error 同时写日志文件和控制台 stderr，Warn/Info/Debug 保持原文件策略；stdout 继续用于命令目录和 MCP JSON-RPC
- panic 的控制台输出由原 hook 保留，文件分支继续只用 try_lock；持锁 panic 不会等待自身的 sink
- 新增独立进程回归验证 Error 双输出、Warn 只落文件、两种记录均不泄漏到 stdout；既有 panic 与日志锁中毒回归通过
- 实际启动默认 Info 桌面程序，普通命令 start 默认可见且 Present start 为 0；轮转正常
- 实际 MCP 无效命令的同一 request/operation/ms 错误同时存在于日志和 stderr，stdout 三条响应均能独立解析为 JSON-RPC；--command-list 仍为纯 JSON
- 本节按用户新要求调整了最初仅写文件的约束，不添加控制台窗口或 UI 交互
- 证据：desktop-console-full.log、console-check.log、console-build.log、console-runtime.log、runtime/runtime-summary.json，均位于 target/silent-failure-validation/

## 18 · 桌面导入、节点安装与恢复文件清理

状态：已修复；桌面全套 336 passed，0 failed，0 ignored。

- 仅在 create_new 成功后认领文件，错误返回和 unwind 都先关闭文件再删除；清理失败记录路径与 IO 原因，不覆盖主错误
- 图片导入成功保留源文件，失败或 panic 尝试删除本次创建的文件；节点运行包在写入、安装失败或 unwind 后清理
- 引擎恢复 JSON 只有写入、flush 和 sync 全部成功才保留；恢复过程失败仍传播原始错误
- 新增 3 项真实文件系统回归覆盖拒绝认领既有文件、错误和 panic 的关闭/清理、成功保留及已清理文件；已有真实图片导入/节点安装/worker 恢复测试通过
- 本机 Windows 下验证文件已关闭才删除；强杀进程不会执行 Drop，仍属后续启动清扫议题，不宣称本次解决
- 关联辅助清单：11.1、11.3–11.5；日志：target/silent-failure-validation/desktop-cleanup-full.log
- 11.2 所述代理缓存并非永久孤儿：缓存按内容哈希与字节预算管理，可由后续解码/裁剪回收；不把每次源导入回滚升级为清空共享缓存

## 19 · 节点后台任务与已写入文件的证据

状态：已完成；桌面全套 338 passed，0 failed，0 ignored；既有 graph groups 5 项通过。

- 文件任务在执行前记录默认 Info start 并 flush；后台失败记录 task id、种类、epoch、revision 和耗时，编译/校验拒绝记录 warn
- 保存/导出成功即记录实际磁盘路径，即使随后取消、切换编辑器或丢弃过期完成结果，产物证据仍保留
- 高频预览编译的 start/done、接收端关闭和正常 worker 退出仅用 debug，不增加 Present 热路径日志
- 新增独立进程回归实际制造输出目录被文件阻塞的保存失败，并验证 file/stderr；再保存成功后切换编辑器，验证旧文件确已保存且不会覆盖新编辑器的 source_path
- 保留过期校验、编译、保存结果的 epoch/revision 守卫；过期结果不应覆盖新文档，也不应把新编辑标成已保存
- 新增真实 Ctrl+S 拒绝非法字段回归，证实输入与就地错误和 notice 原本均保留，不新增重复提示
- 关联辅助清单：2.23–2.25、5.5–5.6、10.19–10.21；日志：desktop-node-observability-full.log、group-contract-review.log

## 本轮进一步核实的保留项

| 位置 | 结论与验证 |
| --- | --- |
| node_editor request/commit_field / 5.5–5.6 | 文件请求唯一 Err 来源为 commit_field；该层先填 field.error 与 notice 再返回 Err，因此调用处 if let Ok 不会吞掉错误反馈；新增实际 Ctrl+S 回归已通过 |
| motion-graph ports::group_port | node_port_specs 先调用 node_ports，其 Group 分支通过 graph.ports 验证边界类型与语义后才构造帮助信息；不把帮助信息内部的 Option 查询当作绕过验证；groups 5 项现有集成回归通过 |
| host ops/media::release_frozen_video | 返回 released:false 是幂等释放的明确结果，已区分本次是否真正持有资源；创建时四个 reader 上限另有明确 Err，不等于隐瞒池耗尽 |
| node_tasks 过期结果 / 2.23–2.25 | 防止旧结果覆盖新文档是有意设计，既有 stale_worker_results_and_save_completion_order_are_safe 与真实异步保存回归通过；新增产物日志补足独立磁盘副作用的可追踪性 |

## 20 · 视频预取失败的计数与原因

状态：已完成；motion-host 全套 47 passed，0 failed，0 ignored。

- 视频预取解码失败或缓存拒绝时保留 source id、失败 PTS、累计 prefetch_failures 和有界的 last_prefetch_error
- 请求状态暴露失败计数、当前阻塞 PTS 和最近原因；聚合 metrics 增加 prefetchFailures，供现有 session 快照读取
- 复用进程级 FailureCounter：首次 debug，连续第 60 次仅一次 Error，成功的预取复位；维持失败后暂停该轮预取的策略
- 新增可控解码器的真实后台线程测试：当前必需帧成功而未来帧失败，状态仍 ready 且计数/原因可查；新 sequence 后解除故障可恢复未来帧，累计证据保留
- 不把预取失败伪装成当前帧解码失败，不改变缓存大小和播放器交互
- 关联辅助清单：10.37 的视频两处；日志：target/silent-failure-validation/video-prefetch-batch20.log

## 21 · 引擎启动与退出阶段的定位

状态：已完成；桌面全套 339 passed，0 failed，0 ignored；cargo check/build 通过。

- 启动时在工程锁、session 创建、内存配置之前记录阶段并 flush；ready 记录工程根、session 和总耗时，启动前的挂起也有最后阶段证据
- 明确记录锁、创建、配置与线程 spawn 失败；既有 60 秒启动等待区分 Timeout 和 Disconnected，超时说明后台初始化仍可能持有工程锁
- ready 接收端已关闭时记录正在释放 session；记录正常 Shutdown 与资源释放完成，join panic 保留有界 payload
- u64 内存输入先检查有符号预算上限，拒绝回绕；新回归验证拒绝时未创建/锁住工程，正常预算仍可启动和重开
- 保持原有等待、线程所有权和退出策略，不添加命令超时、强制取消、跨线程释放或 worker 重启；真正卡住的原生初始化仍需外部诊断，日志只提高可定位性
- 关联辅助清单：10.1–10.5；全集 engine total_memory as i64 候选；日志：desktop-startup-full.log、final-check.log、final-build.log

## 22 · 本轮集成验收与下一处修改边界

状态：以 01–21 为本节基线完成集成验收；后续批次另记，原始两份清单尚未全部关闭。

- cargo check -p motion-desktop --offline 与构建通过；桌面全套 339 passed，0 failed，0 ignored；host 全套 47 passed，0 failed，0 ignored
- 最新二进制完整运行 tools/ci_desktop.py：中英文、浮窗、工作区切换、节点图 GPU/项目绑定、共享 MCP 与样式覆盖全部通过
- 已查看最新中英文主窗口截图，合成正常绘制；未把编译通过或二进制自述单独作为视觉验收
- 最新二进制再次实测默认 Info：123 条普通命令 start、0 条 Present start，debug 健康 Present 无逐帧 done；轮转、Error 文件/stderr 双输出、MCP/命令目录 stdout JSON 纯净均通过
- 原生验收 stderr 未发生 panic；本机 Vulkan loader 报 Epic Overlay 两个缺失 manifest 路径，现由文件和 stderr 显式暴露，原生渲染验证仍通过；未修改机器的驱动或注册表
- 原生证据：target/silent-failure-validation/native-final.log、native-final/；协议及日志证据：runtime/runtime-summary.json
- 相对 origin/main，panels 差异仍只有用户已批准的 mask_inspector 测试初始化一行；生产 panel 文件保持未修改，既有断言不变
- 两份原始审计文档保持原样；未覆盖项继续待审，机械匹配和未标记行数均不当成缺陷完成数

进一步核实的误报：

| 辅助编号 | 源码结论 |
| --- | --- |
| 2.7 quick-ease 半次提交 | edits 是局部 Vec，任何一次校验失败都 return，只有整个循环结束才 push EditBatch；中途 return 会丢弃整批，不能把局部 push 说成已应用 incoming |
| 4.1、8.1–8.4 曲线图零尺寸 | animation_controls 的 width 在入口是 (area.width()-32).max(1)，graph 高度固定 130；报告忽略了入口约束，当前该除法并非零宽/零高路径 |
| 8.7 拖拽期间复用 bounds | 固定手势开始的坐标范围可避免控制点拖动时标尺随数据变化，引发反馈跳动；不是独立静默失败 |

下一处修改的授权记录（后续实施见 23）：

- path_controls/mask_path.rs：保留现有 Option 返回，拒绝时记录 object、mask 和真实错误
- path_controls.rs：仅在确实生成命令后推进 last_geometry，避免未投递的几何被标为已投递
- 具体待审补丁：target/silent-failure-validation/panel-observability-proposal.patch
- 此前获准例外仅为一行测试夹具；本次另行给出具体补丁后，用户明确批准这两个生产文件及对应回归测试，不扩展至其他 panels 生产文件

## 23 · 获准修复的蒙版编辑拒绝与拖拽推进

状态：已修复；用户已明确批准两个生产文件及对应测试。蒙版路径专项 8 项通过；加入后续异步导出观测后的当前工作区全套 342 passed，0 failed，0 ignored。

- mask_path::command 保持 Option 返回契约，拒绝时保留 object、mask 和原因；首次 warn、第 60 次升级 Error 一次，成功复位，避免持续拒绝时逐帧刷盘
- path_controls 只在确实生成编辑命令后更新 last_geometry；未生成命令的几何不会被标为已投递，目标恢复后同一指针位置可重试
- 新增日志子进程回归，验证 70 次拒绝只告警一次、升级一次，成功后重新告警，且不污染 stdout
- 新增真实 draw 输入回归：拖拽中目标暂时缺失时无编辑且几何未推进；目标恢复后相同 held 输入生成唯一命令并可应用到模型
- 旧代码负对照实际失败：无命令时 last_geometry 错误变成 [30,20,0,0,0,0]；恢复修复代码后同一断言通过，未修改任何旧断言
- check/build 通过；生产拖拽修复的完整原生双语/浮窗/节点/共享 MCP/样式验收通过，最新日志限频另由实际子进程回归覆盖
- 已查看 Path 与 Properties/Masks 原生截图；无布局、样式、控件、命中范围、图层数据结构或快捷键路由改动
- 关联辅助清单：2.1、2.21；证据：panel-mask-batch23.log、panel-mask-negative-control.log、desktop-panel-full.log、desktop-mask-throttle-full.log、panel-check.log、panel-build.log、native-panel.log 与 native-panel/
- 相对 main 的 panels 改动仅为本次获准的两个生产文件、mask_path_tests 对应回归，以及先前已批准的 mask_inspector 一行测试夹具

DESIGN §0.7 基线逐项处理范围（本次不是视觉整改）：

| 基线项 | 本次状态与理由 |
| --- | --- |
| path_controls 六处纵向步进 | retained with reason：未改相关绘制块，保留独立视觉整改追踪 |
| path_controls 两列宽度 | retained with reason：未改列计算；当前 Path/Masks 截图作为本轮无视觉回归证据 |
| path_controls 按钮/字段高度 | retained with reason：不改尺寸与命中区 |
| path_controls 自绘数值字段 | retained with reason：不更换控件 |
| path_controls 全宽布尔控件 | retained with reason：不重排属性行 |
| effect_controls 行高/步进 | retained with reason：不在本次获准文件范围 |
| effect_controls 布尔文本按钮 | retained with reason：不在本次获准文件范围 |
| effect_controls 全宽枚举 | retained with reason：不在本次获准文件范围 |
| effect_controls hover alpha | retained with reason：不在本次获准文件范围 |
| effect_controls 层级缩进 | retained with reason：不在本次获准文件范围 |
| timeline 11 列坐标 | retained with reason：保留兼容基线 |
| timeline snap guide 颜色 | retained with reason：遵循规范保留当前行为，不新增 token |

以上仅登记本批未改动视觉基线的范围，不宣称这些历史整改项已全部修复或现有行号仍准确。

## 24 · 脱离串行 worker 的电影冻结与导出日志

状态：已完成观测补漏；当前桌面全套 342 passed，0 failed，0 ignored；check/build 通过。

- 电影冻结线程继承原 request/operation/started 上下文，在真正冻结前写入并 flush 默认 Info async start；异步失败继续通过原 respond 记录编号、操作与总耗时
- 保留 respond 中 let _ = reply.send(value) 原样；结果无人接收时仍丢弃，后台操作本身的失败证据不再随 reply 消失
- 后台 panic hook 可读取同一 request 上下文，随后返回的错误仍保留原 panic payload；done 使用 debug
- MovieExport 在编码前记录 revision 和输出路径，记录实际成功产物/字节数、失败原因、耗时与 spawn 失败；正常取消记录 info，不伪装为故障
- 新增独立进程、真实后台线程回归：可控阻塞期间文件已有 start 且没有完成错误，释放后 IO 错误正确关联 #21；另一次真实 panic 的 hook 和错误都关联 #22，并验证 file/stderr 与 stdout 隔离
- 不改变 Engine::in_flight 的含义，不把独立后台任务冒充为串行 worker 当前命令；不增加超时、线程重启或同步等待
- 关联辅助清单：10.6–10.10；证据：async-movie-batch24.log、desktop-async-movie-full.log、desktop-mask-throttle-full.log、async-movie-check.log、async-movie-build.log
- 本节验证了生产异步调度/错误传递 helper；未据此宣称运行了 feature=ffmpeg 的真实编码导出验收

## 25 · 内存探测回退的默认可见原因

状态：已完成观测修复；桌面全套 344 passed，0 failed，0 ignored；check/build 与最新进程日志/协议验收通过。

- /proc/meminfo 读取或解析失败时，warn 明确记录平台、原始原因和实际使用的 fallback_bytes，不再把固定 8 GiB 当作无标记的实测值
- 解析验证 MemTotal、单位、正数、乘法溢出与有符号预算范围；异常输入不 panic、不回绕
- 两项新回归覆盖正常总量、缺失/错误字段、单位错误、零值、负数、溢出与符号边界
- 实际 Windows 启动确认默认日志有 platform=windows 与 fallback_bytes=8589934592，Warn 没有泄漏到 stderr/stdout；Error 仍同时落文件和 stderr，MCP/命令目录 stdout 保持 JSON
- 保留原有回退预算策略，不改变正常 UI 与预览策略；本节未新增 Windows/macOS 原生物理内存探测，不把回退值称为真实容量
- 关联辅助清单尾部未编号 total_memory 项；全集中该项原标 no-loss，但回退不可观测的问题已核实
- 证据：desktop-memory-probe-full.log、memory-probe-check.log、memory-probe-build.log、memory-probe-runtime.log、runtime/runtime-summary.json

## 26 · 工程包导入暂存目录清理的拒绝与失败原因

状态：已修复；motion-core 全套 211 passed，0 failed，0 ignored，包含原手动测量项；暂存清理/工程保存/包事务专项共 7 项通过。

- StageCleanup 对路径解析失败、越过已验证 base/命名边界的清理拒绝、以及 remove_dir_all 的 IO 失败记录路径和原因
- 已被发布 rename 或已清理的 NotFound 保持正常退出，不把成功清理伪装为错误
- 保留原有 canonicalize、base containment、非 base 本身与 importer-owned 名称检查，不在路径未验证时递归删除
- 新增真实文件系统回归验证：不可解析的路径给出原因、边界外同名前缀目录保留并告警、正常认领目录可删除、重复清理不误报；测试捕获日志只匹配独有夹具名称，避免并行测试互相干扰
- 首次不限制并发编译全 core 测试触发本机 Windows 页面文件不足；随后使用 -j 2 完整重跑通过，没有改测试断言或系统页面文件配置
- 关联辅助清单：11.8；证据：staging-cleanup-batch26-serial.log、core-batch26-full.log；保留首次资源不足日志 staging-cleanup-batch26.log

## 27 · 节点属性检查器的编辑拒绝日志

状态：已完成；当前桌面全套 345 passed，0 failed，0 ignored；check/build 通过。

- 当前 inspector 实际有 10 个丢弃 set_operation / set_parameter_default 结果的调用，统一记录 revision 与完整 Diagnostic，不再默默吞掉拒绝
- 只在用户操作确实被拒绝时 warn；成功绘制与 Present 路径不新增日志，不改变文档、历史、布局、控件或既有 notice 结构
- 新回归在子进程中实际绘制并点击 Bool 检查框，使用可编辑但有两个 Color Output 的图触发 MultipleOutputs，验证节点/版本/历史均未变化，日志包含 node toggle 与真实原因，stdout 未受污染
- 没有为符合报告而把这类尚不能编译的图禁止加载或禁止修复；保留既有图编辑策略
- 本批只修改 node_editor/view，不扩大到未经本次授权的其他 panels 生产文件
- 关联辅助清单：2.2；证据：node-inspector-batch27.log、desktop-inspector-full.log、inspector-check.log、inspector-build.log
