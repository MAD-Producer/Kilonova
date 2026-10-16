# 引擎侧可观测性验收

日期：2026-10-17。分支：`feat/engine-observability`，基于远端 main `ffb8864`。

## 行为与边界

- 文件日志位于 `directories()/logs/motion-studio.log`，启动保留一份 `.log.1`
- `MOTION_LOG` 默认 Info；普通命令的 start 在 handle 前写入并刷新，done/dispatch 使用 Debug
- Present 不写 start/dispatch；worker done 仅在耗时至少 100ms 时写 Debug，已有 timing 阈值及 debug_assertions 保留
- panic hook 强制捕获 backtrace，持锁路径使用 try_lock，失败回退原始 stderr，再调用原 hook
- Flight 在 handle 前发布、diagnostics.record 后清除；in_flight/queue_depth 无需向 worker 排队
- send 成功后的加计数和 recv 后的减计数使用同一短锁，避免抢先接收造成 AtomicUsize 下溢；Shutdown 同样经过 dispatch
- 普通 Err、panic 和阻塞调用超时携带 request/operation/耗时；保留回复接收方已离开时的丢弃行为

| 进程级降级 site | 上下文 | 成功时 |
| --- | --- | --- |
| renderer.image_prefetch | source_id、解码错误 | 解码成功清零 |
| renderer.prepare_scene_assets | source_id、prefetch_failed、decoder_busy | 所需图片准备完成清零 |
| desktop.present.surface | surface 状态、window | acquire 成功清零 |
| host.acquire_preview_surface | surface 状态 | acquire 成功清零 |

每个 site 首次失败记 Debug，连续第 60 次记一次 Error，之后静默直至成功清零。日志包含 site/count。

每次 trim（包括无需淘汰的提前返回）比较 texture_bytes 与 images 中的驻留字节之和。失配首次及每 60 次记 Warn，恢复一致清零。video_upload_bytes 是累计上传流量，不能当作驻留量比较；告警同时记录它和 video_planes，不修改原有记账或淘汰行为。

## 已执行验证

本机编译环境通过 `. .\.tools\msvc-portable\env.ps1` 加载，全程使用已有依赖的离线模式。

- `cargo check -p motion-desktop --offline`：通过
- `cargo test -p motion-desktop --offline -- --include-ignored`：321 passed，0 failed，0 ignored，包含原先默认忽略的真实音频设备测试
- `cargo test -p motion-render --offline --lib observability::tests`：通过，捕获实际 log 验证首次、60 次、只升级一次及恢复重置
- 新增 TestWait 回归测试在 worker 阻塞期间读取 Flight 和排队深度，释放后等待 record/clear 完成并验证空闲状态
- sink 测试验证 Info 缓冲、Debug 过滤和 Warn 立即刷新；子进程测试验证轮转、被 catch_unwind 捕获的 panic 栈和同线程持锁时继续 unwind
- 实际运行 command-list 和 MCP，逐行 JSON 解析 stdout；MCP 查询 state 成功，无效编辑返回错误且文件中有 request/operation/ms
- 实际运行原生 GPU smoke，默认环境移除 MOTION_LOG，确认 Info start 已落盘；重复 Debug smoke，确认没有 Present start/dispatch，任何 Present done 都受 100ms 门控
- smoke 报告为 ok，包含渲染、保存重开、浮动窗口、resize/redock 和截图；designCaptureProjectUntouched 为 true，实际查看主窗口截图
- 新模块 rustfmt --check 和 git diff --check 通过
- `desktop/app/src/panels/**`、dark.json 和 shell/presentation.rs 均无修改；既有两个 MOTION_TRACE_UI stderr trace 保留

本地原始记录位于 `target/observability-validation/`，包括 desktop-check-final.log、desktop-tests-final.log、render-counter-tests.log、runtime-summary.json、启动日志和 smoke 报告/截图。正常日志目录为 `%LOCALAPPDATA%/MotionStudio/logs`。

## 责任边界

DESIGN.md §0.7 视觉整改基线全部保留，原因是本次仅补引擎观测、不编辑 panel；未将已有视觉问题标记为修复。此次 smoke 证明原生窗口和绘制链可用，并不替代 §12 全部 DPI、快捷键、IME 与控件状态的完整人工验收。

未注入真实 surface 丢失或损坏图片；连续失败升级与恢复由计数器实际日志测试覆盖，接线点经源码审查。没有引入 worker 超时/重启、UI watchdog、last_error/message 结构变更或新的图像失败处理策略。

仅启用 Cargo.lock 已存在的 log 依赖，不新增第三方资产；来源遵守 PROVENANCE.md、ATTRIBUTION.md 的 Rust 依赖规则与 NOTICE。
