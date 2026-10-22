# 矢量路径与三角化错误边界（本地完成，远端待验证）

继续处理 #138、#135，在已空闲的本人工作目录使用 `fix/cpu-geometry-errors`，基于数值分支 `68b5b4e0`，将数值和矢量修复合成一个后续 PR。未编辑仍在完整回归中的 `fix/generator-numeric-errors`。开工前核对分支路径和提交元数据：vector_mesh 自 2026-10-22 中午以来没有其他近期修改；effect_plan 与 renderer 的资源工作存在重叠，只增加错误包装和枚举，不改资源策略。mask_plan 与精度/资源分支重叠，本批不编辑它。没有审阅其他人的 PR，没有添加依赖、资产、UI 或像素算法。

七阶段完成后，将两个本人提交重放到 #422 已同步主线的 head `d6889b0b`，以 `fix/native-generator-errors` 为堆叠基底发布。870 项 Rust/Cargo/toolchain 工作文件与七阶段输入逐字节相同，继承差异只有父提交的 CI 下载源与验收文档；本段及标题仅更新发布状态。没有发布原先 GitHub 自动生成的英文 CI 合并提交。父 PR 和本批的远端 CI 均须独立成功后再按顺序合并，不把重放或本地源码一致当作远端通过。

## 真实原行为

公开 PlanBuilder 探针在两个真实路径处理拒绝中确认，底层原生 Model 错误被转成字符串：非法 trim start 和单项 dash pattern。健康几何原始 bits 在失败前后相同，输入保持。工件 `artifacts/pr-ci-followups/vector-native-pending-probe/` 保留原 stdout/stderr、源码/库/二进制 SHA。这是公开可构造的 sampled scene，不声称正常 GUI 可达。

另一个实际公开 PlanBuilder 探针分别输入 NaN 端点、Infinity 端点和 f32::MAX 与 f32::MAX 的切线相加，真实产生三次 Lyon Path builder 的断言 panic。独立探针用 catch_unwind 记录原行为；产品没有新增 panic 捕获。正常输入在每次失败后都恢复相同的几何 bits。原始工件 `vector-coordinate-pending-probe/` 的 SHA 和源码收据保持；不是用 mock 返回值代替失败。

## 逐项边界

| ID | 分类与实际变化 | 验证限制 |
| --- | --- | --- |
| V1 | 原生传播：trim/dash 保留 Model 原因；Lyon fill/stroke 的返回错误保留 TessellationError 和阶段 | 真实 trim/dash 及组内 trim 的 source 身份已通过；Lyon 返回错误包装不冒充已复现每种内部故障 |
| V2 | 功能修复：在把实际使用的端点/控制点交给 Path builder 前检查有限性，覆盖输入非有限和控制点加法溢出 | 不扫描未使用的开路径末尾手柄，不把原先跳过的不足两个节点轮廓改成故障；不添加默认点、重试或 catch_unwind |
| V3 | 原生上下文：矢量 planner 包装保留 source，携带 layer、实际 frame、vector rasterize 阶段 | 不新增热路径日志、计数器、观察键或文件 IO |
| V4 | 原有预算优先：BoundedGeometry 的检查、顶点上限、Lyon 调用、颜色/顶点运算顺序保持 | 不改变资源策略、输出上限或任何已有断言 |
| V5 | 已有独立文本边界：MaskCache 的 String 合约仍在显式 adapter 处转换 | mask_plan 没有改动，不声称所有 CPU/蒙版错误均已类型化；端点检查仍由共用实现提供 |
| V6 | 默认信道：六种真实公开失败，经 host/respond/mpsc 验证默认 Info 文件、Error stderr 和 stdout 隔离 | 手动 Flight 只证明回复边界，不冒充完整 worker 调度和受控阻塞发布 |

数值分支的五阶段流水线先正常结束，576 通过、0 失败、1 项既有忽略，FFmpeg 构建成功；确认原进程终止后才复用同一缓存执行本组合，不并发启动另一套 Cargo，不新增构建缓存。该父阶段数字不与本组合相加。

本组合两项真实公开入口专项通过（17.922 秒）；六种失败均保留输入，恢复原几何 bits 且继续复用原 Arc 几何缓存，未使用的开路径末尾手柄和跳过的退化轮廓保持正常。桌面三项信道通过（87.516 秒），包含既有缓存错误、十四种生成器拒绝及六种矢量拒绝。所有目标检查通过（112.640 秒），四项原有矢量预算测试通过（24.047 秒）。各阶段 Rust/Cargo 字节收据不变，没有更改原预算或断言。

在同一最终组合的桌面测试二进制 SHA `420a189d8dc854fb947162e70f8b26082455edfae7cef8be31c587e3df581547` 上，分别执行独立默认留样。矢量文件与 Error stderr 各六条、生成器各十四条，二十个实际原因均完整回传，request、operation、owner、阶段、实际帧、耗时匹配，stdout 隔离；包含父批八例，不能称为二十个新缺陷。源码与二进制不变，原始工件为 `cpu-geometry-vector-default-output/`、`cpu-geometry-generator-default-output/`。V6 的手动 Flight 限制仍适用。

另用实际新库重新调用旧探针的五种输入，trim/dash 现在可沿 source 找到原生 Model，三种原 panic 输入返回错误；正常几何在每次恢复后与旧库原样一致。顶点位置/颜色 bits 的原始行共 3910 字节，与两份修改前独立探针逐字节相同，SHA `b0e289ed24e965da16e29acb7c5ae53f6426d7861d50c4566d885d8ec9077c84`。工件 `vector-native-fixed/` 保留真实库、二进制及源码 SHA，不冒充全部绘制命令、完整像素基线或每种 Lyon 内部故障。

最终七阶段本地流水线均正常结束：上述专项 2、桌面信道 3、原有预算 4，加上相关渲染/GPU 61 和完整宿主 120，共 190 通过、0 失败、0 忽略；所有目标检查和带 FFmpeg 的 Debug 产品构建也成功。相关渲染 149.860 秒、宿主 240.672 秒、构建 40.016 秒，七份 Rust/Cargo 字节收据保持不变。九个相关渲染套件包含生成器、运动粒子、分组、预算、调整图层、上传复用、蒙版和计划缓存；不冒充本地重复运行全部渲染器。父阶段 576 不混入本组合 190，原始默认留样也不重复计数。

同一最终库另重跑生成器旧库对照，132 组、19460 字节仍与修改前完全相同，SHA `58aae57ae017261e63455f45d662af53bd7425571741c1e7a6b1be4ea2165cf1`；当前输入和库身份记录在 `cpu-geometry-generator-healthy/`。这是最终组合的真实调用，不仅依赖父阶段推断。

新构建产品二进制 SHA `b5fb5b2d27f2de6e70bfbf1a707eb69d4afe2173875715ef8b04e02721826cd9` 实际启动 `--command-list` 成功，stdout 为 24475 字节有效 JSON，stderr 为空，二进制未变。最初新增留样器误把既有 `--help` 的 stderr 用法说明判为失败；原产品实际退出 0，原始输出和首次 AssertionError 保留并分类为收集器假设错误，产品渠道与返回值未改。该命令启动验证不是 GUI 或视频解码验收。

远端 CI、主线组合与实际合入树尚待验证；仍保留 V1/V5/V6 和像素基线的明确范围限制，不关闭 #138/#135。
