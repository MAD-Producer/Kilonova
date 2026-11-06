# 管线预热期间的桌面首帧完成约定（#64）

基线为 `7cf26f865f4a314667b20ca9bbacb553814d0456`，接续
`desktop-driver-cache-storage.md`。本轮修复桌面请求的完成约定，不修改效果
着色器、合成像素语义、面板布局、设计 token 或已有测试的断言与等待预算。
PR #627 与 #64 仍未完成，保持草稿。

## 问题及行为

宿主把实际管线准备移入 worker 后，首个呈现步骤可以正常返回 `Ok(false)`。
旧桌面接口却把该步骤当作整个请求已完成：单次 `Present` 的调用者拿不到
完整合成画面，`Render` 的短轮询也可能在 worker 完成前结束。原工具原生
验收已在本机复现这一回归，旧失败证据保留在上一轮记录。

- `Present`、`PresentFrozen` 和 `Render` 在确实等待已有管线任务时保留原
  请求，等 owner 安装准备结果后完成。没有合成区域的 chrome 呈现直接执行。
- Shell 改用明确的 `PresentPoll` 步骤接口，未完成时立即回复 false，使原生
  事件循环继续更新窗口、播放屏障和交互。没有把未完成画面伪装成 true。
- 完成队列上限 128，另有原命令队列的既有独立上限。队首等待时每 8 ms
  只收集已提交任务的结果，不重复采样、创建替代任务或等待驱动。后续完整帧
  请求按顺序保留，防止主窗口与浮窗相互替换同一准备任务。
- 恢复检查包括工程 generation、工程 revision、合成 ID、帧、视图 revision、
  surface epoch、窗口 ID 及原生尺寸。过期请求正常取消并回复 false；关闭
  浮窗时立即取消它的请求，避免同 ID、同尺寸的新窗口接受旧请求。
- 正常关闭在释放被阻塞的预热 worker 前取消待完成回复；故障退出保留原始
  Error 身份。没有增加 worker、后台重启、错误重试或隐藏原始失败。
- 宿主新增 owner 轮询接口，仍在既有 Session 线程边界内收集和安装结果。
  只有已提交的缓存恢复计入帧等待；配置缓存能力或保存检查点不算等待帧。

## 观察与错误信道审计

| ID | 分类 | 实际证据与处置 |
| --- | --- | --- |
| 64F-1 | 功能修复 | 原单次工具 Present、视图及原播放研究不改断言通过；新原生用例要求完整请求在阻塞时不回复，解除后真实 inverted 像素出现。已验证 |
| 64F-2 | 功能边界 | 真实缓存 Restore callback 阻塞时，Shell Poll 回复 false，chrome-only 呈现回复 true，状态查询和编辑仍可处理。已验证 |
| 64F-3 | 正常等待 | 每场景 65 次状态查询期间 renderAttempts 不增加，callback 只进入一次；宿主另验证 ticket 和 worker ID 不变、无替代任务。已验证，不将等待计为故障 |
| 64F-4 | 取消边界 | 原生 edit、close/reopen 同 ID 同尺寸、shutdown 三场景分别验证旧回复 false、新请求成功或在释放 gate 前取消；单元验证关闭仅取消指定窗口且恢复当前请求上下文。已验证 |
| 64F-5 | 新增观察 | `state.desktop.frameCompletions` 暴露有界 pending、limit、原请求 ID、operation、window、waitingForPipeline、elapsedMs；完成后 pending=0。默认 Info 文件初始化存在，健康 Present 没有 Info start，缓存 Info 未写 stdout。已验证这些明确断言 |
| 64F-6 | 故障保持 | 宿主真实准备任务受控 panic 后，65 次轮询保留同一 typed PipelinePreparation 的 Arc 身份、ticket 与 worker；桌面单元另验证 fatal drain 保留原 Error、正常 drain 为 false。已验证这两个层次，未冒充完整桌面管线故障端到端验收 |
| 64F-7 | 既有观察回归 | 重跑原 worker panic/surface 清理、effect pick、shy、toolbar、effect drag 等原生测试，保留其原日志与信道断言；受控 panic 的原始 stderr 未删除。已验证，未将其登记为新日志实现 |
| 64F-8 | 新增功能／观察边界 | 容量耗尽经既有 respond 返回显式错误，继续使用原错误文件／stderr 机制；本轮没有强制填满 128 个请求，也未注入实际 fatal dispatch 锁中毒。实现存在，待专项验证 |

非 Present 请求首次进入 handler 前继续发布原 in-flight 与 Info start；等待恢复
保留原 request、起始时间和诊断上下文，不重复 start。等待项在有界状态中可见，
串行 worker 空闲时不伪装成持续执行 handler。完成耗时包含等待时间，健康
Present 不产生 start/done；慢 Present 沿用既有 Debug 阈值。

普通表面 miss、无窗口、取消、过期及缺失缓存不转换成错误。原 render 的
其他软 false 返回约定未改；`Session::last_error` 对 pending 判断的所有
组合尚无穷举验收，不能把本轮结果扩大为所有降级路径已完成。

## 当前源码的实际验证

所有下列命令运行时的 2471 个源码／清单文件哈希一致，且各命令前后未变。
旧测试未修改，所有按名称运行的二进制用例都先在 `--list` 中核对存在，并
检查输出确实执行 1 个测试，而非零测试成功。共 **38 passed、0 failed、
0 ignored**，另有全目标编译检查通过。没有声称完整 CI 或全测试套件已通过。

| 原始 phase | 结果 | 用时（秒，含该命令的编译） |
| --- | --- | --- |
| frame-completion-lifecycle | 4 passed，含一个原生子进程的 complete/edit/reopen/shutdown 四场景 | 279.375 |
| frame-completion-host | 14 passed | 150.531 |
| frame-completion-original-single-present | 1 passed | 14.735 |
| frame-completion-original-viewer | 1 passed | 9.203 |
| frame-completion-original-paced | 1 passed，四模式各 5 预热与 60 完整帧，保留原帧前进断言 | 53.547 |
| frame-completion-queue | 4 passed | 8.188 |
| frame-completion-playback-barriers | 4 passed | 18.344 |
| frame-completion-existing-panic-scope | 1 passed | 10.859 |
| frame-completion-existing-effect-pick | 1 passed | 11.297 |
| frame-completion-existing-shy | 1 passed | 12.953 |
| frame-completion-existing-toolbar | 1 passed | 21.719 |
| frame-completion-existing-camera | 1 passed | 16.875 |
| frame-completion-existing-work-area | 1 passed | 12.125 |
| frame-completion-existing-effect-drag | 1 passed | 13.000 |
| frame-completion-existing-model-import | 1 passed | 11.735 |
| frame-completion-existing-model-layer | 1 passed | 13.500 |
| frame-completion-all-targets | render/host/desktop，FFmpeg + diagnostics + ai-depth，all-targets 检查通过 | 90.062 |

本机 Windows MSVC / Vulkan 的四个新原生场景均实际支持驱动缓存，受控阻塞
分支确实执行。测试对不支持缓存的适配器另有显式降级结果；本轮没有用该分支
替代四场景阻塞验收。全目标检查仍有编译 warning，原始 stderr 完整保留，
不声称无 warning 或完成了其他模块的清理。

当前测试二进制为 153317376 字节，SHA-256
`3f3caf28c8ade87cf9ca2a26fa6e8ac30bbb1fe389647d164c24e1c5464bceb6`。
这只是本机 debug 验收二进制，不能视为发行包身份。

## 历史失败与复核范围

初次 `frame-completion-desktop` 编译退出 101：callback 缺 Arc 包装、失败回复
Option 类型不明确。确认编译进程终止后修复，原始输出保留。中间三项通过的
`frame-completion-desktop-types-fixed` 先于关闭／重开和 shutdown 用例，
不用于证明最终源码。后续没有仅为通过验收修改既有断言、预算或 ignore。

旧 `fa9bfca4` Windows run 42213685935 为 1724 passed、12 failed、69 ignored；
其中十二个原始失败名称均在上述本机当前源码上逐项通过。新源码尚未提交时的
`7cf26f86` run 42225037209 观察快照：audit、Rust/GPU、Ubuntu 成功，Windows、
macOS Intel 仍运行，libav skipped。这些是各自 revision 的事实，不是当前
源码 CI 结果。旧日志不删，不把独立重跑十二项称为旧整套 CI 已修复。

## 证据重建与尚未完成范围

`desktop-pipeline-frame-completion-evidence.zip` 有 257 个成员，10590109 字节，
SHA-256 `be72602517c648e219054b638d75ceab6b96e33ab46e748503208cc9389d58ea`。
每成员 SHA-256、CRC、大小已核验；包含最终／历史运行的原始命令和 stdout /
stderr、每次运行前后源码身份、修改文件当前及基线字节、tracked diff、原生
截图／报告／日志、原测试列表、采样报告、旧 CI 失败日志和打包控制器。未修改
文件从基线提交重建；新未跟踪文件的字节在 sources/current 中，不依赖 diff。
源码先被冻结并完成运行，本说明与证据包随后写入；没有用文档改动触发源码复验。

本轮完成首帧约定修复，#64 仍缺：嵌套／time-remap 独有未来边界、用户可见准备
状态、实际应用独立进程 cold/warm 与首次／重加效果配对耗时、完整格式／精度／
包失效和平台矩阵、真实账户路径启动以及多视图准备整合验收等。容量拒绝、完整
桌面待完成请求的管线故障端到端及上述未覆盖组合也仍待验证。此前记录的 Unix、
Windows reparse、存储 callback panic／清理双失败等未完成项未因本次回归通过
而消失。无新增依赖或第三方资产，不改变渲染基线。
