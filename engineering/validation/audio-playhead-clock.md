# 音频播放头与实际寻帧消费验证

关联 #138 第六项和 #135 的设备无关测试要求。已有实现按 PCM 设备采样数推进、补偿输出延迟并在欠载时冻结；本批补充公开 `AudioOutput::frame` 及实际 Shell 消费路径的可执行验收，不宣称新修复了这些既有播放行为。

## 协调与实现边界

从宿主错误 PR #388 的 `5f8b44f1` 开始，开工前核对开放 PR 的文件范围：#380 处理模型不变量，#382 处理合成打包，#383 处理矢量上传/GPU 观测，#386、#387 处理资源预算。没有修改这些实现。另检查已有 `fix/audio-waveform-progress`，其改动是 mixer 波形进展守卫，不是播放设备时钟。

Shared 中增加仅 `cfg(test)` 存在的原子时间输入，缺省继续使用原来的 Instant。公开 frame/error、生产环形队列和回调不替换，不打开物理音频设备，不启动监控线程，也不依赖 sleep 推进被测时钟。非测试构建没有新增字段、分支、锁或回调 IO。

## 逐项证据

| ID | 分类 | 证据及边界 |
|---|---|---|
| APC-01 时间注入 | 测试能力补充 | 公开 frame/error 读取测试原子时钟；普通测试默认 None，现有时钟仍生效。没有更改生产时间源或新建可观测性计数器。 |
| APC-02 播放头 | 既有功能新增覆盖 | 真实立体声 PCM 经生产 ring 与 render_callback，覆盖 44.1/96 kHz 和 24/30/60 fps，非零起播样本、延迟前不推进、提交量上界、欠载静音与恢复不倒退、合成样本终点截断。断言包含实际 PCM 与欠载计数。 |
| APC-03 实际消费方 | 既有功能新增覆盖 | 临时工程中创建真实 Shell/Engine，使用生产 advance_playback，验证音频帧号成为实际异步 seek 的返回帧号；结束时停止播放、释放音频、停在合成末帧且不生成错误提示。没有模拟成功响应或只比较本地公式。 |
| APC-04 负面对照 | 新增验收有效性 | 暂时取消生产 frame 中的设备采样率换算，新用例退出 101，实际 24.6615、预期 24.72。恢复后原源码 SHA-256 相同，保持完整断言不变。该错误未提交。 |
| APC-05 故障与正常分支 | 既有覆盖、未改观察行为 | 音频完整筛选保留设备回调超时、初始化/混音 panic、欠载、停止、默认日志与恢复等既有用例。没有新增错误观察、返回类型变化、阈值、重试或线程重启；正常静音和结束不变成故障。 |
| APC-06 限制 | 明确保留 | 注入回调不等同于物理声卡验收；既有真实设备用例仍按其原条件忽略。#138 的渲染计划及模型不变量、#135 的其他条目仍待分别完成，不自动关闭合集。 |

## 验证记录

Windows，FFmpeg 与 host diagnostics 启用，debug assertions 保留，同一独立工作区及专用 target，测试单线程：

```text
cargo test -p motion-desktop --bin motion-studio --features ffmpeg,motion-host/diagnostics --locked --offline -j2 audio_output:: -- --test-threads=1
cargo check --workspace --all-targets --exclude motion-android --features motion-host/diagnostics,motion-desktop/ffmpeg --locked --offline -j2
```

最终音频回归 19 项通过、0 失败、1 项既有硬件设备忽略，7.43 秒；工作区检查通过，18.00 秒。没有放宽既有断言、删除测试或新增忽略条件。新文件 rustfmt 与差异空白检查另行通过。

原始证据在开发者工作区 `artifacts/pr-ci-followups/`：`audio-public-clock-positive-final.log`、`audio-public-clock-negative.log`、`audio-public-clock-restored-final.log`、`audio-public-clock-workspace-check.log`。恢复前后本地 audio_output.rs SHA-256 为 `9CD6FBE473943093748ED07C30DE4BA7E3F3553EA61FEB7512CE1DD0AC3DBA80`。

保留两次验证过程问题：首版新用例要求 44.1 kHz 下 25 ms 恰好为整数样本，实际为 1102.5；改为两种设备采样率都恰为整数样本的 30 ms 检查点，严格精度仍为 1e-10。负面对照之后首次复制恢复保留了旧时间戳，Cargo 未重新编译，运行了仍含负面对照的旧二进制；该失败不作为恢复结果。更新时间戳、确认重新编译之后，上面的最终完整音频回归通过。

没有新增依赖、素材、可见 UI 或面板源码变更。DESIGN §0.7 的十二项既有基线均 retained，理由是没有涉及表现层，逐项表沿用 host-error-contract.md；不以本次编译或测试通过声明视觉整改验收。
