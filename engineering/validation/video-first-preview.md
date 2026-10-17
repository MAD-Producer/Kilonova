# 视频首帧等待与编辑器呈现

用户报告导入后首次看到视频、播放进入视频图层起点时都会卡顿，并提供了 FFmpeg `UDTA parsing failed retrying raw` 输出和一次运行日志。本次只修改呈现等待路径，没有改动面板布局、样式、媒体格式或依赖。

## 证据与修复范围

用户日志中，导入从 1792246846.682 开始，到 1792246847.129 完成后续媒体请求；视频 worker 在 1792246847.141 开始打开。下一条 seek 在 1792246848.931。只有 Info 片段不能证明这段间隔全部用于解码，也不能将 worker stopped 的生命周期时长当作单帧耗时。

在 `Session::render_with_overlay` 中，`prepare_scene` 正常返回 `None` 时，旧实现直接 `Ok(false)`，没有提交任何窗口画面。即使解码已在独立线程运行，编辑器控件也一起停止呈现。受控阻塞首帧的真实 Engine、隐藏原生窗口和 GPU 用例在旧代码上失败：`no editor chrome was presented while the video decoder was blocked`。

修复后，等待期间仍将当前编辑器 paint 呈现到表面。存在有效合成缓存时复用最后完成的画面；首次没有缓存时清空底层表面。`presented` 记录实际表面呈现，但合成结果继续返回 `Ok(false)`，不更新 `lastPresentedFrame`，不将尚未完成的合成写入 `CompositionStamp`。因此 Shell 仍会按既有 pending 协议请求后续呈现，解码完成后才确认新帧。

这修复了可复现的“等待解码时连同界面一起冻结”。未将其描述为所有首次解码耗时已经消除；原工程的连续播放、首次显示延迟和跨图层时的实际体验仍需复测。

## 错误处理审计

| 编号 | 分类 | 证据与边界 |
| --- | --- | --- |
| VFP-01 | 功能修复 | 改变正常 pending 分支的表面呈现；继续返回未就绪，不伪造成功、不更改播放时钟或工程 revision。 |
| VFP-02 | 既有错误信道 | 解码 `Err` 和 GPU/表面真实错误继续向上传递。真实 worker 注入首帧失败后，父进程确认默认 info 文件和 Error stderr 保留原始原因，stdout 无该日志。 |
| VFP-03 | 既有观测 | worker open start、in-flight 状态、持续 pending 计数及原有失败阈值没有静音。没有新增健康 Present 的 Info、逐帧文件 IO、重启、重试或锁内 logger。 |
| VFP-04 | 正常分支 | 解码尚未完成、首次没有合成缓存、表面暂不可取得继续遵守原有协议；没有把正常 cache miss 改为故障。 |
| VFP-05 | 外部警告与待验证项 | 同一用户视频可复现 UDTA 警告，但可成功解码。没有过滤 FFmpeg 输出、修改媒体文件或把元数据警告认定为完整停顿的根因。原工程长时间停顿的其余原因仍待新运行证据。 |

## 验证记录

本地原始记录位于 `E:/Dev/aem/artifacts/workspace-stall/video-pending-chrome-*.log` 和 `E:/Dev/aem/artifacts/video-first-preview/`，不随 Git 提交用户媒体或其内容。

- 修复前复现：`video-pending-chrome-baseline-valid-fixture.log`，有效工程 fixture 上真实失败。更早两次分别因测试误用 Layer 字段、fixture 视频可恢复区间不足而失败；随后修正 fixture，未放宽断言。
- 修复后专项：`video-pending-chrome-fixed.log`，同一用例通过。覆盖首次无画面、先有画面后进入延后视频层、解码失败三种情况；阻塞时仍有实际表面呈现、合成帧不提前确认，解除阻塞后才得到成功或原始错误。
- 桌面完整回归：`desktop-tests.log`，372 项通过，0 失败，1 项需要实际音频输出设备而按原规则忽略，116.73 秒。样式消费者检查、独立测试文件 rustfmt 和 `git diff --check` 通过；17 项既有测试编译警告未在本次变更。
- 实际用户媒体只读验证：2560×1440、386 帧、约 12.87 秒、7,088,952 字节。最初旧 Release 的 `--video-smoke` 成功，总共约 2062ms，包含完整探测与四个位置的解码和 CPU RGBA 转换，不能当作首帧延迟。
- 临时分阶段 harness 未提交产品代码。固定 Windows FFmpeg `n8.1.3-9-g29e619e767-20260930` 下，探测约 477.25ms、解码器打开约 33.68ms、首帧解码约 14.29ms、进程首次 YUV 上传约 9.72ms、第二次约 2.08ms；Renderer 初始化另计约 9527.74ms。这是未优化 harness、已有系统驱动缓存且 Release 编译并行期间的单次测量，不代表用户完整工程、冷驱动缓存或播放帧率。先前本机 SDK `n8.1.3-16-ge0a878dd70` 的测量另存，不冒充固定构建输入。
- 原 FFmpeg SDK 和旧交付目录在本次调查期间已不可用。恢复过程中保留网络超时、缺失头文件的首次 harness 构建失败记录；最终源码归档与 Windows 运行库均重新取得并通过仓库固定 SHA-256 校验，使用现有 `prepare_ffmpeg.py` 生成许可、来源和逐文件清单，没有修改下载校验或依赖版本。
- 为预编译依赖而启动的临时本机 SDK Release 构建，在固定 SDK 恢复后主动中止链接；不计为完成构建。最终固定 SDK 构建、打包和原生验收结果随后补充。

本次没有新增资产或依赖，未修改 `desktop/app/src/panels`；DESIGN.md §0.7 的既有面板基线均保留，不能据此宣布既有视觉问题已修复。
