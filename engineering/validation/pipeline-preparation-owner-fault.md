# 空闲准备 owner 的终止故障验收

关联 #64、草稿 #627，补齐 [终止收尾验收](pipeline-preparation-terminal.md) 的
64-T7。起点为 `5ba4b9bb`；最终命令收据保存编译前后每个源码文件的 SHA-256，
不把未提交测试等同于该起点提交。

## 改动和故障边界

本批仅增加测试。`preparation_progress` 的一次性 owner 线程钩子、触发代码及
原生用例都受 `cfg(test)` 约束，普通应用不编译这些入口。产品的原始错误类型、
worker 终止、队列、超时、恢复协议和 UI 均不改变，没有增加重试或自动重启。
DESIGN §0.7 的面板条目全部保留，原因是本批不编辑面板或样式。

原生子进程初始化真实默认 Info 文件 logger、panic hook、Windows 隐藏窗口、
Engine 和 Vulkan 设备；实际私有缓存 callback 被门闩阻塞。PresentPoll 返回
false 后，在空闲 owner 的真实 `advance` 入口暂停，确认准备句柄仍忙、普通命令
in-flight 为空，再实际排入 State 与 Save 两个命令，确认队列深度为 2。

释放缓存 callback 和入口门闩后分别触发：

- **Session 失效**：在所属线程关闭真实 Session，再由原来的 `with_session`
  查询注册表产生 `native session is closed`，没有手工返回一个构造的 Err。
- **owner panic**：在相同边界 panic，沿用产品的 `catch_panic`、强制回溯 hook
  和原来的终止处理，不走普通命令 handler 的 panic 分支。

两种情况都要求：worker 实际终止；Session／surface factory 释放；准备原子句柄
清零；队列深度回到 0；cleanup in-flight 清除。保存文件字节不变，仍保留失败的
旧 Engine 时可以重新打开同一工程，并比较完整工程值。

排队命令尚未执行，原始 channel 在 worker 退出时断开；UI 的既有 `poll_reply`
必须返回 `ReplyDisconnected` 并保留同一个 primary。Session 错误保持 Host Arc
身份，panic 保持既有 Message 类型；随后 State 和 Save 直接返回同一个 primary。
不把断开的原始 channel 写成已发送的成功／错误回复，也不改变 respond 语义。

默认配置下，两个 owner stopped 记录在文件和 stderr 中各出现一次，stdout 无
原始原因。panic hook 的同一多行记录必须包含 owner 上下文、源位置、原始原因、
`Backtrace::force_capture`、`engine::preparation_progress::advance` 和
`motion_host::ops::catch_panic`。六个负对照分别移除上下文、位置、原因和三个
栈帧之一，都必须被拒绝；后续另一条 Error 中的相同原因不能补齐残缺 panic。
健康 Present 路径仍不产生无条件 Info start。

## 命令、首次结果与限制

最终源码共 2483 个文件的 SHA-256 相同，三个最终命令均核验编译／运行前后未改动
源码。普通构建没有 cfg(test)，确认测试入口不影响普通程序编译。

| 收据 | 结果 | 实际耗时 |
|---|---|---:|
| idle-preparation-fault-native-v3 | 新增原生故障用例及日志负对照两项通过，0 ignored；真实缓存 supported=true | 129.891s，含编译；测试 2.78s |
| idle-preparation-fault-regression-v2 | 六项原有帧完成测试通过，0 ignored，包括三个 Windows 原生用例及 52 张状态截图 | 51.203s |
| idle-preparation-fault-normal-build-v1 | 普通 FFmpeg／diagnostics／ai-depth 程序构建通过，36 条既有 warning | 57.265s |

新增用例命令：

```text
cargo test --locked -p motion-host -p motion-desktop --lib --bin motion-studio --features motion-host/diagnostics,motion-host/ai-depth,motion-desktop/ffmpeg engine::preparation_progress::native_tests -- --nocapture --test-threads=1
```

回归使用同一编译出的 motion-studio 测试 exe，过滤
`engine::frame_completion::tests --nocapture --test-threads=1`；未重新编译或修改
原用例。普通构建使用 `cargo build --locked -p motion-host -p motion-desktop
--bin motion-studio --features motion-host/diagnostics,motion-host/ai-depth,motion-desktop/ffmpeg`。
构建和原生驱动沿用独立 target、固定 FFmpeg DLL 与既有隔离临时目录，不改系统
临时目录 ACL，不删除驱动全局缓存。

证据包：[pipeline-preparation-owner-fault-evidence.zip](pipeline-preparation-owner-fault-evidence.zip)，
106 个成员，4747387 字节，SHA-256：
`48c4f16f33782be3b7608d48e21880777b745d6e7d243c1775906091480b3da1`。
包含所有首次失败、通过轮次、当时测试源码、最终源码、完整默认日志、命令收据、
截图和归档控制器。逐成员内容、SHA-256 清单和 zip CRC 已核验；不包含 exe、
DLL、私有驱动缓存或转储。

首次结果完整保留：

1. `idle-preparation-fault-native-v1` 退出 101。子进程两种真实故障和资源／工程
   检查均通过，但新写的父进程断言错误要求 panic 上下文和原因出现在同一行。
   实际 PanicInfo 将位置和原因分为两行。这不是产品故障。
2. `idle-preparation-fault-native-v2` 通过两项测试；改用同一多行记录解析并补
   缺字段负对照。复查第一轮原始栈还发现：链接器会折叠注入闭包，使闭包符号
   显示成另一个同构函数，不能以该闭包名作为必需证据。最终版本保留上下文、
   位置和原因检查，另要求强制捕获、advance 和 catch_panic 三个稳定栈帧，并
   分别增加缺帧负对照。没有因此修改产品 logger 或放宽已有产品断言。
3. `idle-preparation-fault-regression-v1` 退出 101：三个队列单元测试通过，三个
   原生用例在创建临时目录时即遇到系统 Temp ACL 的 PermissionDenied；原生
   子进程尚未启动。该启动命令漏带本任务原有的隔离 TEMP／TMP／TMPDIR。
   后续恢复原构建环境和隔离临时目录，再运行原测试，不修改其断言或期限。

## 处置账本

| ID | 分类 | 证据和边界 |
|---|---|---|
| 64-F1 | 既有终止分支补证据 | 真实 Session 关闭后由注册表产生错误，Host Arc 在后续调用中不变 |
| 64-F2 | 既有终止分支补证据 | 空闲 owner 入口 panic 被原 catch_panic 捕获，默认强制回溯和原始原因保留 |
| 64-F3 | 协议／资源边界 | 两个排队调用通过既有 UI 轮询拿到携带原始 primary 的断开错误；窗口和 Session 释放后同路径重开 |
| 64-F4 | 测试夹具修正 | 首次失败、当时源码、完整日志、最终多行解析和六个负对照均归档；未改既有断言 |
| 64-F5 | 正常分支回归 | 原有等待、取消、supersession、错误身份和准备状态用例原样重跑 |
| 64-F6 | 后续范围 | [多视图准备验收](multiview-pipeline-preparation.md)已补近期预热与普通程序冷／热进程；其余平台／格式组合及普通应用 native GPU 创建故障不由本故障测试替代 |

此处验证的是 owner 的终止收尾，不等同于非法 shader 的 CPU 校验或 native GPU
创建失败；既有独立证据仍保留各自边界。测试工程没有未保存编辑，本批不新增或
声称完成未保存工程的额外恢复策略。Windows/Vulkan 的实际通过也不替代其他
平台的运行证据。#64 和草稿 #627 仍保持打开。
