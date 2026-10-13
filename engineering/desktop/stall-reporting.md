# 桌面卡顿诊断与 Sentry

桌面端在内存中自动保留最近 128 条计时和最近 512 条超过 16ms 的记录。记录函数使用 try_lock 和有界非阻塞发送，不在 UI 或引擎线程执行文件或网络 IO。丢弃计数在报告中显示，不能将未记录的样本解释为没有卡顿。

记录包含已注册操作类型、UI 面板编号、播放/拖动状态、帧号、合成尺寸、图层和效果数量，以及可用的 GPU 名称和后端。报告不序列化工程名、图层名、素材路径、文本内容、表达式、着色器或完整工程，也不收集键盘文本和画面截图。

## 计时口径

构建时提供 MOTION_BUILD_COMMIT 或 CI 的 GITHUB_SHA 时，报告记录经校验的提交 SHA；缺失时保留 null。totalStalls 是超过阈值的计时记录数，同一次交互可能有多个阶段记录，不能据此计算独立掉帧次数。

- worker：工作线程收到的请求，分别记录排队等待和执行墙钟时间
- blocking_wait：调用方同步等待结果的总时间，不推测其中 GPU 或 CPU 所占比例
- response_poll：异步请求发出至 UI 观察到结果的时间，包含调度和轮询等待
- ui_build：面板绘制命令构建时间，排除原生文件对话框的用户等待

未知的阶段耗时写为 null。所有计时都是墙钟测量，surface 工作包含驱动和 GPU 队列等待，不等同于 GPU timestamp query 测出的执行时间。后台操作超过阈值也会被记录，需结合类别和上下文判断是否造成可见卡顿。

## 本地报告

通过“帮助 → 导出卡顿诊断”生成 JSON。也可运行：

```powershell
MotionStudio.exe --diagnostics-report "E:/reports/stalls.json"
```

该参数在编辑器退出时导出。自动检查点默认保存到应用数据目录的 diagnostics/last-stalls.json；使用 MOTION_DIAGNOSTICS_PATH 可重定向检查点文件。检查点合并到最多每 5 秒一次，报告限额 2MiB，以临时文件和 rename 替换正式文件，文件 IO 在独立报告线程执行。

MCP motion_state 返回 desktop.diagnostics，包含配置状态、用户开关、SDK 入队数量、丢弃数量和本地写入错误数量。queuedUploads 只表示已加入 SDK 队列，不能当成服务端已接收。

## Sentry 配置

1. 在 Sentry 创建 Rust 项目，复制项目 DSN
2. 通过运行时环境变量 MOTION_SENTRY_DSN 配置 DSN，也支持构建时注入同名变量
3. 在“帮助”菜单开启“允许后台上报卡顿诊断”

开关默认关闭，选择保存在桌面偏好中。未配置有效 DSN 时，后台上报菜单不可用，本地收集和导出仍可用。无需向应用提供管理 API 令牌。

```powershell
$env:MOTION_SENTRY_DSN = "https://PUBLIC_KEY@YOUR_INGEST_HOST/PROJECT_ID"
.\MotionStudio.exe
```

发送使用官方 Rust SDK，关闭默认集成、PII、自动堆栈和 session 收集；仅发送我们构造的事件与白名单 JSON 附件。原生网络请求仍需由 Sentry 的服务端数据保留与隐私配置管理。

同类卡顿以操作名称分组，间隔至少 5 分钟，每次应用运行最多入队 12 份报告；没有新增慢记录时不重复发送。关闭开关停止后续报告入队，已经进行中的网络请求可能已经发送。网络收发不参与 UI 和引擎命令通道，失败时可以继续导出本地记录。

## 验证边界

测试验证内存与 JSON 上界、记录锁竞争、原子替换及错误返回、未知阶段保持 null，以及官方 SDK 实际 envelope 的用户开关、附件和限流。模拟 transport 不访问网络。配置真实 DSN 后，还需在 Sentry 项目中核验事件及 motion-studio-stalls.json 附件，才能宣称云端上报验收完成。
