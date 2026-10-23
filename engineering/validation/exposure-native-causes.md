# 曝光帧失败的原生原因与重放（本地验收完成，远端组合待验收）

继续 #138、#135，复用本人空闲工作目录，在 `fix/exposure-frame-error-causes` 基于 #437 head `393fe44e` 工作。开工前只审查分支路径元数据：exposure_frame 及其测试无其他未提交或近期非祖先改动；nested_exposure 有其他资源分支提交，宿主 session/sampling 有未提交改动，因此本批明确不编辑这两处，也不审阅别人的 PR。无 UI、依赖或资产变更。

| ID | 分类与改变 | 证据与限制 |
| --- | --- | --- |
| E1 | 原生传播：五个 ExposureScenes→ExposureFrame Model 边界保留 Model 原因，沿既有 Diagnostic 保持原验证前缀 | 构造时中心采样拒绝，以及接收者采样拒绝，实际 Model::Composition 的 code/message 和类型可匹配；不声称恢复 ExposureScenes 更早已转换掉的 provider 原因 |
| E2 | 原生缓存：每个失败帧保留一个 Arc<RenderError>，首次和重放共享同一原对象，销毁任务释放它 | 真实缺失文件产生 NotFound 和 OS code，60 次重放保持原 IO 对象地址，准备与 evaluator 次数不增长；Model 重放同样检查对象和 Arc 身份 |
| E3 | 行为保持：失败后仍终止、不重试，仍释放局部图像，目标由任务销毁释放；资源拒绝仍走原 getter | 原有真实上传拒绝、嵌套/整帧额度及持有者断言不修改；不改时间片、预算、调度、清理顺序、返回 Pending 的正常路径 |
| E4 | 边界保持：首次完整诊断、重放的 2048 字符上下文上限保持 | 原有 16384 字符失败对照及 <=2080 字符断言保持。缓存数量为每个失败任务一个原生原因、以任务销毁为释放点；原生对象自身内容不为截断文字而篡改，不把显示上限宣称为任意第三方错误对象的内存字节上限 |
| E5 | 既有默认信道：真实文件错误与同任务重放经 Host/respond/mpsc 回传，默认 Info 文件、Error stderr 和 stdout 隔离 | 受控 Flight 只验证回复边界，不冒充完整 worker 调度或阻塞前状态发布；随后显式创建新任务成功 readback，不重启失败任务 |
| E6 | 保留边界与待完成范围 | 渲染 Shared 枚举仍是既有 Arc 包装；验证沿公开枚举访问原 IO/Model，而非声称 source 自动展平所有透明/Arc 包装。宿主曝光缓存、嵌套构造和其他 IO/readback 拍平边界仍待处理，两个合集不关闭 |

新增两项回归先在旧产品执行，实际 0 通过、2 失败：旧中心构造没有 Model 类型，旧终止轮询没有 IO 类型。日志 `exposure-native-before-tests.log` SHA-256 `1d07bf8e5a7e232dc7705e8106369bf2f3bd5f3775f2179f68375b4ba15f10a8`。未改已有断言以消除失败。

第一次修复后，原八项曝光帧回归和新增两项均通过，原日志 `exposure-native-first-tests.log` SHA-256 `918074aa86c06c092c3acab91d43cb43bb1609ac00fc62adf11532d4e8b6e47a`。新增测试之后加强真实 OS code 与短诊断重放一致性，并增加桌面默认信道用例；最终验收必须使用后续冻结输入，首轮十项不与最终结果重复计数。

最终六阶段运行使用既有共享缓存，不新增构建目录。曝光调度/累积/额度/嵌套 GPU 37 项、桌面默认信道 6 项、相关渲染 11 项、完整宿主 120 项，合计 174 项通过、0 失败、0 忽略；全工作区全目标检查及 FFmpeg 产品构建成功。健康输出以既有独立 readback oracle 验证；本批没有修改像素算法，也不冒充重跑全部基线。

冻结后的曝光五套回归 37 项实际通过；随后新增桌面测试因 E0106 缺少返回借用的显式生命周期而未编译，原日志 SHA-256 `84655be75964f03d5f413f71570e0d4a342ac8ac43ca198641f3b03eb8baa8d9` 保留。进程终止后，只修正该新测试 helper 的生命周期注解；生产源码、渲染测试、Cargo/WGSL/YAML 逐字节不变，唯一差异记录为 `exposure-native-desktop-lifetime-only-delta.json`。最终后五阶段使用 `exposure-native-errors-v2-source-receipt.json`，首项 37 不重跑、不重复计数。

独立默认信道留样使用同一最终测试程序：准备回调产生一次真实 IO 失败，同一任务首报与重放各产生一条文件 Error 和 stderr Error；总计两个回复、两个文件记录、两个 stderr 记录，stdout 无故障文本。日志 SHA-256 `e6220a729863ad8eecddba6ba9df7617284df1b7f6a7acf9ad7522ac4949ef87`，stderr SHA-256 `cc8ac80dd84460ed8a31eecc946a0ba758f45d1503c31a99498be96e6b6f3c36`。这不是两个独立根因，也不另计两项测试。

实际 FFmpeg Debug 产品 SHA-256 `2c593db3e2c1926818ee698aaef31f840ee44d39c07aa46667cce9c7ed52949a`；运行 `--command-list` 退出 0，stdout 为 24475 字节有效 JSON，stderr 为空，运行前后程序和冻结源码不变。此项只证明新程序实际启动与命令列表协议，不代表 GUI 或媒体解码验收。

本地验收以 `393fe44e` 为父提交。#437 已实际合入，但主线同时包含其他资源改动，实际合入树与其原 PR CI 树不同。创建本 PR 前将再次核对六个文件没有主线交叠；本地 174 项不能冒充新主线组合的验证，远端 PR CI 与实际合入组合仍须独立验收。完整阶段结果、原始日志哈希、源码冻结及启动收据保留在 `artifacts/pr-ci-followups/exposure-native-errors*-results.json`、`exposure-native-default-output/execution.json`、`exposure-native-final-product/execution.json`。
