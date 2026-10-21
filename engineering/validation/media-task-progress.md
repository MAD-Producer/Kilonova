# 媒体未知进度契约验收

关联 #131；这是其中未知总帧数进度的后台实现，不关闭整个媒体健壮性合集。起点为 #389 当前格式改动 `d9a7eac6`，已快进接入 fixture 修正 `f9dadb0a`；不重复归属工程格式或浮点渲染的工作。

开始前核对所有 worktree、近期分支和开放 PR。浮点曲线与渲染由 `feat/semantic-resource-contracts` 处理，未触碰其工作区、源文件或基线。本分支修改媒体回调/任务状态及宿主适配器；桌面文件只包含测试供应方签名和既有日志故障验收，不改变面板呈现。

| ID | 分类 | 状态与证据 |
| --- | --- | --- |
| AP-01 | 功能契约修正 | `DecodeAudio` 与共享音视频任务用 Option 表达进度，未知为 None/JSON null。不是错误观察；没有为正常未知总量新增告警。 |
| AP-02 | 真实解码、输出保持 | 测试从仓库现有 FLAC 在临时目录生成总采样数为零的合法 STREAMINFO，保留采样格式、MD5 与压缩帧。已知和未知源经过实际 Symphonia 解码，元数据与 PCM 完全一致；所有未知进度检查均为 None，已知比例仍有效且前进。未添加第三方资产、修改既有文件或清除校验和。 |
| AP-03 | 取消边界保持 | 真实解码检查点的原始取消原因保留；受控 worker 阻塞允许检查 running/decoding/null 状态，取消后不发布成功、metadata 或 Error，不保留 staging。正常成功的实际 worker 发布 succeeded/1，探测结果不留暂存树。未增加线程超时、重启或重试。 |
| AP-04 | 视频音频适配边界 | 视频事务测试由受控适配器先报告未知，再使用实际 MP4 音频解码。检查 decoding_audio/null、准备完成/1、取消与释放；不把该用例声称为新的未知时长视频解包器。 |
| AP-05 | 非有限值错误与已有观察 | 非有限比例返回错误，拒绝将 NaN 自动序列化为 null；复用 TaskLog 的开始、失败及默认日志信道。桌面真实文件 logger 的故障/恢复用例已扩展，实际验证默认文件、Error stderr、stdout 隔离与后续实际解码恢复。没有新增生产日志站点、键、计数器或热路径 IO。 |
| AP-06 | 协议与功能边界 | 有限比例限制到 [0,1]；音频沿用既有范围语义，视频也统一约束。排队、打开和不可计量阶段明确为未知。PackageTaskStatus、导出、采样、工程 JSON 和像素计算不变。 |
| AP-07 | 待办 | 用户可见的状态呈现后续处理，桌面旧的零值显示回退仍存在；本轮不关闭 #131。Android 仅同步 trait 类型，未做设备验证；跨平台 CI 以新提交为准。 |

## 本机记录

日志位于开发者工作区 `artifacts/pr-ci-followups/`。锁定离线依赖，便携 MSVC、调试断言开启、单任务与单测试线程；使用专用 target，没有操作其他任务的编译进程。

- `media-progress-contract.log`：4 项真实 FLAC/worker 契约测试通过。后续完整运行包含这些用例，不重复累计。
- `media-progress-media-verified.log`：媒体完整测试 **120 项通过、0 失败、2 项既有大文件测试忽略**，包含 4 项新音频测试和 1 项新视频适配器测试。
- `media-progress-workspace-check.log`：带 FFmpeg、排除 Android 的完整工作区 all-targets 锁定离线检查通过，1 分 15 秒。保留已有 unused/dead_code 警告；编译成功不等于全部桌面/渲染运行验收。
- `media-progress-first.log`：首次编译失败，因为新 import 置于 video.rs 的模块文档之前；将 import 放到文档之后，不更改模块文档语义。
- `media-progress-media-final.log`：新增视频测试误用现有 VideoTaskStatus 未实现的 Debug 格式；改为输出现有 JSON，原有 ready/进度断言保留，随后完整媒体运行通过。

当前媒体结果对应后台实现，不沿用 #389 的 CI 或其他分支的图像验收。默认日志故障、完整宿主、桌面构建和后续集成结果分别追加；未完成项不得记为已通过。

## 最终信道与集成复验

- `media-progress-adapters-final.log`：完整宿主与带 FFmpeg 的桌面媒体适配器（含宿主 doctest）共 **147 项通过、0 失败、0 忽略**。
- 进入桌面验证时，`media-progress-native-logging.log` 出现 5 处相互一致的 f64/Option 类型冲突，而当前源码及 Cargo metadata 均指向 Option。此前在两个 worktree 间复用了相同 target；按已确认的绝对目录定向清理 motion-media、motion-host、motion-desktop-media 的构建产物，没有更改正确的回调类型或测试断言。
- `media-progress-workspace-clean-check.log`：清理后完整工作区 all-targets（排除 Android、FFmpeg）重新通过，1 分 07 秒。之后一直在本 worktree 构建，避免旧的依赖变体混用。
- `media-progress-native-logging-clean.log`：桌面完整日志模块 **20 项通过、0 失败、0 忽略**，包括真实缺失文件、损坏音频、非法比例的失败信道，以及失败后的实际音频解码恢复。未通过的旧缓存编译不计为测试通过。
- 再直接运行同一真实 logger 子进程并保留原始信道：`media-progress-native-output-1f622845`。Error 在文件/stderr/stdout 为 **3/3/0**，Info start 为 **4/0/0**，正常恢复没有 Error，退出 0；摘要为 `media-progress-native-summary.json`。该次只保存证据，不重复累计测试数。
- 本轮媒体、适配器与桌面日志三个不重叠组合共 **287 项通过、0 失败、2 项既有大文件测试忽略**。格式分支此前完整桌面 709 项对应 `f9dadb0a`，不冒充本轮 API 改动后的完整桌面回归，也不再与其中的日志子组相加。

按用户减少 CI 排队的要求，后台进度与当前格式一起提交 #389，不新建额外 PR。用户可见 UI 与其他聚合子项仍是待办；新提交的四项跨平台必需门禁须全部通过后再合并。

`media-progress-desktop-build.log`：带 FFmpeg 的 Debug 程序构建成功，28.93 秒，保留 21 条既有 warning。本地完整目录为 `artifacts/media-task-progress/windows-ffmpeg-debug`，沿用前轮已校验来源及 SHA-256 的固定 FFmpeg DLL 和原始许可证，仅替换本次编译的程序。清除开发环境 FFmpeg 路径和 FFMPEG_* 变量后，命令列表与实际 silent-24fps.mp4 视频 smoke 均退出 0、stdout 为有效 JSON；详情在 `local-build-validation.json`。没有伪造 Release 报告或改动第三方字节。来源、样式消费、差异空白守卫通过；新增两个 Rust 文件已 rustfmt。
