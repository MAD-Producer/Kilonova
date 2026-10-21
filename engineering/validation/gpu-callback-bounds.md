# GPU 回调错误摘要与真实失败信道

本批合入既有矢量上传 PR #383，继续推进 #138 的故障上下文边界、#135 的真实 GPU 故障覆盖，减少新增 PR 和重复排队。#126/#60 的上传复用改动保留，聚合 issue 均不因这批局部完成而关闭。没有用户可见 UI、样式、依赖、素材、Android 或 CI 修改。

开工前逐个检查 worktree 在 `renderer/gpu_failure.rs`、其测试目录及新桌面日志测试文件的提交差异与未提交内容。没有相关未提交占用；两个可观测性工具分支的旧恢复修改与当前 main 对应文件逐字一致，不重复实现。另行检查发现会话锁收窄已经在 `fix/session-registry-scope` 实现，故没有修改该路径。协调记录保留在 `artifacts/backend-next-batch/gpu-failure-ownership.json`。

## 功能与观察边界

| ID | 分类 | 处置与边界 |
| --- | --- | --- |
| GCB-01 错误 latch | 功能修复 | 新回调写入的首因/设备丢失文本最多 2048 UTF-8 字节。长原因保留开头和末尾，以标记替换中间；原始根因通常位于长标签之后。短原因逐字不变，后来的普通回调仍不替换首因；设备丢失仍按既有语义替换 latch。 |
| GCB-02 日志上下文 | 既有观察加边界 | 回调日志的原因最多 2048 字节，source 调试摘要、适配器名和驱动名各最多 512 字节。观察对象不再保留没有输出用途的完整 driver_info；Renderer 自身的原始适配器查询不变。日志在 latch 解锁后输出。 |
| GCB-03 阈值与正常分支 | 既有策略保留 | 每个 Observation、每种 uncaptured 错误种类分别在首次 Error、第 60 次 Warn；设备丢失有独立计数。计数是实例生命周期累计、饱和增长，随实例销毁结束，健康查询不重置。主动 Destroyed 保持 Debug；健康 check_health/read 不新增日志、IO、重试或重启。关闭日志时仍保留错误与计数，额外 source 摘要不格式化；以 Debug 会 panic 的错误 source 验证该分支，而非只检查没有输出。 |
| GCB-04 控制失败 | 新增覆盖 | 超长多字节/control-character 上下文与真实错误对象验证 latch、计数、源原因、两端摘要及解锁后日志。既有 poison 恢复、短原因保持测试原样保留。受控 Unknown 仅验证该回调分支，不冒充实际驱动重置。 |
| GCB-05 原生失败 | 新增覆盖 | 实际 WGPU 创建设备后，以超长标签和非法 buffer usage 触发生产 uncaptured 回调。在任何 gpu_error/check_health 调用前读取真实文件日志，证明故障无需下一帧推动；再检查首因、后续错误抑制和真实主动销毁。默认 Info 验证文件/stderr/stdout 分流。 |
| GCB-06 剩余限制 | 待处理 | 原生错误先由 wgpu/HAL 构造，格式化仍可能有临时分配；这些摘要上限不代表驱动或原生错误构造的总内存上限。没有将原有字符串 latch 全面迁移为类型化 GPU 错误，也没有为非 Destroyed 设备丢失制造真实驱动重置。 |

Error 的操作、renderer ID、累计次数、耗时、适配器/backend/vendor/device、源原因继续保留。截断仅影响超长诊断文本，不把错误改成成功，也不清空原始故障。既有测试直接注入的历史 latch 不被一次正常读取重新改写。

## 构建隔离与原生崩溃复核

旧共享目标目录的本轮首次构建报告包发布 trait 需要 `&mut self`，但该分支 `motion-model/src/boundaries.rs` 明确定义 `publish(self)`，相邻媒体/核心调用也使用消费式接口；#381 的另一个 worktree 才采用借用式接口。失败输出保留在 `gpu-callback-unit.log`，不能算源码回归通过。最终验证改用本分支独立的 `E:/Dev/aem/target/vector-upload-reuse-verified`，从空目标目录重编译，不以共享目录的旧测试结果代替本轮验收。

上一批 #384 的并行访问冲突在 Windows Application/1000 系统事件中定位到 `C:/WINDOWS/SYSTEM32/vulkan-1.dll`，版本 1.4.321.0、异常 0xc0000005、偏移 0x432bc。该事件只证明故障模块，不能证明根因。2 线程帧混合 GPU 组 11 项通过，完整宿主并行复跑 57 项通过，但不能据此宣称偶发崩溃已解决。原始记录在 `artifacts/frozen-reader-isolation/native-fault-events.json`、`parallel-frame-blend.log`、`parallel-host-recheck.log`；不重复累加为本批独立测试。

来源规范、ATTRIBUTION 与 NOTICE 已核对，本次没有新增第三方输入。

## 验收结果

本轮均为独立目标目录、MSVC、锁定离线依赖、debug assertions 开启、关闭调试符号与增量构建、两个构建任务、单测试线程。证据在 `artifacts/vector-upload-reuse/`，汇总为 `gpu-callback-verification.json`。

1. 初次干净编译的四项回调测试通过，编译 3 分 33 秒；实际桌面默认日志用例通过，额外编译 2 分 40 秒。`gpu-callback-isolated-unit.log`、`gpu-callback-isolated-native.log`。这些不是额外独立用例，后续完整运行已覆盖。
2. 首轮 `gpu-isolated-render.log` 包含库 47 项及九组集成 55 项，合计 102 通过；`gpu-isolated-host.log` 完整宿主 86 通过，0 失败/忽略。之后仅补日志关闭时避免额外 source Debug 格式化的保护和一项新回归，未修改图形或宿主语义。
3. 最终 `gpu-final-render-lib.log`：完整渲染库 **48 通过**；`gpu-final-logging.log`：桌面日志 **12 通过**。关闭日志用例保留首因与 120 次累计计数，底层 source 的 Debug 若执行会真实 panic，不能仅靠“没有日志”通过。
4. 去重后 **201 个不同测试通过**：最终渲染库 48、先前未受日志门控调整影响的渲染集成 55、宿主 86、最终桌面日志 12。首轮库、聚焦回归、原始输出留档的子进程和负向对照不重复计数。不把这些运行称为完整桌面套件。
5. 负向对照只移除新首因的长度约束，保持其他逻辑不变，回归因 `first.len() <= CAUSE_BYTES` 失败，退出 **101**；finally 原字节恢复后通过，退出 **0**。`gpu-callback-mutant.log`、`gpu-callback-restored.log`、`gpu-callback-negative.json`。最终恢复 SHA-256 为 `41e012b1a687744195edffe620f4f5d356406bdf1e71b8ea9b7e9c7ede4f159e`；没有提交变异实现或放宽旧断言。
6. `gpu-callback-native-output.json` 及 `native-gpu-callback-6opbmzbj/`：真实默认 Info（MOTION_LOG 未设置），指定 GPU 回调 Error 在文件/stderr/stdout 为 **1/1/0**；文件 **4448 字节**，stderr **3414 字节**。原始适配器为 RTX 4070 Laptop、NVIDIA 591.86、Vulkan。错误在任何 health 查询前已进文件，摘要含 buffer 标签两端及末尾 usage 根因；后续错误不覆盖首因，主动销毁没有额外故障记录。销毁后的测试使用 GPU 等待以排空回调，子进程有 45 秒外部上限；这不是新增生产超时或重试。
7. 最终工作区 `--all-targets --exclude motion-android --features motion-host/diagnostics --locked --offline` 检查通过，**35 秒**，`gpu-final-workspace.log`。带 FFmpeg 的桌面构建通过，**17.28 秒**，保留 21 条既有构建警告，`gpu-final-build.log`。
8. 三个新增/修改的故障 Rust 实现及测试文件 rustfmt、暂存差异空白检查通过；来源守卫覆盖 **1569 个跟踪路径**，桌面样式消费守卫通过。没有全量格式化历史生产文件。远程 CI 仍需按最新提交验证，先前 head 的绿色检查不冒充最新检查结果。

## 模型、钢笔主线合并后的复验（2026-10-21）

#380、#379 已合入后，将本分支同步到主线 `c390e3851de4af8b55f8acb01afc4c536a6e4987`。唯一文本冲突位于 Renderer 的相邻诊断接口：同时保留本分支 `vector_upload_info` 和主线 `submission_memory_snapshot`，没有移除提交台账接口，也没有改变上述复用及故障语义。相对该主线只有原有上传、GPU 观察和验收文件差异，无新增产品 UI、依赖、资产或 CI 改动。

沿用独立目标目录 `target/vector-upload-reuse-verified`、原生 MSVC/FFmpeg、锁定离线依赖、debug assertions 开启、两个构建任务及单测试线程。证据位于 `artifacts/pr-ci-followups/`：

- `vector-current-main-check.log`：工作区 `--all-targets --exclude motion-android --features motion-desktop/ffmpeg,motion-host/diagnostics` 检查通过，1 分 53 秒。
- `vector-current-main-render.log`：完整渲染库 **52 项**、成本记录 **3 项**、矢量复用真实 GPU 像素 **2 项**通过；包含主线替换纹理在途保留、台账上限及预算拒绝回归。两个像素用例继续验证预热后创建次数不增长、跨图层隔离和一次提交内两次 encode 的独立输出。
- `vector-current-main-upload-logs.log`、`vector-current-main-gpu-logs.log`：各 **1 项**真实原生失败通过。未设置 MOTION_LOG，实际销毁设备触发 BufferAsyncError、非法 buffer usage 触发生产 uncaptured 回调，断言默认文件/Error stderr/stdout 分流为 1/1/0、上下文和有界摘要；子进程内断言不重复计数。
- 本次合计 **59 个不同测试通过，0 失败、0 忽略**。这是最新组合的定向回归，未重跑完整宿主或完整桌面运行套件；不与早期 201 项相加。前一次仅同步旧主线 `3dfa5e1e` 的检查和 `--no-run` 输出保留为阶段记录，不计作当前运行通过。
- 来源守卫覆盖 **1638 个跟踪路径**、桌面样式消费及差异空白检查通过。仅有既有编译警告。

最新提交仍须重新通过远程平台 CI；旧 head 的绿色状态不能代替本次检查。上述复验未证明历史 Vulkan loader 访问冲突的根因或修复，也不扩展为 FPS、整机显存下降或聚合 issue 已完成的结论。
