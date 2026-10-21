# 媒体任务启动边界与锁中毒

推进 #130 第六项，基于 #393 的 `73540134` 堆叠实施。开工前检查其他工作树：资源预算分支正在编辑 motion-render，精度分支保留独立工作；本次只改 motion-media 的音视频任务启动及专项验证，不修改 Session、渲染、模型或面板。

## 实现与功能边界

此前音频导入／缓存、视频导入／缓存共有八处生产 `task.lock().unwrap()`。四处为取得启动快照而重新锁定已发布的任务；另四处在线程创建失败后更新终态，遇到中毒会再次 panic，盖掉原生创建失败。

现在 reserve 在发布任务前复制并返回 queued 快照，启动入口不再为初始回复重新取得任务锁。线程创建失败的两个共享终结函数使用既有 RecoveryScope 恢复中毒 guard，写入 failed 状态、failed 阶段及完整原始错误；释放 guard 后才调用既有 TaskLog。返回值仍为原生 IO 错误，保留错误类型及 OS code，不用锁中毒文本替代它。没有清除中毒标记，公开 status／cancel／release 的中毒错误边界保留。

功能变化是启动回复固定为入队快照，以及创建线程失败后的 phase 从 queued 改为 failed。没有新增重试、重启、超时或线程回收，不能据此宣称导入／缓存 worker 已可 join。

新增两个固定观察站点 `audio.start.spawn_failure_task`、`video.start.spawn_failure_task`，复用现有 RecoverySite 的进程生命周期计数：首次恢复 Warn，第 60 次 Error，其余抑制；饱和计数不回绕，成功及正常 miss 不清零。未把次数换算为时间。事件字段及观察缓冲沿用既有上界；原始创建失败仍经 TaskLog 单独记录为 Error。RecoveryScope 的逆序析构以及显式释放任务 guard，确保宿主 logger 不在业务锁内执行。

## 证据审计

| ID | 分类 | 验证与限制 |
| --- | --- | --- |
| MTP-01 | 构造及功能修复 | 两个 reserve 在发布 Mutex 前复制初始状态，四个启动入口接收该快照。专项用例检查任务后来变为 failed／中毒时，初始 request、operation、running／queued 和无错误保持独立。生产音视频任务中八处裸锁 unwrap 被移除；其他合法构造不变量不在此计数。 |
| MTP-02 | 真实中毒、注入的创建失败结果 | 音频／视频专项分别覆盖 import、probe、cache 的健康锁及中毒锁，共 12 个隔离子场景。通过持有任务锁时实际 panic 制造 poison，在共享创建失败边界注入带 OS code 的 IO 错误，检查原生类型／code／文本、终态及阶段、已有 status 的中毒错误和正常 release。没有通过耗尽系统线程制造实际 Builder::spawn 失败，不据此宣称系统资源耗尽路径已端到端验收。 |
| MTP-03 | 默认级别与锁外记录 | 单独子进程以 Info 级别 Capture 检查一次原始 Error、首次恢复 Warn、request／operation／phase；日志回调 try_lock 任务及注册表以检查锁已释放。Capture 是宿主 log 接口的单元验证，不等于桌面文件／stderr 实测。桌面既有真实 IO 失败的默认文件／Error stderr／stdout 隔离回归另行执行，覆盖共用 TaskLog 输出层；未通过公开桌面接口注入 Mutex poison。 |
| MTP-04 | 既有阈值、恢复及正常分支 | 复用既有 poison_observation 的首次／60 次、饱和、展开及锁外输出测试；音视频任务的取消、提交、清理、失败保留和工作额度验证保留。新专项手动释放持有的 Worker 令牌并检查额度恢复，不将其当成真实 OS 创建失败的闭包析构证据。 |
| MTP-05 | 验收负面对照 | 临时把音频创建失败 helper 改回 `task.lock().unwrap()`，专项退出 101，实际中毒子场景出现 PoisonError；恢复原字节后整理新增代码的换行，并重新编译完整媒体及桌面日志回归。没有放宽断言、清除 poison 或提交错误变异。证明专项能拦截中毒级联，不代表实际 OS 线程耗尽已验证。 |

## 本机验证

同一工作树与专用 target，便携 MSVC、锁定离线依赖、保留 debug assertions，编译及测试均单任务执行，桌面测试启用 FFmpeg。所有编译／测试进程结束后才继续源文件编辑。

- 最终完整 motion-media：123 通过、0 失败、2 项既有大文件忽略。新增两个 Rust 测试，内部合计 12 个隔离子场景；不重复累计子场景数。
- 桌面默认 `logging::`：21 通过、0 失败、0 忽略，37.80 秒。覆盖实际 IO 故障的文件、Error stderr 与 stdout 隔离，新增 helper 的真实中毒在 MTP-02／03 的库侧验证；两者验证范围有区分。
- 工作区 all-targets 检查（排除停止维护的 Android，启用桌面 FFmpeg）：通过，51.24 秒。
- FFmpeg Debug 程序构建：通过，24.16 秒；保留 21 条既有二进制警告及 19 条既有桌面测试构建警告。
- 来源守卫通过（1662 个 tracked paths）、面板样式消费者守卫通过、新增测试 rustfmt 及差异空白检查通过。
- 程序带入经 runtime_payload 校验的 7 个固定 FFmpeg DLL 和许可证，清除开发 FFmpeg 配置、PATH 仅留系统目录后，命令清单有效 JSON，实际解码 24fps／72 帧视频成功。Debug 程序为 `artifacts/media-task-poison-boundaries/windows-ffmpeg-debug/motion-studio.exe`，SHA-256 `72195ec57ce1f0abec9c0a4a431f72d26a9642c71a6decdd133426226293caa5`；不是 Release 产物。

原始日志及负面对照证明位于本机 `artifacts/pr-ci-followups/media-poison-boundaries-*`，独立程序的输入哈希、运行验证和共享库来源记录位于上述程序目录。远端跨平台 CI 尚待提交后验证，不以本机通过替代跨平台证明。

未新增依赖、素材、用户可见 UI、工程格式或渲染语义。DESIGN §0.7 十二项均 retained，逐项原因沿用 host-error-contract.md 的 DESIGN 表；未编辑相关面板，不能据编译或后端测试声称视觉整改完成。#130 其余线程生命周期及整体验收仍开放。
