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
- 当时将预览编译 start/done 设为 debug；第 33 批已把真正执行的 Compile start 调整为默认 Info 并 flush，done、接收端关闭和正常退出仍为 debug
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

## 28 · 合成包 ABI 与部分数值转换的保留结论

状态：已核实下表范围；motion-render 的 compositions 5 项真实 GPU/序列化集成回归通过，未修改生产编码。

| 全集位置 | 源码边界与结论 |
| --- | --- |
| composition_plan.rs parent as u32 | root 的 parent 以 usize::MAX 初始化，序列化为 u32::MAX 是无父节点哨兵；普通 parent 为后序节点索引，活跃节点上限 64。直接改 checked conversion 会错误拒绝根节点 |
| composition_plan.rs slot / order casts | slot 定义为 i32，按 -(layer.order+1) 编码，再以 u32 保存位型；有效模型每个合成 MAX_LAYERS=128，不是把越界序号默默降级 |
| composition_plan.rs 偏移/长度/数量 | 写入前检查总包 MAX_BYTES=32 MiB，节点上限 64，节点/视频记录固定 80/40 字节；对应 header、start、size、videos.len 和 root 的数值均在 u32 范围内 |
| effect_plan.rs input/source/resource_input、compositing matte as u32 | 有符号纹理/遮罩 token 作为 32 位记录位型保存；effect_gpu 的 view(id:i32) 和 resource_input 分支明确区分负资产 token 与非负暂存/绘制槽位，不能把负数当成非法转换统一拒绝 |
| effect_plan.rs scratch_budget as u32 | 正规 PlanBuilder 设置范围受 MAX_SCRATCH_BUDGET=384 MiB 限制；本条仅说明受支持构建路径，不为任意外部伪造 plan 背书 |
| layer_gpu.rs mesh.vertices.len as u32 | 正规 tessellation 的 BoundedGeometry 在增长前执行共享顶点预算，MAX_TRIANGLE_VERTICES=262144；实际 draw count 的收窄不丢失这些合法值 |
| compositing_gpu.rs 等 usize/u32 as u64 | 当前受支持桌面目标为 64 位，列出的索引、字节数到 u64 属于等宽/拓宽；未标 no-loss 不等于真实失败 |

- 已有测试实际验证嵌套像素、后序父索引与程序绑定、随机 seek、重复 scene/plan 一致性，以及活跃实例超限拒绝
- 证据：target/silent-failure-validation/composition-abi-review.log
- effect_plan 的其他浮点尺寸、长期缓存 program 索引等候选未被本表一并关闭；不以单个 GPU 回归通过替代所有 104 行的逐项核实

## 29 · 音频后台错误的文件与控制台输出

状态：已完成；桌面全套 346 passed，0 failed，0 ignored；check 通过。

- 实时设备回调继续只发布首个错误与 stop，不增加文件或 stderr IO
- watchdog 退出时在后台线程、释放 error Mutex 后记录一次 Error，让设备错误即使未被 UI 轮询也能落文件与 stderr；正常停止不记录假故障
- 记录 mixer/watchdog 的 spawn 失败与已结束 worker 的 join panic，保留有界 panic 原因
- 新增独立进程回归：回调侧发布故障时无错误日志，启动真实 watchdog 后首因仅输出一次；次级取消不覆盖首因，正常停止无 Error，stdout 保持干净
- 保留音频 stop 的非阻塞策略：磁盘 IO 未完成时不在 UI 线程 join
- 辅助 1.7 的“永久杀死音频输出”表述不准确：latch 属于单个 AudioOutput，Shell::stop 会 take/drop，下一次播放创建新的输出；不清除当前坏流的保护闩锁
- 关联辅助清单：1.7、10.11–10.14；证据：desktop-audio-failure-full.log、audio-failure-check.log；随后日志过滤批次的完整构建覆盖本节生产源码

## 30 · Debug 依赖日志刷帧的实测修复

状态：已完成；桌面全套 347 passed，0 failed，0 ignored；check/build、最新二进制日志/MCP/GUI 冒烟通过。

- 真实验收发现即使已过滤 Motion Present start/done，wgpu 的 Debug 描述符仍把同一冒烟路径日志写到 80,425,613 字节，掩盖应用诊断
- MOTION_LOG=debug 现在保留 motion_* 模块的 Debug，第三方保留 Info/Warn/Error；MOTION_LOG=trace 显式保留完整依赖细节
- 默认 Info、Error 文件/stderr 双输出、原 panic try_lock 策略、用户显式 MOTION_TRACE_UI 均保持
- 新回归验证 Motion dispatch Debug 保留、依赖逐帧 Debug 在 debug 模式过滤、adapter Info/Warn 保留、trace 模式仍可见依赖细节
- 最新实际同一路径 Debug 日志为 82,891 文件字节（读为文本后 81,954 UTF-8 字节，差异为换行归一化），此为本机两次冒烟观测，不是跨机器性能承诺
- 实际验证 Motion attach dispatch Debug 存在，wgpu/naga Debug 不存在；默认 123 条普通 start、0 条 Present start，健康 Present 无逐帧 done；Error 双输出、内存回退标记、轮转及 stdout JSON 纯净均通过
- cargo tree 未发现会在 release 静态屏蔽 Info 的 log max_level/release_max_level feature；未据此宣称已运行 release 二进制
- 证据：desktop-debug-filter-full.log、debug-filter-check.log、debug-filter-build.log、debug-filter-runtime.log、runtime/runtime-summary.json、log-features.log

当前观测补齐及核实分批记录到 44；后续实施范围限定为可观测性，功能与算法问题单独登记。原始两份清单保留原样，未覆盖条目仍待审，不将 832 个候选当作已确认或已关闭缺陷。

## 31 · 控制台反压时优先保全错误文件

状态：已完成；桌面全套 348 passed，0 failed，0 ignored；check/build 与最新 CLI/MCP 实测通过。

- 普通 Error 先写文件并 flush，释放文件 Mutex 后再写 stderr；stdout 仍完全不参与日志
- 新增独立进程回归，故意不读取子进程 stderr 并输出足以填满管道的 Error；控制台可能阻塞之前，完整错误尾标记已能从日志文件读取
- 测试只终止并回收自己创建的阻塞子进程，不对产品 worker 增加超时、取消或重启；产品 stderr 仍是同步输出，本节不宣称解决任意阻塞终端
- 原 panic hook 仍只 try_lock，原 stderr hook 与锁中毒恢复回归均通过
- 最新二进制实际验证 CLI 命令目录 JSON、MCP 三条 JSON-RPC 响应、Error 文件/stderr 双输出及日志轮转通过
- 证据：desktop-console-order-full.log、console-order-check.log、console-order-build.log、console-order-protocol.log


## 32 · 媒体后台任务开始与返回 Ok 状态中的错误

状态：已完成；motion-media 全套 64 passed、0 failed、2 项原有大媒体验收 ignored；桌面全套 348 passed、0 failed、0 ignored，check/build 通过

- 为音频导入/探测、音频缓存、视频导入/探测、视频缓存和工程包导出记录 request、operation 与耗时
- 在后台线程进入 provider/open/执行函数之前写入并 flush 默认 Info start，即使随后阻塞也保留开始证据
- 对存入 status.error、但由状态查询以 Ok 返回的失败记录 Error；由桌面 sink 同时写文件与 stderr，库本身不直接操作控制台
- 在更新状态并释放状态 Mutex 后记录完成/错误，保留现有状态结构、发布事务和取消语义，正常完成与取消只记 Debug
- 新增真实后台线程回归：分别阻塞音频/视频 provider，验证默认 Info start 及同线程 flush 发生在释放之前；释放后验证失败状态和 request/operation/ms 日志；真实工程包导出到被普通文件阻挡的目录，验证失败证据
- 保留两项原有大型媒体忽略条件：release 的大音频/工程包验收和需要 MOTION_LARGE_VIDEO_SOURCE 的大视频验收，未宣称本轮执行了它们
- 证据：media-task-log-check.log、media-task-log-full.log、desktop-media-observability-full.log、media-observability-check.log、media-observability-build.log


## 33 · 节点编译开始也必须默认可见

状态：已完成；桌面全套 348 passed、0 failed、0 ignored，check/build 通过

- 将真正进入执行的 Compile start 与保存/导出 start 统一为 Info 并立即 flush，避免默认配置遗漏编译挂起的开始证据
- 把 start 放在过期/取消检查之后，未执行的淘汰任务不制造开始噪声，完成继续使用 Debug
- 加强既有独立进程回归：真实提交第三个 Compile 任务，断言默认日志存在 INFO node start #3 Compile；既有保存失败、成功产物与控制台隔离断言全部保留
- 不更改执行顺序、结果接收守卫或 Present 路径
- 证据：desktop-media-observability-full.log、media-observability-check.log、media-observability-build.log


## 34 · 电影源时间戳与导出路径解析失败

状态：已完成修复；默认功能的 motion-desktop-media 2 passed，带 ffmpeg 功能全套 16 passed、0 failed、0 ignored

- 将冻结前/后的 modified() 查询错误向上传播，携带资产相对路径和 capture/before-copy/after-copy 阶段，不再把两个失败都塌缩为 None 后比较相等
- 将导出目标解析集中到 export_destination，仅允许 NotFound 代表尚不存在的新输出，保留其他 canonicalize 错误及目标路径
- 保留源路径保护和临时 MP4 原子发布，不改编码、GPU 线程或取消策略；本节不把旧的路径回退直接认定为已经发生了源文件覆盖
- 新增无需 ffmpeg 的真实文件系统回归：成功冻结后保留原字节、同字节长度但修改时间改变时拒绝冻结、已存在源文件不可作为输出、新输出允许但不创建文件、无效 NUL 目标必须报告解析错误
- 加强原有 AAC 回读测试，只将 EOF/EAGAIN 作为正常停止，其他解码错误直接带原因失败，全部既有音频断言保留
- 带 ffmpeg 验收实际执行 MP4 编码、AAC 回读、取消保留既有输出与临时文件清理，以及嵌套视频/效果/表达式/PCM 的冻结导出，不再仅用默认功能编译代替可选功能验证
- 时间戳 API 错误传播已由代码路径检查确认；本机真实文件系统支持 modified()，未宣称复现了“不支持修改时间查询”的文件系统
- 关联全集 B8：movie.rs 原 105、550、579、697；证据：movie-integrity-full.log、ffmpeg-availability-check.log、desktop-media-ffmpeg-final.log


## 35 · 视频旋转与不可用音轨的原始原因

状态：已完成；motion-desktop-media 的 ffmpeg 全套 16 passed、0 failed、0 ignored，包含 4 项本轮新增回归；本机离线编译使用仓库已有 FFmpeg/libclang 开发工具

- 无显示矩阵时，rotate 标签解析错误保留原值和原因，不再默认为 0；缺省标签仍表示 0 度，合法显示矩阵仍优先，不因冗余坏标签拒绝可正确显示的视频
- 在有限性验证后先把角度归一化到一圈，再转为四分之一圈编号，避免有限但过大角度先收窄到整数时饱和
- 为每条音轨保留 error 原因字段；无法选择音轨时的错误包含轨道编号与解码器原因，显式 with_audio:false 仍可放弃音频
- 保留 rate/channels=0 作为原有不可用能力标记，supported=false 与 error 同时给出，不把它当作实测音轨格式，也不改变任务状态结构
- 将解码时间戳的索引一致性检查提前到保存 last_decoded 之前，并使用 checked conversion；合法模型本来已限制一小时以内，本节不夸大为合法视频必然发生的整数回绕
- 新增真实 FFmpeg 流回归：坏 rotate/NaN/inf/非直角/空值拒绝，负角与整圈正确归一化，rotated-90 夹具的有效矩阵覆盖坏标签；真实音频流换成不可用解码器，验证 metadata 和选择错误都保留原因
- 核实 B8 av.rs 原 398：Decoder::open 验证首个 PTS 等于 video_start_us、索引严格递增；frame 拒绝 target 小于 video_start_us，partition_point 的下溢回退在合法索引上不可达，不机械改动
- 核实 B8 shader.rs 原 148：饱和乘积立即与 65536 比较，溢出必拒绝，保留保守预算检查
- 保留取消锁中毒恢复的停止语义；check 与最终发布仍拒绝中毒，原始 panic 由既有 hook 记录，不把恢复停止标志视为成功恢复导出
- 关联全集 B8：av.rs 原 58、83、100、379、398；证据：desktop-media-ffmpeg-final.log、ffmpeg-availability-check.log


## 进度统计快照（截至第 35 批）

本节是保守的已登记关联覆盖率，**不是缺陷修复率，也不是全部清单完成率**。统计修复项与核实保留项，去重使用辅助清单编号，不用提交数或测试数充当工作百分比

| 口径 | 总数 | 当前能明确关联完成记录 | 进度 |
| --- | ---: | ---: | --- |
| silent-failure-paths.md 的编号条目 | 285（编号去重后仍为 285） | 102 | 至少 35.8%，对外约 36% |
| 全集未标 no-loss/test-only/guarded 的候选 | 832 | 尚未建立逐条状态映射 | 暂不报完成率 |
| 全集机械枚举 | 4443 | 含无害/测试/保守预算等候选 | 不直接作为缺陷分母 |

102 项取自现有明确“关联辅助清单/关联辅助”记录、已核实保留表及明确复核的 0.3/2.7/2.22/4.1/10.24/10.26/10.31；同一编号多批涉及只计一次。尚未映射的 183 项不自动认定未修复，也不自动认定是缺陷；后续必须逐条对照源码和证据。部分条目保留线程/丢弃回复等既有设计而补了观测，不能把其原报告的全部后果都宣称修复

以下按首次明确对应记录列出去重编号，便于复核保守下界：

| 对应完成记录 | 辅助清单编号 |
| --- | --- |
| 01 · worker 响应断开与启动快照 | 0.6、1.1–1.4、2.31、10.5 |
| 已核实应保留的行为 | 0.1 |
| 02 · 异步任务保留 panic 原因 | 3.2–3.12、10.7、10.10、10.12、10.20、10.25、10.27、10.30、10.32、10.36 |
| 04 · 控件失败边界与文本裁切 | 6.4、6.7、9.17 |
| 06 · host 导出与音频状态的真实失败 | 0.8、4.4、4.15 |
| 07 · 插件包发布与卸载事务 | 11.23–11.26 |
| 08 · 媒体缓存成对发布与关闭清理 | 1.12、11.10–11.20 |
| 09 · 图片代理缓存的失败与临时文件所有权 | 1.10、11.27–11.28 |
| 10 · 节点交互取消与能力查询请求 | 5.20 |
| 新核实的保留项与误报 | 0.5、1.5–1.6、1.11、4.6、4.11–4.14 |
| 11 · 媒体导入与分析的终态处理 | 5.21 |
| 14 · 工程和节点源的临时文件清理 | 11.9、11.21–11.22 |
| 15 · 撤销历史预算淘汰计数 | 0.7 |
| 18 · 桌面导入、节点安装与恢复文件清理 | 11.1、11.3–11.5 |
| 19 · 节点后台任务与已写入文件的证据 | 2.23–2.25、5.5–5.6、10.19、10.21 |
| 20 · 视频预取失败的计数与原因 | 10.37 |
| 21 · 引擎启动与退出阶段的定位 | 10.1–10.4 |
| 23 · 获准修复的蒙版编辑拒绝与拖拽推进 | 2.1、2.21 |
| 24 · 脱离串行 worker 的电影冻结与导出日志 | 10.6、10.8–10.9 |
| 26 · 工程包导入暂存目录清理的拒绝与失败原因 | 11.8 |
| 27 · 节点属性检查器的编辑拒绝日志 | 2.2 |
| 29 · 音频后台错误的文件与控制台输出 | 1.7、10.11、10.13–10.14 |
| 08 | 0.3 |
| 02 | 10.24、10.26、10.31 |
| 04 | 2.22 |
| 本轮进一步核实的保留项 | 2.7、4.1 |

当前统计不把第 34/35 批的 B8 修复硬换算成辅助编号，也不因一项测试覆盖多个路径而重复累计。原始两份报告保持不变

## 第 32–35 批最终集成验证

- 最新桌面 cargo check/build 通过；cargo test -p motion-desktop --offline -j 2 -- --include-ignored 实际运行原生音频项，348 passed、0 failed、0 ignored
- 首轮标准 cargo test 为 347 passed、1 项原生音频 ignored；随后完整 include-ignored 重跑通过，保留两份输出，不将首轮当作零跳过验收
- motion-desktop-media 默认功能 2 passed；ffmpeg 功能全套 16 passed、0 failed、0 ignored，包含真实编解码与 GPU 导出；motion-media 的两项既有大媒体条件限制见第 32 批
- 第 32/33 批实际程序默认 Info 测得 124 条普通 start、0 条 Present start；Error 同时写文件和 stderr，MCP 三条响应及命令目录 stdout 均保持 JSON，GUI smoke 通过
- 最新 GUI/协议复验记录见 b8-runtime.log 与 runtime/runtime-summary.json；不把后续无关视觉整改纳入本轮完成范围
- 证据：b8-desktop-check.log、b8-desktop-full.log、b8-desktop-full-include-ignored.log、b8-desktop-build.log、desktop-media-ffmpeg-final.log、media-node-runtime.log

## 36 · 安装已提交后的刷新失败与端口解析原因

状态：已完成；桌面完整测试 351 passed、0 failed、0 ignored，cargo check 通过

- 效果包安装成功后立即记录并 flush 默认 Info，目录刷新派发/接收失败记录 Error，包含包路径、installed=true、catalogue 阶段及原始原因；安装失败保留 install 阶段
- 图层节点安装后的 state/catalogue 刷新失败分别记录 Error，携带 epoch/revision，不改变原有成功路径、错误提示结构或安装事务
- 更改节点操作时，邻接节点的端口解析错误记录 Warn，保留 edited_node、失败节点、revision 和原因；保留既有连线裁剪及 Undo 行为，不把无法解析的端口伪装成另一种类型
- 新增真实安装回归：安装独立测试包后关闭 worker，再刷新目录，验证安装证据先于刷新失败，重开工程仍能读取已安装包，Error 同时出现在文件与 stderr，stdout 无诊断文本
- 新增邻接参数定义缺失回归，验证日志保留具体参数原因、既有不兼容连线提示仍在，Undo 完整恢复原图
- 未修改 panels、布局、样式、控件或交互；本节未重新宣称 DESIGN.md 全量视觉验收通过
- 关联辅助清单：2.3、2.6、2.26；证据：b36-desktop-full.log、b36-desktop-check.log

## 37 · 取消任务前保留已经发生的原始失败

状态：已完成；桌面完整测试 351 passed、0 failed、0 ignored；取消负对照确实失败

- 将节点任务的错误观测放在完成结果被改为 Cancelled 之前，保留 request、operation、耗时、epoch/revision、可用目标路径、cancelled 标记及原始 IO/编译错误
- 编译预览失败不再先被内部取消检查抹掉；已经取消且编译成功时仍跳过后续规范编译，不增加过期工作
- 对外仍返回取消结果，generation、pending、preview stamp 的守卫继续丢弃过期结果，不把旧错误写入新文档
- 新增独立进程回归：实际向被普通文件阻挡的目录保存，实际编译缺少输出的图，在执行结束和完成发布的确定性边界取消，验证原始错误落盘、Error 写 stderr、stdout 不被污染；成功后取消不制造假故障
- 该测试控制完成交界，不宣称复现某个操作系统 IO 调用内部的竞争时序
- 负对照临时在错误观测前返回 Cancelled，保留同一断言，Cargo 实际退出 101；随后恢复原字节并重跑完整桌面测试通过
- 关联辅助清单：3.1；证据：b37-cancel-negative.log、b36-desktop-full.log、b39-desktop-full.log

## 38 · 有限 PCM 运算产生 NaN 的真实失败

状态：已完成；motion-media 全套 67 passed、0 failed、2 项原有大型媒体 ignored；桌面完整测试 351 passed、0 failed、0 ignored

- 两个模型合法、缓存样本也有限的音源，分别为正负 f32::MAX、音量均为 2，能够在原有 f32 累加中得到 NaN；原 mix 返回 Ok(4)，clamp 无法移除 NaN
- 最终限幅前拒绝 NaN 并返回绝对 sample、合成 frame、零起始 channel 和算术溢出/对消原因，由既有后台失败观测接收；不新增实时回调日志
- 保留正常增益、重采样顺序、普通削波和正负无穷饱和到正负 1 的既有合同，不把合法的超幅 PCM 一并拒绝
- 新增真实缓存回归验证 NaN 拒绝、正负削波、单源正负无穷饱和和正常有限增益；移除检查的负对照实际退出 101，同一断言收到原先的 Ok(4)
- 辅助 4.16 核实保留：不足一帧的源尾部需要补静音；新增 16 样本真实缓存测试验证原样本完整保留，尾部随机 seek 与整块一致，进一步越源的整帧仍被模型明确拒绝
- 辅助 12.16 为 cfg(test) 波形线程，接收端有 recv_timeout 和断言，不增加产品 worker 监控；10.39 在本节只核实其中 mixer 的测试线程部分
- 保留两项原有大型媒体忽略条件：release 大音频/工程包与需要外部大于 512 MiB MP4 的视频验收；未修改断言或忽略条件，未称本轮执行过它们
- 关联辅助清单：4.16、12.16；证据：agent-audio-model-final-full.log、agent-audio-model-negative.log、agent-audio-model.md、b39-desktop-full.log

## 39 · 主程序错误出口与后台服务结束

状态：已完成；桌面完整测试 351 passed、0 failed、0 ignored；带 ffmpeg 独立程序构建与真实进程验收通过

- 在日志初始化后的主程序统一错误出口记录 Error，包含运行耗时和原始原因；保留 Rust main 的退出码和既有 stderr 行为，stdout 不参与日志
- MCP UI 服务开始/正常返回分别记录默认 Info 并 flush；正常 EOF 不记为错误，不增加重启、超时或新的协议消息
- 退出时诊断 checkpoint 的派发失败、返回 Err、等待超时/断开记录 Warn，保留原有一秒等待上限和正常退出行为
- 实际以缺少参数的 --project 启动独立程序，验证非零退出、错误文件/stderr 双输出、stdout 为空；实际 --mcp-ui 配合隐藏窗口 smoke 和 EOF 验证服务开始/结束日志
- 实际把 MOTION_DIAGNOSTICS_PATH 的父目录替换为普通文件，GUI smoke 仍完成，退出日志保留 checkpoint 的文件系统错误；Warn 不额外写 stderr
- 实际默认 Info 有 124 条普通 start、0 条 Present start，CLI/MCP stdout JSON、错误双输出和日志轮转均通过；Debug 日志 82,088 字节，Motion Debug 保留而依赖逐帧 Debug 被过滤
- 使用独立二进制和 LOCALAPPDATA 目录验收，未覆盖用户正在运行的程序或日志；已查看实际主窗口截图，内容区正常绘制，保留 §0.7 既有视觉整改项，不宣称完成 §12 全部人工验收
- 关联辅助清单：10.18、10.22；证据：b39-desktop-full.log、b39-desktop-build-ffmpeg.log、b39-runtime.log、runtime-b39/runtime-summary.json

## 40 · 音频缓存错误保留资产、阶段和原始原因

状态：已完成；motion-media 全套 69 passed、0 failed、2 项原有大型媒体 ignored；桌面 352 passed、0 failed、0 ignored，cargo check 通过

- 缓存路径解析不再丢弃原始 IO 错误并统一报成 missing，记录资产 ID、缓存路径、IO kind 与 reason，继续提示 prepare_audio
- 缓存打开/metadata/seek/read 的错误带上对应阶段及资产；长度不符同时给出 expected_bytes/actual_bytes，越工程路径仍明确拒绝
- 非有限缓存样本带上源资产、绝对源 sample 和 channel，区别于第 38 批的混音运算产生 NaN
- 新增真实文件回归：构造成功后截断已打开的缓存，验证 UnexpectedEof；删除缓存后重新构造验证 NotFound 和具体路径；等长缓存内写入 NaN，验证源样本定位
- 不把权限错误伪装成 NotFound，不使用平台 ACL 桩声称实际复现了权限错误；open/metadata/seek 的上下文由代码路径核对，当前新增真实故障覆盖 read/路径解析/坏样本
- 保留既有 Err 调用链，由后台 watchdog/导出失败日志观测；不在实时回调或逐采样循环写文件/stderr，不改变成功 PCM、缓存预算或模型格式
- 证据：b40-media-full.log、b40-desktop-full.log、b40-desktop-check.log

## 41 · 诊断报告执行前状态与无人接收的写入失败

状态：已完成；桌面完整测试 352 passed、0 failed、0 ignored，cargo check 通过；负对照实际失败

- reporter 在 report()、序列化与文件 IO 之前记录默认 Info start 并立即 flush，携带线程内任务编号、checkpoint/export 操作及目标路径
- 写入失败保留 localWriteErrors，并记录 Error 的任务编号、操作、耗时、路径与原始原因；无 reply 的自动 checkpoint 同样留下证据，Error 沿既有日志层同时写文件和控制台 stderr
- 成功完成使用 Debug；自动 checkpoint 沿用既有五秒限频，record() 中不增加文件 IO 或逐帧日志
- 保留原有写入、上传、回复结果与处理顺序；完成错误日志位于 reply 发送后，避免控制台阻塞先吞掉已生成的回复
- 新增独立进程回归：真实持有报告状态锁阻塞快照，直接读取日志确认默认 Info start 已落盘；用普通文件阻挡目标父目录，验证显式/自动 checkpoint 两次错误均有文件和 stderr 记录、stdout 无日志、计数为 2，随后真实导出可读 JSON 且原阻挡文件字节不变
- 负对照只将 start 移到 report() 之后，保留同一断言，Cargo 实际退出 101；finally 恢复源码原字节后再执行完整验收
- 本节只补日志与回归，未修改 panels；DESIGN.md §0.7 既有视觉项继续保留，不把测试通过表述为 §12 全量人工视觉验收
- 证据：b41-reporter-test.log、b41-reporter-negative.log、b41-desktop-full.log、b41-desktop-check.log

## 42 · 诊断服务生命周期与退出发送未完成

状态：已完成；桌面完整测试 353 passed、0 failed、0 ignored，cargo check 通过

- 诊断服务 started、stopping、stopped 使用默认 Info 并 flush，停止原因区分 channel_closed 与 owner_dropped；正常关闭不记为 Error
- 观察 Sentry Client::flush 的返回值，false 时记录 Warn，包含既有 wait_limit_ms=500 与实际 elapsed_ms；保留原有等待上限、上传同意、上传预算与退出流程
- 新增独立进程回归，使用真实 SDK 配合受控 transport 返回 false，验证从开始、停止、未完成警告到结束的日志顺序，以及默认 Warn 仅写文件、stdout/stderr 不增加诊断文本、日志不含 DSN；不声称复现了真实网络超时
- 此批仍只增加观测，不新增重试、worker 重启、UI 数据结构或算法修复
- 关联辅助清单：10.16；证据：b42-desktop-full.log、b42-desktop-check.log

### 第 41–42 批可选功能验证边界

- 带 ffmpeg 的功能与集成测试实际 11 + 5 项通过；随后同一完整命令的文档测试报 E0463，无法找到 motion_core/host/media/render/effects 依赖，整条命令未通过
- 单独文档重试曾通过，不能据此覆盖完整命令的失败；原因尚未核定，不将共享目录里同时存在其他编译任务直接认定为根因
- 另一次完整重跑被磁盘空间不足中止；释放旧构建符号文件后重跑，仍为 16 项功能测试通过、文档阶段 E0463，未修改断言、忽略项或产品代码绕过
- 本轮保持可观测性范围，不把可选功能元数据排查扩展成产品逻辑修复；桌面验收与可选功能的结果分别登记
- 证据：b40-desktop-media-ffmpeg.log、b40-doc-retry.log、b41-desktop-media-ffmpeg.log、b41-desktop-media-ffmpeg-retry.log


## 43 · 视频解码线程的飞行状态与持续等待

状态：已完成；host 全套 49 passed、0 failed、0 ignored，含文档测试；桌面完整重跑 353 passed、0 failed、0 ignored，cargo check 通过

- 在原生解码器打开及 frame() 调用之前发布飞行状态，保留 object、source、sequence、PTS、open/decode 阶段、speculative 与 elapsed_ms，完成结果处理后清除
- 通过既有 video_info 的 status/metrics 快照追加 in_flight、pending_polls、decodeInFlight，聚合沿用最多四条 stream 的现有限制，不连接新 UI 或改变状态结构合同
- 每条 stream 连续返回 pending 时首次 Debug、第 60 次 Error 且只升级一次，成功观察或显式错误后清零；错误同时保留当前请求与实际活动请求，避免把过期帧误认为当前帧
- 60 次为连续查询次数，不能直接等同一秒或证明原生线程已死锁；日志包含实际解码耗时，换请求仍未就绪时不清空连续等待证据
- 默认 Info 记录并 flush worker 开始、首次 decoder open start 和停止；不逐帧记录 start，完成仅在耗时至少 100ms 时 Debug，不增加 Present 日志
- 停止后的返回值仍按既有设计丢弃，但真实解码错误和 panic 保留 Error 的 object/source/sequence/PTS/耗时/原因；预期 video target superseded 不制造故障日志，状态锁在文件/控制台输出前释放
- 新增独立进程测试，真实后台线程分别阻塞受控 open_decoder/frame，验证阻塞期间状态可读、默认 Info 开始证据已刷入文件、120 次 pending 每轮只报一次、成功后可再次升级及停止后原始失败保留；stdout 无诊断文本
- 负对照临时移除调用前的飞行状态写入，保持同一断言，实际退出 101 且断言收到 Null 而非 open；随后 finally 恢复源码原字节并跑全套，不以编译失败冒充回归守卫
- 测试使用受控平台适配器，不宣称实际复现了系统编解码器内部死锁；不新增生产超时、重启、跨线程 GPU 操作或改变取消/缓存策略，panels 未修改，既有视觉项保留
- 首轮默认测试并发为 352 passed、1 failed，失败在既有双击测试的真实时钟断言；不改源码或断言，以 --include-ignored --test-threads=2 完整重跑后 353 项全部通过，不据重跑直接宣称已证明首轮失败根因
- 关联辅助清单：10.35、10.36；证据：b43-video-observability.log、b43-video-flight-negative.log、b43-host-full.log、b43-desktop-full.log、b43-desktop-full-retry.log、b43-desktop-check.log


## 44 · 媒体任务取消时保留已经返回的真实失败

状态：已完成；motion-media 全套 71 passed、0 failed、2 项原有大型媒体 ignored；桌面完整 353 passed、0 failed、0 ignored，cargo check 通过

- 在音频导入/缓存、视频导入/缓存及工程包导出五处 cancel 分支丢弃结果前读取已返回的错误；释放任务状态锁后记录 Error，携带 request、operation、phase=cancelled、实际耗时与原始原因
- 被 catch_unwind 捕获的 panic 同样保留有界原因，沿用 2048 字符和非字符串 payload 回退；既有公开 panic_reason 接口及正常失败前缀保持不变
- 仅抑制该操作已有的确切取消原因字符串，成功后取消也不制造故障；不把所有错误统一替换成 cancelled，不修改终态、发布、清理或任务释放语义
- 工程包仍先关闭/删除准备中的临时归档，再发布 cancelled 终态，日志位于释放状态锁之后；实际 Error 输出沿用桌面文件与 stderr sink，不在媒体库直接写 stdout/stderr
- 新增独立进程回归：四类音频/视频后台操作在受控 provider/decoder/probe 中阻塞，取消后分别返回真实 Err 和 panic，默认 Info 下验证八次终端 Error 的原始原因、请求、操作和耗时，同时状态保持 cancelled 且 error 字段仍为空
- 工程包在日志 start 交界受控暂停，先取消，再实际向普通文件阻挡的父目录准备归档，验证原始 IO 原因保留、原文件字节不变且不产生输出；不宣称复现了 OS IO 调用内部的取消竞争或工程包原生 panic
- 对五类任务再走真实协作取消检查，使用 Debug 仅等待既有完成事件，验证正常取消均无 Error；这些噪声检查与前面的默认 Info 故障观测分开执行
- 负对照只把音频导入的取消日志恢复为原先 None，保留同一断言，实际退出 101 并报告 missing cancelled worker observation；finally 恢复原始字节后全套媒体测试通过
- 保留两项原有大型媒体忽略条件，不改变断言或假称实际执行；桌面 include-ignored 仍执行其全部原生音频项
- 不修改 panels、可视布局、播放行为或业务错误结构；辅助清单关于取消后卡在非终态的原始断言仍按第 02 批核实保留，本批关闭的是原始错误被取消丢弃的观测缺口
- 关联辅助清单：10.24、10.26、10.29、10.31、10.34；证据：b44-cancellation-tests.log、b44-cancel-negative.log、b44-media-full.log、b44-desktop-full.log、b44-desktop-check.log
