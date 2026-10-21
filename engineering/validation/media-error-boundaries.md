# 媒体故障类型与图集边界验收

本批补入媒体任务生命周期 PR，与 #130、#131 的清理边界共用验证；推进 #138 的媒体错误类型迁移。三个 issue 都是聚合项，本批没有完成全部要求，不使用关闭语法。

## 改动与兼容边界

- `motion-media::Result` 从字符串错误改为可克隆的 `Error`。IO、JSON、图像、模型、时钟、Symphonia 解码和原生后端保留各自原始错误；上下文和回滚保留主因及各项次因。不可克隆的原生错误通过 `Arc` 共享，`source()` 返回共享原因，可以检查枚举或下转为相应 `Arc<T>`；克隆不会用显示文本重建原因。
- FFmpeg 及渲染转换错误由桌面适配器保留为后端原因，不让媒体层依赖具体视频库或渲染层。视频 worker 状态保留类型，回到既有 JSON、任务状态、宿主字符串协议和日志器时才转为显示文本。媒体能力与解码回调采用媒体错误，窗口 surface 能力继续采用宿主错误。
- 音视频缓存 worker、包发布与包析构 panic 保留独立类别和既有有界原因。发布主因与清理次因不会互相覆盖。本批沿用前阶段的锁顺序、条件变量和取消行为，没有再加入等待、重试或重启。
- 字体图集的预检和实际栅格分配前均验证完整尺寸，乘法与 padding 加法显式检查溢出，窄化发生在尺寸验证之后。原有输入限制已挡住常规超限情况，这属于 #131 的边界加固，不宣称复现了原有真实字体崩溃。有效字体的排序、覆盖率和像素输出仍由原有真实字体回归验证。
- 仍保留 `Message` 作为旧文本校验、第三方能力和宿主适配的过渡边界；普通取消、目标被替代仍沿用原有判定。宿主总体字符串错误、组合错误前缀桥、渲染计划错误及 #138 的其他项目尚未完成。

没有用户可见 UI 或样式更改。仅新增对工作区已锁定 `thiserror` 的直接依赖边，没有新增第三方包版本或素材。已核对 PROVENANCE、ATTRIBUTION、NOTICE；测试复用仓库已有独立素材和测试临时文件。Android 目标未编译验收，不作为本批支持范围。

## 报错与功能审计

| ID | 分类 | 证据与处置 |
| --- | --- | --- |
| ME-01 缺失音频文件 | 原因保留；既有错误信道 | 公开解码接口执行真实 File::open，验证 NotFound、原始 OS 错误、克隆共享同一原因，以及增加路径上下文后仍可分类。没有创建 PCM。 |
| ME-02 损坏音频、字体目录与项目 | 原因保留；既有覆盖加强 | 实际 Symphonia probe、损坏 JSON 目录、字体解析、无效项目分别保留 Decode/Json/Font/Model，JSON 行号和原始错误对象可检查；文件内容保留。 |
| ME-03 PCM 截断与缺失 | 原因保留；既有断言保留 | 原有读缓存失败测试增加类型检查，真实截断仍为 UnexpectedEof，删除缓存仍为 NotFound，原有 asset/path/sample/kind 文案断言不变。 |
| ME-04 发布与回滚 | 协议边界保留 | 第二次 rename 失败保留 NotFound 并恢复旧文件；真实目录阻碍 remove_file 时，主错误和每个次错误各自保留类型，保留外来目录。前阶段持锁发布、锁外清理回归继续执行。 |
| ME-05 原生视频后端 | 原因保留 | 实际 FFmpeg 打开不存在的视频，保留 backend=ffmpeg 及原生 errno；原有视频帧、seek、stride、取消及元数据回归验证显示文案和正常行为。旋转错误中的元数据预览限制为 128 字符，原始解析错误仍保留。 |
| ME-06 日志信道 | 新增验收；沿用既有观察 | 新的桌面子进程使用真实缺失文件及损坏音频，经实际 AudioJobs worker 与默认日志器验证 start、终态原因、文件 Error、stderr Error 和 stdout 隔离。既有包/清理/播放日志用例继续验证阈值、锁顺序、取消和恢复。 |
| ME-07 图集尺寸 | 功能边界加固 | 检查整图乘法溢出、零尺寸、8192 单边与 16MP 界限，等于上限仍成功；现有有效字体测试保留像素一致性及缓存复用断言。不是新增观察或 UI 改版。 |
| ME-08 剩余范围 | 待处理 | 未更改未知帧数进度契约、Android 短读、音频输出时钟、解码线程退出/峰值和其他聚合项。原生调用不返回时的行为与既有 in-flight 观察保持一致，没有新增超时保证。 |

没有新增生产观察键或计数器、Present 起止日志或逐帧 IO。共享原因的保留范围仍是当前返回错误/任务状态，缓存回滚与 panic 观察沿用既有 2048 字符限额；现有成功、普通 miss、取消和 superseded 分支不新增 Error。

## 验证记录

原始日志位于 `E:/Dev/aem/artifacts/cache-publication-cleanup/`，不提交测试生成文件。使用现有便携 MSVC、锁定依赖、单任务构建及单线程测试；关闭调试符号和增量编译，保留调试断言。

- 媒体完整回归：`media-errors-final-media.log`。
- 宿主与 FFmpeg 媒体完整回归：`media-errors-final-adapters.log`。
- 默认日志、包清理与真实媒体失败：`media-errors-final-native.log`。
- 桌面 pending-video 与音频分析 worker：`media-errors-final-pending.log`、`media-errors-final-audio-worker.log`。
- 带 FFmpeg 的桌面构建：`media-errors-final-build.log`。

最终组合验收为 **257 通过、0 失败、2 项既有大文件测试忽略**，各日志互不叠加重复计数：媒体 115、宿主与 FFmpeg 适配器 117、默认桌面日志 15、pending-video 3、音频分析 worker 2、包事务与依赖边界 5。媒体负向对照恢复后的最终日志是 `media-errors-restored-media.log`；阶段性完整媒体日志不再额外计数。摘要为 `media-errors-test-summary.json`。

负向对照仅临时把公开音频解码入口的真实 File::open 错误转回字符串，再运行新增缺失文件用例。测试退出 **101**，明确报告原生 IO 类型丢失，`media-errors-string-mutant.log`。恢复前后源码 SHA-256 均为 `7c13e8a11827f97a9ee1d0b10d55c227489b77eaaae180b24fe1cf5b452b9266`，`media-errors-mutant-summary.json`；恢复后完整媒体回归再次通过。没有删除错误、换成成功返回或放宽既有断言。

默认 Info 原始信道输出保留在 `media-errors-native-output/verified-ehrawe52/`。真实缺失文件与损坏音频的 Error 在文件/stderr/stdout 为 **2/2/0**，start 为 **2/0/0**；包发布与析构六种故障组合为 **6/6/0** 与 **6/0/0**，真实清理 IO 的 Warn 为 **1/0/0**，`media-errors-native-summary.json`。这些子测试直接重跑只保留原始输出，不再累加测试数。

`cargo check --workspace --exclude motion-android --all-targets --features motion-host/diagnostics --locked --offline -j1` 成功，`media-errors-workspace-check.jsonl` 以 `build-finished success=true` 结束。该检查覆盖无 FFmpeg 的工作区接口；上面的原生运行验证启用了 FFmpeg。`cargo build -p motion-desktop --features ffmpeg --locked --offline -j1` 成功，22.15 秒，保留 21 条既有构建警告。桌面仅运行上述相关用例，不冒充完整 UI/采样成本验收或所有未合并分支的集成发行包。

新增三个 Rust 文件的 rustfmt、差异空白、来源与桌面样式消费校验通过；原有格式不一致文件没有全量格式化。类型迁移期间既有错误断言仅增加显式 `.to_string()`，相同字符串、相同谓词、相同取消确认条件保留。新 source 测试初版误将共享 `Arc<T>` 直接下转成 T，失败记录保留在 `media-error-full.log`；修正为检查共享对象指针、原始 kind/raw_os_error 及上下文链，未删除原因检查。接口适配的编译失败也保留在 `media-error-*-check*.jsonl`，不作为通过证据。

仅更新自己的媒体修复 PR，不合并其他 PR，不取消、重启或调度远程 CI。跨平台远程结果仍由现有审核流程确认。
