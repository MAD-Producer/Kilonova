# 矢量上传复用与成本记录验收

基线：`c5f81223`。本批合并推进 #126 的矢量顶点/分组 opacity 重建站点与 #60 的对应成本统计，不关闭两个聚合 issue。没有用户可见 UI、依赖、资产或着色器改动。

开始前检查开放 PR、本地分支和 worktree。`presenter-source-reuse`、`viewer-timeline-navigation` 与 `effect-input-order-allocations` 分别涉及 presenter 和效果输入顺序，本批不修改这些文件。旧 `egui-desktop-shell` 的相同文件差异仅为 wgpu API 迁移，主分支已有，不包含本次复用实现；蒙版输入绑定缓存也已存在。不管理其他人负责的 PR 或 CI。

## 条目处置

| 条目 | 类别 | 结果与边界 |
| --- | --- | --- |
| V126-01：动画矢量顶点缓冲 | 新的性能改动 | 扁平/分组路径共用按容量增长的顶点缓冲；指纹命中继续跳过栅格化与上传。不是缓存全部历史帧，几何增长仍会创建更大的缓冲 |
| V126-02：分组 opacity | 新的性能改动 | 一个可增长 uniform 缓冲、一个随容量更新的 bind group；按设备对齐约束写入动态偏移，保留子组与根组独立透明度。CPU 打包 Vec 复用容量 |
| V126-03：跨绘制上传顺序 | 功能边界保留并验证 | staging 内容通过编码器 copy 插入每次绘制之前；没有用同一提交前的多次 queue write 覆盖各图层输入。同一编码器两次 encode、多个图层与嵌套组均有真实 GPU 回读对照 |
| V126-04：staging 回收 | 新的资源生命周期实现 | GPU 完成 remap 后才能再次写入；空闲缓存同时限制 16 MiB/128 个缓冲，超限回收。取消未提交 encoder/command buffer 时由闭包析构扣减 pending，不记作故障；清理 working set 后的旧成功回调不能重新填满缓存 |
| V60-01：容量与上传记录 | 新的观察 | 普通呈现和曝光采样均记录顶点/opacity 当前容量、staging pending/idle 容量与数量、创建/复用次数、编码/成功 remap 字节、取消及 remap 失败次数；成本报告分开统计，保留既有逻辑纹理容量与 GPU 时间含义 |
| V-ERR：真实 remap 失败 | 新的错误边界和观察 | 原始 `BufferAsyncError` 被保留在 `VectorUploadError` 的 source 中；映射范围错误、尺寸/对齐错误和锁中毒显式返回。真实异步 remap 失败首次记录 Error，包含 layer/frame/bytes/elapsed_us/cause；持有池锁时不调用 logger。失败后上传返回保留的错误，不自动重试，release 不清除故障 |
| V-NORMAL：正常命中、取消与卸载 | 保留的正常分支 | 指纹命中、没有可复用 staging、缓存超限、未提交工作被丢弃和 owner 消失不是错误；成功热路径无新增 Info/文件 IO。没有新 worker、超时/重启或错误弹窗 |
| V-EXISTING：预算与日志基础设施 | 既有覆盖 | 纹理预算拒绝、正常清理、GPU 故障观察和桌面日志路由继续沿用既有实现；没有把这些既有能力计作新修复 |

所有总计均为单个 renderer 生命周期内的饱和累加值，清理资源不重置总计。首次真实 remap 失败按池生命周期保留并只记录一次 Error，后续回调增加失败计数；这不是“连续 60 次降级”计数。池不因一次成功回调或 working-set 清理恢复。原始原因只保留一个，键不随图层数增长。

## 测量口径

`encodedUploadBytesTotal` 表示已编码 copy 的有效数据字节，包括后来取消的编码；`completedUploadBytesTotal` 只在对应 GPU remap 成功后增加，不把失败、取消或仅编码算成完成。pending/idle 字节是源缓冲容量而非有效字节。创建次数只覆盖该池及顶点/opacity 目标缓冲，不表示整个渲染器的对象创建次数。

`vectorBufferCapacityBytes` 为当前顶点、opacity、pending staging 和 idle staging 容量之和。它不包含已被替换但仍由在途 GPU 命令引用的旧目标缓冲、CPU opacity Vec、驱动实际驻留和其他缓冲；pending 随实际编码的在途工作增长，16 MiB/128 约束仅作用于空闲缓存。原有逻辑纹理预算没有被扩大或重新解释为整机显存上限。

使用仓库锁定的 wgpu 30.0.1，没有新增依赖。顺序依据官方 [Queue 文档](https://wgpu.rs/doc/wgpu/struct.Queue.html) 与对应版本的 [CommandEncoder::map_buffer_on_submit](https://docs.rs/wgpu/30.0.1/wgpu/struct.CommandEncoder.html#method.map_buffer_on_submit)；这里自行管理小型 staging 池，以保留异步错误和取消清理的可验证状态。

## 验证

Windows 原生 MSVC，锁定离线依赖，debug assertions 开启，使用现有 FFmpeg 共享运行时。GPU 为 NVIDIA GeForce RTX 4070 Laptop GPU，驱动 591.86，Vulkan；适配器原始信息保留在 `pixels-final.log`。原始结果位于工作区 `artifacts/vector-upload-reuse/`，它们是本地验收材料，并非已发布的跨设备性能基线。

- `pool-final.log`：3 个上传池单元测试通过，覆盖编码顺序回读、复用、容量增长/缩小、取消、旧回调、128 个句柄与超过 16 MiB 的真实容量边界、实际原生映射失败和故障保留。早期 `final-pool-filtered.log` 命令过滤掉的集成用例不计入通过数。
- `render-regression.log`：既有像素/资源预算回归与成本记录通过。首次新增复用测试误把预热后的同一内容当成一次新上传；后来补上显式缓存命中断言，并从变化后的内容开始测量。已有断言未削弱。
- `restored-pixels.log`、`pixels-final.log`：新增 2 项实际 GPU 用例通过，后者还包含 3 项成本记录测试。三图层预热创建 3 个目标缓冲（含一次容量增长）、5 个 staging；之后 5 次颜色/透明度变化仍为 3/5，累计复用 24 次。透明 RGB/alpha、跨图层隔离、同次提交内两次 encode 分别验证；清理后容量归零，总计保留。
- `mutant-recreate-vertices.log`：临时恢复已存在图层的逐次顶点重建，创建次数断言失败；`mutant-wrong-opacity-offset.log`：临时破坏动态偏移，真实像素 alpha 断言失败。均退出 101，原文件 SHA-256 还原记录在 `mutants.json`，还原后复验通过。
- `host.log`：`cargo test -p motion-host --features diagnostics --locked --offline -j1`，完整宿主 86 项通过。
- `logging-final.log`：`cargo test -p motion-desktop --features ffmpeg logging:: --locked --offline -j1 -- --nocapture`，11 项通过，包括新增真实 GPU 销毁后 remap 失败。`native-output.json` 及 `native-vector-upload/` 保留默认 `MOTION_LOG` 未设置时的原始输出；新站点 Error 在文件/stderr/stdout 中分别为 1/1/0 条。受控销毁还会触发既有 GPU 故障记录，未伪装成正常完成。该子进程复验不重复计数。
- 去重后共 155 项通过：`render-regression.log` 53、上传池 3、最终像素/成本记录 5、宿主 86、桌面日志 11，扣除重复运行的成本记录 3；还原复验和原生日志子进程不重复计数。原始计数见 `test-summary.json`。
- `workspace-check.jsonl` / `.stderr`：`cargo check --workspace --exclude motion-android --all-targets --features motion-host/diagnostics --locked --offline -j1 --message-format=json`，最终 `build-finished success=true`，包含测试 target。
- `build.log`：`cargo build -p motion-desktop --features ffmpeg --bin motion-studio --locked --offline -j1`，42.17 秒成功，21 条已有警告。新增 Rust 模块/测试的 rustfmt 检查、差异空白、provenance 和桌面样式消费守卫通过。没有全量格式化历史文件。

新增夹具最初的项目版本、嵌套几何默认白色填充与首次容量增长假设错误，保留在早期失败日志中；不作为通过证据。没有变更现有测试断言来通过验收。

## 剩余范围

同一 PR 后续补入 #138/#135 的 GPU 回调摘要约束和真实默认日志验证。此前共享目标目录的结果保留为阶段记录；当前分支的最终独立目标目录构建、覆盖范围及重复项去重见 [GPU 回调错误摘要与真实失败信道](gpu-callback-bounds.md)，不把两阶段测试数相加。

#126 的其他站点及全预览堆分配验收、#60 的完整所有权台账、上传/readback 覆盖、真实设备/驱动/API 性能基线与 P50/P95/P99 对照仍未完成。本批证明对象复用、像素语义、计数口径和指定失败路径，未测量播放 FPS 改善或总显存下降。Android 已退出本次桌面验证范围；macOS/Linux 结果由后续 CI 验证，不能用本机编译替代。
