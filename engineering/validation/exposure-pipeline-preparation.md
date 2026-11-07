# 运动模糊预览的后台效果管线准备

继续 #64 / 草稿 #627。开始前核对开放 PR：#575 的预渲染、#582 的追踪和
#626 的深度后端由其他分支负责；本次只修改自己的效果管线准备。

## 功能边界

普通预览已有后台入口，运动模糊此前在该入口之前返回，仍可能在接收图层或
调整层首次渲染时同步创建效果管线。现在曝光任务的准备回调携带实际局部输出
尺寸；宿主在素材就绪后把实际采样 Scene 交给同一个 Session worker，未安装
对应结果时返回 pending，不推进接收图层或累积相位。

准备任务使用严格曝光上下文和保留图层规划器，包含 preflight 与实时 coverage
两个状态，随后按当前上下文中的非调整图层规划 materialization。它不预编译
整个效果库，仍受既有场景、句柄、WGSL 和快照上限约束。规划会遍历当前上下文
的图层；不能将其描述为只有当前一个接收图层的规划成本。

请求身份包含原有 Renderer/document/revision/view/surface/格式范围及实际合成、
采样帧的完整浮点位、实例图层 ID 序列和局部输出尺寸。曝光结果不能套用普通
预览的“同修订即可继续”策略；改变时间、子合成实例或尺寸后必须等待对应结果。
曝光采样由既有 ExposureScenes 执行，不在这个 worker 中把子合成时间当成根时间
重新采样。普通预览的近期窗口行为保持原样。

新增 `PollError::Preparation` 保留原始宿主准备错误；它不经过既有曝光错误的
字符串转换，也不让任务误记一次接收完成。准备 State 保留同一失败和票据，
后续轮询不重试；显式改变视图/工程请求后才可重新准备。没有新增 worker、
超时、重启或生产内存预算调整。

`foregroundBuilds` 读取 Renderer 已有的效果管线创建计数，按 Renderer 生命周期
累计，registry 替换不重置；不含后台创建。它不统计固定 presenter/compositor
管线，不能据此声称前台没有任何 GPU 工作。

## 已执行验证

- `exposure-preparation-check`：锁定依赖的宿主 all-targets 检查通过。
- `exposure-surface-native`：实际 Cargo 宿主原生测试七项通过，包含第一版真实
  Windows 窗口、四相位运动模糊与调整层、受控阻塞、旧图保留及同步像素对照。
- `exposure-nested-recovery`：七项通过、一项失败。扩展的夹具子合成 ID 没有
  使用仓库要求的 `comp-` 前缀，工程校验拒绝，尚未进入渲染；保留原始输出。
- `exposure-nested-corrected`：修正夹具 ID，保留全部原断言，八项通过、零失败、
  零忽略。真实窗口包含 8×8 子合成与 16×16 根输出；准备与同步曝光输出每个
  通道的差异均不超过既有显示量化容差 1。前台效果管线创建为零，独立同步
  对照实际创建了效果管线，避免把没有效果工作的空场景当作通过。
- 同一窗口用例故意使后台采样边界 panic，验证原始 Panicked 类型和原因穿过
  新回调；65 次失败轮询保持同一 Arc、同一 ticket、同一 workUnits 和完整旧图。
  新 view 请求恢复真实输出，工程与 revision 始终不变。这是错误类型/生命周期
  证据，不代替完整桌面 logger/panic hook 的默认输出资格。
- `exposure-all-targets`：包含 FFmpeg、diagnostics、ai-depth 的完整宿主/桌面
  all-targets 检查未通过。桌面测试程序的 rustc 实际报告
  `memory allocation of 2097152 bytes failed`，随后以 0xc0000409 退出。
  不是 Rust 类型检查错误，也不能计作编译成功；保留完整命令与原输出。
- `exposure-render-regression`：实际 Cargo 执行最终 render 库全部 325 项，
  `exposure_frame` 集成 10 项、`nested_exposure` 集成 9 项，合计 344 项通过，
  零失败、零忽略。它覆盖保留接收图、调整层、递归子合成、不同帧率及重复引用，
  不代替完整桌面测试和其默认日志输出资格。
- `exposure-all-targets-recheck`：系统提交内存回落后，以同一源码、原配置和
  完全相同命令复查，通过 FFmpeg、diagnostics、ai-depth 的完整桌面/宿主
  all-targets 检查，58.84 秒。未修改生产预算、断言、超时或系统页面文件，
  两轮原始记录分别保留。编译检查通过仍不等于完整桌面原生测试通过。
- `exposure-host-lib-regression`：实际 Cargo 执行完整宿主库 225 项，全部通过，
  零失败、零忽略，257.68 秒；包括此前分配失败的两个 registry 子进程用例。
  没有调整相关测试或实现，旧失败输出保持原字节。这是本轮库回归结果，不是
  完整宿主集成套件或首轮失败原因的额外推断。
- `exposure-desktop-default-log`：包含 FFmpeg 的完整桌面测试程序实际编译并
  执行原有准备日志验收，一项通过，1832 项过滤，零失败、零忽略；命令总计
  529.98 秒，测试 12.28 秒。本轮原样链接桌面生产实现及真实 engine context，
  不再使用此前独立入口的空闲 engine 替身。它不是完整桌面测试套件的运行。
- `exposure-desktop-sink-record`：再执行同一实际桌面原生 EXE 的相同用例，
  保存原始 file/stderr/stdout，退出 0。阻塞期间 Info start 已落文件；同一个
  SceneCount 原因只记一次，文件和 Error stderr 均包含 request、generation、
  revision、elapsed 和原始 cause，后续真实 GPU 输出恢复。stdout 只有 libtest
  正常输出，没有产品日志；不能将其说成整个测试进程 stdout 为零字节。

原始命令、源码前后摘要、stdout/stderr 和实际像素输出保留在
`artifacts/issue-goal-current/native-pipeline-preparation/`；既有两个证据包不覆盖。
PNG 是真实合成输出，不是用户面板样式截图；本次没有修改面板或样式 token。

新增不可覆盖证据包 `exposure-pipeline-preparation-evidence.zip`，66 个成员、
2,419,368 字节，SHA-256
`a0bf1f1a7bea738c8f90d99c35d2f162df484104389c65e0e1fea6d2cd0b0f6c`。
包含十个已结束命令的原始结果与完整源码前后身份、原始失败、真实像素、
桌面日志、对应 native EXE 摘要及逐成员校验清单；已验证 ZIP CRC 和全部 SHA-256。

## 审计与仍未完成范围

| 项目 | 类型与处置 | 证据或限制 |
| --- | --- | --- |
| 64-E1 | 功能：曝光采样使用后台严格规划和原生创建 | Windows 原生窗口、调整层、子合成、同步像素对照 |
| 64-E2 | 功能/协议：精确采样身份、pending 不前进、保留旧图 | 八项准备测试，包含不同帧/实例/尺寸身份负对照 |
| 64-E3 | 既有观察与新错误边界：保留 typed cause、失败不重试 | 受控 panic、65 次相同失败及显式视图恢复；未新增日志 sink |
| 64-E4 | 观察：已有前台创建计数独立呈现 | Renderer 生命周期计数；不含固定合成器或后台创建 |
| 64-E5 | 正常分支：准备 pending、取消、快照容量限制、普通窗口刷新 | 沿用既有协议和限制，不把 pending 记为故障 |
| 64-E6 | 既有观察的完整桌面 native 资格：实际默认 logger 与 engine context | 原有 SceneCount 用例通过并保存原始三路输出；不覆盖非法 WGSL/panic hook 全部路径 |

本次不宣称 #64 完成。应用私有目录的缓存后台恢复/保存、普通预览的嵌套/时间
映射专属近期边界、最终可见准备提示、独立进程 cold/warm 和首次/再次添加效果
配对计时仍需完成。此前宿主子进程分配失败本轮完整库复查通过，旧失败证据
保留；完整宿主集成及完整桌面原生回归仍需执行，不能用库回归和编译检查替代。
曝光按精确采样上下文交接后台任务，其暖播放调度与规划成本尚未做配对计时，
不能用“前台效果管线创建为零”推导持续播放已提速。
