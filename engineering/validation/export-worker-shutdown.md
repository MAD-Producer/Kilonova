# 编码与 PNG 导出线程的退出验收

关联 #130 第七项的 motion-movie-export 和 motion-frame-export；基于 #391 的冻结线程修复 `89848816` 接续，不把冻结任务的测试重复归为本次新增工作。开始前核对分支和开放 PR：显存预算分支正在修改渲染资源台账与 Engine 预算接入，本次只改桌面导出任务的所有权和退出处理，未编辑其文件。

## 功能与观察分类

| ID | 分类 | 改动与证据 | 边界 |
| --- | --- | --- | --- |
| EWS-01 | 功能修正 | MovieExport 持有原生编码线程；PNG worker 保留原请求并持有原有线程。两者析构均先取消、再 join，等待资源释放后返回。受控测试使用实际源文件和线程屏障，验证取消已发布但析构尚未完成，解除屏障后才退出。 | 普通 start/poll/cancel 仍异步；析构现在可能阻塞。这是功能变化，不伪装为单纯日志。永久阻塞的系统 IO 或 GPU 调用不能被该取消令牌强制中断，关闭耗时无上限保证。 |
| EWS-02 | 功能边界保持 | 成功结果、原始失败和已发布文件不因回收线程而丢失。MovieExport 的二次清理 panic 单独记录；状态已有主要错误时保留它，首次出现的线程异常才补失败状态。 | cancel 返回 false 表示结果已发布，是既有正常终态，不新增降级告警。保留 respond 和原有结果协议，没有添加超时、重试、重启或返回伪成功。 |
| EWS-03 | 新增观察 | 在等待仍运行的线程前，Info 写明任务、阶段和耗时并刷新；join 的异常写 Error，保留原始 panic 原因。MovieExport 记录 revision 与受限输出路径；PNG 记录原请求与耗时。例行 join 完成使用 Debug。 | 不新增 Present 热路径日志或正常轮询 IO。MovieExport 原入口没有请求 ID，不能宣称凭空补齐该标识。 |
| EWS-04 | 功能修正 | PNG 下一个请求回收已完成但异常退出的线程时，保留原始 join 错误并清除过期在飞状态，再返回调用方。受控线程真实 panic 后调用生产 start，验证错误上下文、空槽与状态复位。 | 不隐式替用户重试请求，也不为失败后的下一次请求编造成功。 |
| EWS-05 | 默认信道实测 | 真实 logger 子进程验证 PNG 和编码线程的回收异常进入文件、Error stderr，Info 等待记录不写 stderr，stdout 不混入应用日志。编码清理同时有原始错误时，状态保留原错误，日志记录二次 panic。 | 受控原生线程产生 panic，调用的是生产析构路径；不声称注入了真实驱动内部故障。 |
| EWS-06 | 实际导出与事务 | FFmpeg 用例通过生产 Engine/FrozenMovie/MovieExport 完成两帧 MP4 并实际解码，验证正常取消和真实受阻目录错误；生产 PNG 路径实际保存并解码图像，预先取消时保留原输出字节。 | FFmpeg 条件用例只算本机覆盖，不冒充无该 feature 的 PR CI。既有渲染像素基线不改动。 |
| EWS-07 | 既有基础设施与待办 | 状态与在飞记录复用 recovered/Counter，日志在业务锁外写入。新 join 站点按进程内固定站点累计中毒恢复，健康访问不重置，饱和计数首次 Warn、第 60 次 Error。 | 未逐一注入这些互斥量中毒；不声称已做新站点的独立阈值验收。其他媒体注册表线程、硬件永久停滞和全应用退出验证仍属 #130 待办，不能关闭整个合集。 |

DESIGN §0.7 十二项均 retained：没有改面板、样式、菜单、尺寸或命中区，不以编译当作视觉验收。检查 PROVENANCE.md、ATTRIBUTION.md 与 NOTICE；没有新增依赖、第三方资产、工程/效果包格式、渲染语义或基线。movie_export.rs 的既有紧凑代码同时按 rustfmt 展开，仅格式变化的部分不算功能修正。

## 本机记录

沿用本工作树的 portable MSVC、固定 FFmpeg、专用 target、锁定离线依赖、单编译任务和单测试线程；未操作其他任务的源文件或进程。日志保留于 artifacts/pr-ci-followups。

- 首轮 export-shutdown-movie-first.log 和 export-shutdown-png-first.log 各 3 项通过。之后加强实际输出错误的默认信道检查，并补 PNG 已完成线程 panic 回收测试；只按最终结果累计。
- 最终专项、完整桌面、工作区检查及带 FFmpeg 构建仍在运行，以完成后的日志追加结果，不能沿用 #391 的 715 项作为本次完整回归。

export-shutdown-movie-final.log：最终编码任务 3 项通过，12.63 秒，实际输出目录受阻的原始错误进入状态、默认文件及 Error stderr；取消保持 Info，stdout 隔离。export-shutdown-png-final.log：最终 PNG 任务 4 项通过，7.65 秒，包括追加的已完成异常线程回收。两组共 7 个不同测试，后续若被完整桌面运行包含则不重复累计。

## 完整复验与程序

- export-shutdown-desktop-full.log：**722 项通过、0 失败、1 项既有物理音频设备测试忽略**，597.36 秒。包含本次 7 项与 #391 的冻结任务测试，不重复累计专项结果；不将耗时与不同负载环境的旧运行当作性能对比。
- export-shutdown-workspace-check.log：启用 motion-desktop/ffmpeg、排除 Android 的 workspace all-targets 锁定离线检查通过，22.19 秒。
- export-shutdown-desktop-build.log：带 FFmpeg 的 Debug 构建通过，21.91 秒。保留 19 条测试编译警告与 21 条程序编译警告，没有声称零警告。
- 可运行目录 artifacts/export-worker-shutdown/windows-ffmpeg-debug：通过既有 runtime_payload 核验固定 FFmpeg 的架构、作者、来源、修订、许可证与逐文件 SHA-256，包含 7 个 DLL 和原始许可。清除开发用 FFmpeg 路径及 FFMPEG_* 后，命令列表、实际视频 smoke 均退出 0，stdout 为有效 JSON。程序 SHA-256 e769ce585de45a838957d3dcc6a9c3b43c7f317e11a894060ced1a8e49357ee8，详见 local-build-validation.json；没有伪造 Release 报告。

完整运行基于 89848816 与本次改动，未包含 #390 新增的 Windows 原生输入测试，不能冒充最终 main 合并树的完整运行；PR 必须另跑自己的跨平台门禁。来源守卫、面板样式消费守卫与差异空白检查通过，新文件已 rustfmt。未做全应用句柄普查、真实永久驱动阻塞或临时目录清理磁盘故障注入，不能据此关闭 #130 全集。
