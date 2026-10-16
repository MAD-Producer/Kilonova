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

- 当前 renderer 的异步 demand 会在 busy 结束后启动非 speculative decode；其错误进入 Err。现有 image_preview_cache 已包含“坏预取资产转为 active 后必须返回 image 9 错误、不能永久 pending”的真 GPU 回归。原“只因没有检查 failed 集合就一定永久 false”推断与当前代码不符，留待本轮 GPU 实测确认
