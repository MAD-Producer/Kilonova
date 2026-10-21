# 导出快照线程的并发与退出契约

关联 #130 第七项中的 `motion-freeze-movie`，仅完成这一类线程的所有权与回收。起点为 main `26b834f7`；#390 在独立分支验证输入路由，`feat/semantic-resource-contracts` 继续处理精度与渲染基线。开始前已核对分支、工作树和开放 PR，本次没有修改其源文件或缓存。

## 功能变化与观察范围

| ID | 分类 | 实现与证据 | 限制 |
| --- | --- | --- | --- |
| MFL-01 | 功能修正 | 每个 Engine 持有一个快照线程及其取消令牌；正在运行或取消时，明确拒绝后续冻结请求，并在捕获新源文件句柄前检查容量。受控测试持有实际源文件，阻塞时连续拒绝 64 次请求，额外闭包均未执行。 | 上限为每引擎一个；不同引擎互不共用该槽。不是排队、自动重试或重启。 |
| MFL-02 | 功能修正 | 退出先取消未交付任务，再 join；持有源文件及临时副本的线程退出后，才允许引擎释放工程会话与工程锁。受控阻塞验证取消已生效时析构仍未返回，释放屏障后线程回收完成。 | 不能中断永久不返回的系统 IO。join 可能等待当前 IO 返回，不能据此声称关闭时间有上限。其他编码、PNG、媒体注册表线程不在本次范围。 |
| MFL-03 | 功能边界保持 | 完成与关闭通过短临界区交接；结果就绪后不再因回收线程而取消调用方后续导出。既有 ExportControl 和 wire reply 保持。 | 关闭与尚未交付的冻结并发时仍允许正常取消。没有修改 MP4 编码或像素语义。 |
| MFL-04 | 新增观察 | 在进入源文件复制前发布独立在飞状态，保留原请求 ID、操作名和开始时间；state.desktop.movieFreeze 提供耗时、进度及线程状态，Engine 也保留可直接读取的在飞记录。沿用 Info async start，并增加 freeze_sources 阶段。 | 在飞状态不依赖串行 worker 正在处理的命令；正常查询无逐帧文件 IO，不增加 Present start/done 日志。 |
| MFL-05 | 新增观察与已有覆盖 | 回收遇到 handler 以外的 panic 时，显式传播或记录原始原因与原请求；清除在飞状态。实际向已退出接收者发送具有故障析构的值，验证 join 识别线程 panic，清理状态，并允许用户显式发起下一请求。 | 不改变 respond 的尽力回复语义；没有吞并 panic、重启线程或伪造成功。操作系统 spawn 失败保留原因，但未注入真实 OS 资源耗尽。 |
| MFL-06 | 正常分支 | 原始取消回复仍为 movie export cancelled，以 Info 记录，不作为 Error。默认 logger 子进程验证 IO 错误、handler panic 与关闭回收失败进入文件和 Error stderr，取消不进入 Error，stdout 不混入应用日志。 | 不把缓存未命中、接收端已退出或取消计为故障次数；沿用既有 panic hook。 |
| MFL-07 | 既有观察基础设施 | 新的共享状态与完成门使用既有 recovered/Counter，不在锁内调用 logger。各固定站点在进程寿命内累计中毒恢复次数，健康访问不重置；计数饱和，首次 Warn、第 60 次 Error。 | 本次未为这些新互斥量逐一做中毒注入；不声称这些站点已有单独的 60 次阈值验收，也不把次数解释为持续秒数。 |
| MFL-08 | 待办 | #130 其他线程类别、长 GPU 操作的事件循环逃生路径与全应用退出验收仍须分别完成。 | 不关闭整个并发合集，也不把源码审计等同于故障实测。 |

没有新增依赖、第三方资产、兼容格式、效果包字节或渲染实现；已检查 PROVENANCE.md、ATTRIBUTION.md 和 NOTICE。DESIGN §0.7 十二项均 retained：未编辑面板、token、布局、菜单或交互命中区，现有视觉整改留给对应任务。没有以编译成功代替视觉验收，也没有重跑或替换图像基线。

## 本机验证

Windows portable MSVC，固定 FFmpeg 环境；锁定离线依赖、单编译任务与单测试线程。沿用本工作树的专用 target，未停止其他任务的进程。日志保留于开发者工作区 artifacts/pr-ci-followups。

- movie-freeze-manager-final.log：6 项通过，包括 5 项任务生命周期测试与 1 项实际 Engine → FrozenMovie → 关闭 Engine → MP4 编码 → libav 解码测试，输出含 2 帧。真实导出测试仅在 ffmpeg feature 下启用，不冒充无 FFmpeg 的跨平台 CI 覆盖。
- movie-freeze-default-log.log：扩展既有默认日志子进程测试后通过。首跑失败原因是读取前未刷新 Info 缓冲；显式刷新后保留原始失败、panic 与信道断言，未放宽既有断言。后续又补充了回复清理失败的 join 错误信道检查，以最终完整故障组结果为准。
- movie-freeze-failure-final.log：完整故障组 21 项通过、0 失败，74.89 秒，包括实际默认 logger 的关闭回收失败、正常取消、原始 handler panic 和故障回复信道。没有删改既有断言以绕过故障。
- movie-freeze-workspace-check.log：完整工作区 all-targets（排除 Android、启用 motion-desktop/ffmpeg）检查通过，23.74 秒。
- movie-freeze-desktop-full.log：完整桌面回归 **715 项通过、0 失败、1 项既有物理音频设备测试忽略**，452.21 秒。包含上述 6 项任务测试和 21 项故障组，不重复累计；保留 19 条既有测试编译警告。

上述完整运行基于 main 26b834f7 与本次修复。期间 #390 已经独立通过四项必需门禁并合并为 3dc9c573；其新增原生输入测试不在本次 715 项中，不混算两次运行或声称已经运行本次合并后的全部用例。可运行构建及最终 PR 的跨平台 CI 分别追加。

movie-freeze-desktop-build.log：带 FFmpeg 的 Debug 构建通过，21.85 秒，保留 21 条既有警告。可运行目录为 artifacts/movie-freeze-lifecycle/windows-ffmpeg-debug；复制前使用既有打包器核对固定运行库的架构、作者、来源、修订、许可证及每文件 SHA-256，保留 7 个 DLL 和原始许可证。清除开发环境 FFmpeg 路径与 FFMPEG_* 变量后，命令列表和实际 silent-24fps.mp4 视频 smoke 均退出 0，stdout 为有效 JSON；详情见 local-build-validation.json。程序 SHA-256 为 59538107586a83364642ce1c06c022b75a09866d8adaedf3054f513cd6613d11，不伪造 Release 报告。来源守卫、面板样式消费守卫和差异空白检查均通过；新增模块已 rustfmt。
