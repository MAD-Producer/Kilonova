# 静默失败机械候选逐项复核

本表与 `silent-failure-paths-followup.md` 的 161 个辅助编号分开计数。原始 `silent-failure-paths-exhaustive.md` 列出 832 个候选；候选不是已确认缺陷。用原始路径与行号作稳定标识，行号不代表修改后的源码位置。

当前B1–B9的832个候选已全部逐行复核并唯一映射，未映射0项：核实保留 542行、功能边界 54行、新增观测 98行、已有覆盖 138行。本轮补齐B3的122项和B4的159项，分别标明新增观测、已有覆盖、正常契约和功能边界；54行功能边界保留，不将机械复核完成等同所有功能缺陷修复。原始4443行中的no-loss/test-only/guarded等非候选不计入832。本轮实际验证与限制见完成记录§68–70

## B1 · GPU 协议、资源池与提交台账（首批 77 项）

本批复核 B1 的 14 个文件、77 行；不包含 effect_plan 的 104 行或 renderer 的 73 行，不将跨文件边界说明视为那些行已全部复核。处置为新增观测 3、已有覆盖 2、核实保留 69、功能边界 3

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-render/src/compositing_gpu.rs:425 | 核实保留 | blend_offset 唯一调用方使用当前 draw 的 i，encode_local 先拒绝超过 MAX_LAYERS=128 的 scene；同 stride×MAX_LAYERS 的 uniform buffer 已由 device 建立。该内部偏移来自已分配 buffer 的元素范围，不将任意外部 layer/异常设备 limits 算作已验证 |
| crates/motion-render/src/composition_plan.rs:81 | 核实保留 | root 的 usize::MAX parent 写为 u32::MAX 是无父节点哨兵；其余 parent 为最多 64 个活跃节点的后序索引。§28 已核实，checked conversion 会错误拒绝合法 root |
| crates/motion-render/src/composition_plan.rs:82 | 核实保留 | slot 以 -(order+1) 作为有符号纹理 token，再按 u32 保存原位型；有效模型 MAX_LAYERS=128，不是丢失负值。§28 的后序/绑定/像素回归 |
| crates/motion-render/src/composition_plan.rs:88 | 核实保留 | 写入前限制整个 bundle 为 MAX_BYTES=32 MiB，entries 限深度及 MAX_RENDER_COMPOSITION_INSTANCES=64；节点/视频记录固定 80/40 字节，因此正常 start/size/end/count/root/header 值可用 u32 表示。§28，不为任意伪造 Scene 背书 |
| crates/motion-render/src/composition_plan.rs:89 | 核实保留 | 写入前限制整个 bundle 为 MAX_BYTES=32 MiB，entries 限深度及 MAX_RENDER_COMPOSITION_INSTANCES=64；节点/视频记录固定 80/40 字节，因此正常 start/size/end/count/root/header 值可用 u32 表示。§28，不为任意伪造 Scene 背书 |
| crates/motion-render/src/composition_plan.rs:90 | 核实保留 | 写入前限制整个 bundle 为 MAX_BYTES=32 MiB，entries 限深度及 MAX_RENDER_COMPOSITION_INSTANCES=64；节点/视频记录固定 80/40 字节，因此正常 start/size/end/count/root/header 值可用 u32 表示。§28，不为任意伪造 Scene 背书 |
| crates/motion-render/src/composition_plan.rs:91 | 核实保留 | 写入前限制整个 bundle 为 MAX_BYTES=32 MiB，entries 限深度及 MAX_RENDER_COMPOSITION_INSTANCES=64；节点/视频记录固定 80/40 字节，因此正常 start/size/end/count/root/header 值可用 u32 表示。§28，不为任意伪造 Scene 背书 |
| crates/motion-render/src/composition_plan.rs:104 | 核实保留 | slot 以 -(order+1) 作为有符号纹理 token，再按 u32 保存原位型；有效模型 MAX_LAYERS=128，不是丢失负值。§28 的后序/绑定/像素回归 |
| crates/motion-render/src/composition_plan.rs:111 | 核实保留 | 写入前限制整个 bundle 为 MAX_BYTES=32 MiB，entries 限深度及 MAX_RENDER_COMPOSITION_INSTANCES=64；节点/视频记录固定 80/40 字节，因此正常 start/size/end/count/root/header 值可用 u32 表示。§28，不为任意伪造 Scene 背书 |
| crates/motion-render/src/composition_plan.rs:113 | 核实保留 | 写入前限制整个 bundle 为 MAX_BYTES=32 MiB，entries 限深度及 MAX_RENDER_COMPOSITION_INSTANCES=64；节点/视频记录固定 80/40 字节，因此正常 start/size/end/count/root/header 值可用 u32 表示。§28，不为任意伪造 Scene 背书 |
| crates/motion-render/src/composition_plan.rs:114 | 核实保留 | 写入前限制整个 bundle 为 MAX_BYTES=32 MiB，entries 限深度及 MAX_RENDER_COMPOSITION_INSTANCES=64；节点/视频记录固定 80/40 字节，因此正常 start/size/end/count/root/header 值可用 u32 表示。§28，不为任意伪造 Scene 背书 |
| crates/motion-render/src/compositor.rs:349 | 核实保留 | emit 在 append 前检查 self.vertices.len()+三角展开数量<=MAX_OUTPUT=65536，故 start/end 两个 u32 收窄均在界内；不是拿 Vec capacity 代替输出预算 |
| crates/motion-render/src/compositor.rs:359 | 核实保留 | emit 在 append 前检查 self.vertices.len()+三角展开数量<=MAX_OUTPUT=65536，故 start/end 两个 u32 收窄均在界内；不是拿 Vec capacity 代替输出预算 |
| crates/motion-render/src/effect_gpu.rs:826 | 核实保留 | 正规 PlannedDraw.words[24] 来自 assets 槽位索引，模型合计资产上限 256，f32 精确表示；负值先分流 Video，Vector/EffectInput 另分支，随后资产 get 仍受检。人工篡改公有 frame 的 NaN/小数不在本项有效构建路径证明内 |
| crates/motion-render/src/layer_gpu.rs:393 | 核实保留 | 正规 mesh 输出来自共享 BoundedGeometry，缓存命中也通过 frame checked_sub 记入 MAX_TRIANGLE_VERTICES=262144；u32 draw count 不丢合法范围，缓存预算回归在 render 全套中验证 |
| crates/motion-render/src/mask_gpu.rs:380 | 核实保留 | MaskCache 共用 MAX_TRIANGLE_VERTICES=262144 并对缓存命中 checked_sub 后才形成 frame，draw 的 m.vertices.len 可用 u32 表示；公开手工构造 raster 另属调用边界 |
| crates/motion-render/src/mask_plan.rs:160 | 功能边界 | 正规模型 size 有限且>0、<=32768，PlanBuilder 的 scale 上界 1，产生 raster 后仍与 device_dimension 比较，超限明确 Err；公开 MaskCache::build 可接手工 Scene/NaN scale，max(1) 对这类输入的退化仍保留，未新增其完整有限性校验或专用观测，不算全输入已关闭 |
| crates/motion-render/src/mask_plan.rs:161 | 功能边界 | 正规模型 size 有限且>0、<=32768，PlanBuilder 的 scale 上界 1，产生 raster 后仍与 device_dimension 比较，超限明确 Err；公开 MaskCache::build 可接手工 Scene/NaN scale，max(1) 对这类输入的退化仍保留，未新增其完整有限性校验或专用观测，不算全输入已关闭 |
| crates/motion-render/src/mask_plan.rs:240 | 核实保留 | 正规 m.layer 是至多 MAX_LAYERS=128 的 scene.layers 枚举位置；record 的 u32 编码不损失该索引 |
| crates/motion-render/src/mask_plan.rs:241 | 核实保留 | id 与 fingerprint 同时写低 32 位和下一字的高 32 位，是完整 u64 拆分编码；原报告仅看低半行，未丢高位 |
| crates/motion-render/src/mask_plan.rs:245 | 功能边界 | composition bundle 路径在 write 前有 32 MiB 上界，正常 vertex cursor 可用 u32；独立公开 mask_plan::write 接受任意 records/vertices/out，未增加对>u32::MAX 偏移或伪造 raster 的完整校验，保留此协议入口边界 |
| crates/motion-render/src/mask_plan.rs:246 | 核实保留 | MaskCache 将所有 raster 的实际三角顶点总量限制为 262144，含缓存命中；正规记录的每个 vertices.len 在 u32 内 |
| crates/motion-render/src/mask_plan.rs:247 | 核实保留 | id 与 fingerprint 同时写低 32 位和下一字的高 32 位，是完整 u64 拆分编码；原报告仅看低半行，未丢高位 |
| crates/motion-render/src/quality.rs:88 | 核实保留 | 对正 u32 合成尺寸，f64 scale=min(surface/width,surface/height,1)，乘积不超过原尺寸且 f64 精确表示 u32；ceil/max(1) 保留最小像素，High/零 surface 走原尺寸策略，不是无界浮点收窄 |
| crates/motion-render/src/quality.rs:89 | 核实保留 | 对正 u32 合成尺寸，f64 scale=min(surface/width,surface/height,1)，乘积不超过原尺寸且 f64 精确表示 u32；ceil/max(1) 保留最小像素，High/零 surface 走原尺寸策略，不是无界浮点收窄 |
| crates/motion-render/src/submission_ledger.rs:66 | 新增观测 | submission.begin 的真实中毒仍 into_inner，固定进程累计首次 Warn/第 60 次 Error；在 ledger guard 之前声明恢复证据，释放 guard 后记录 submission、前置 submitted/completed/pending/known bytes 和恢复耗时，健康路径无日志；阈值/解锁/unwind 与真实 GPU completion 回归，§63 |
| crates/motion-render/src/submission_ledger.rs:94 | 新增观测 | submission.complete 的真实中毒仍 into_inner，固定进程累计首次 Warn/第 60 次 Error；在 ledger guard 之前声明恢复证据，释放 guard 后记录 submission、前置 submitted/completed/pending/known bytes 和恢复耗时，健康路径无日志；阈值/解锁/unwind 与真实 GPU completion 回归，§63 |
| crates/motion-render/src/submission_ledger.rs:137 | 新增观测 | submission.info 的真实中毒仍 into_inner，固定进程累计首次 Warn/第 60 次 Error；在 ledger guard 之前声明恢复证据，释放 guard 后记录 submission、前置 submitted/completed/pending/known bytes 和恢复耗时，健康路径无日志；阈值/解锁/unwind 与真实 GPU completion 回归，§63 |
| crates/motion-render/src/compositing_gpu.rs:113 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/compositing_gpu.rs:126 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/compositing_gpu.rs:140 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/compositing_gpu.rs:210 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/compositing_gpu.rs:222 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/compositing_gpu.rs:298 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/compositing_gpu.rs:300 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/compositing_gpu.rs:400 | 核实保留 | 目标尺寸进入 GPU f32 uniform/region，device 纹理尺寸和 source budget 已约束；这是 GPU 坐标接口的既有精度，未发现该转换引入无界尺寸或静默错误 |
| crates/motion-render/src/compositing_gpu.rs:402 | 核实保留 | 目标尺寸进入 GPU f32 uniform/region，device 纹理尺寸和 source budget 已约束；这是 GPU 坐标接口的既有精度，未发现该转换引入无界尺寸或静默错误 |
| crates/motion-render/src/compositing_gpu.rs:412 | 核实保留 | LayerBlend.mode.code 为有限内建枚举编号，写入 GPU f32 参数可精确表示，不是任意 u64 身份标识 |
| crates/motion-render/src/compositing_gpu.rs:419 | 核实保留 | 支持的 32/64 位 usize 到 u64 是拓宽/等宽；uniform 字节数、buffer offset、matte 数量不因该转换丢位。总分配另由 source budget/device 校验，不能由未标 no-loss 推断截断 |
| crates/motion-render/src/effect_gpu.rs:177 | 核实保留 | uniform/sprite/LUT/buffer 的 usize 字节数到 u64 在支持目标为等宽/拓宽；MAX_SPRITES=65536 且其他资源另有预算，不将潜在乘法/分配问题混同为该 cast 丢位 |
| crates/motion-render/src/effect_gpu.rs:245 | 核实保留 | uniform/sprite/LUT/buffer 的 usize 字节数到 u64 在支持目标为等宽/拓宽；MAX_SPRITES=65536 且其他资源另有预算，不将潜在乘法/分配问题混同为该 cast 丢位 |
| crates/motion-render/src/effect_gpu.rs:314 | 核实保留 | uniform/sprite/LUT/buffer 的 usize 字节数到 u64 在支持目标为等宽/拓宽；MAX_SPRITES=65536 且其他资源另有预算，不将潜在乘法/分配问题混同为该 cast 丢位 |
| crates/motion-render/src/effect_gpu.rs:319 | 核实保留 | uniform/sprite/LUT/buffer 的 usize 字节数到 u64 在支持目标为等宽/拓宽；MAX_SPRITES=65536 且其他资源另有预算，不将潜在乘法/分配问题混同为该 cast 丢位 |
| crates/motion-render/src/effect_gpu.rs:357 | 核实保留 | scratch_idle_frames 饱和后仍满足 >=SCRATCH_IDLE_FRAME_LIMIT 并释放闲置池，不影响失效阈值或显存记账；这是已达到终态的闲置计数 |
| crates/motion-render/src/effect_gpu.rs:448 | 核实保留 | uniform/sprite/LUT/buffer 的 usize 字节数到 u64 在支持目标为等宽/拓宽；MAX_SPRITES=65536 且其他资源另有预算，不将潜在乘法/分配问题混同为该 cast 丢位 |
| crates/motion-render/src/effect_gpu.rs:888 | 核实保留 | 实际 EffectPass 的 width/height 经 plan/device 纹理尺寸校验，set_viewport 采用 GPU f32 坐标接口；不引入原生整数 token 转换 |
| crates/motion-render/src/layer_gpu.rs:181 | 核实保留 | prepare_accumulators 的通配只处理 None/相同 Some(size) 的无变化状态；None 和尺寸变化已有显式分支，不是遗漏 enum 变体 |
| crates/motion-render/src/layer_gpu.rs:199 | 核实保留 | accumulators.len 到 u64 为等宽/拓宽；该池按 None 或两个累加纹理建立，字节计算不是从大整数身份标识转小整数 |
| crates/motion-render/src/layer_gpu.rs:207 | 核实保留 | accumulators.len 到 u64 为等宽/拓宽；该池按 None 或两个累加纹理建立，字节计算不是从大整数身份标识转小整数 |
| crates/motion-render/src/layer_gpu.rs:259 | 已有覆盖 | 删除 vector texture 的 saturating_sub 保留；renderer 的 trim 检查 booked 与 images 实测总字节，首次及每 60 次 Warn、健康复位。覆盖共享记账漂移；不声称此减法内部新增即时日志，§01/renderer.check_texture_accounting |
| crates/motion-render/src/mask_gpu.rs:181 | 核实保留 | scratch.len 到 u64 为等宽/拓宽；scratch_dimensions 的 slots 为 1+blur+(group.len()-1).min(2)，最多四槽，非截断 |
| crates/motion-render/src/mask_gpu.rs:287 | 已有覆盖 | 删除 mask texture 的 saturating_sub 保留；共享 texture_bytes 由 renderer.trim_image_cache 的实测总量检查覆盖，首次/每 60 次 Warn，健康复位。不是该减法内部即时日志，未修复记账根因 |
| crates/motion-render/src/mask_gpu.rs:377 | 核实保留 | m.width/height 在正规 PlanBuilder 中先与 device_dimension 比较，超限产生带 layer/mask 的 Err；后续 viewport 使用既有 GPU f32 接口 |
| crates/motion-render/src/mask_plan.rs:202 | 核实保留 | width/height 用于 GPU f32 uniform/texel 步长，正规 raster 已通过 PlanBuilder.device_dimension 与纹理预算；这是既有像素坐标接口精度 |
| crates/motion-render/src/mask_plan.rs:203 | 核实保留 | width/height 用于 GPU f32 uniform/texel 步长，正规 raster 已通过 PlanBuilder.device_dimension 与纹理预算；这是既有像素坐标接口精度 |
| crates/motion-render/src/mask_plan.rs:204 | 核实保留 | width/height 用于 GPU f32 uniform/texel 步长，正规 raster 已通过 PlanBuilder.device_dimension 与纹理预算；这是既有像素坐标接口精度 |
| crates/motion-render/src/mask_plan.rs:205 | 核实保留 | width/height 用于 GPU f32 uniform/texel 步长，正规 raster 已通过 PlanBuilder.device_dimension 与纹理预算；这是既有像素坐标接口精度 |
| crates/motion-render/src/mask_plan.rs:217 | 核实保留 | width/height 用于 GPU f32 uniform/texel 步长，正规 raster 已通过 PlanBuilder.device_dimension 与纹理预算；这是既有像素坐标接口精度 |
| crates/motion-render/src/mask_plan.rs:219 | 核实保留 | width/height 用于 GPU f32 uniform/texel 步长，正规 raster 已通过 PlanBuilder.device_dimension 与纹理预算；这是既有像素坐标接口精度 |
| crates/motion-render/src/mask_plan.rs:229 | 核实保留 | mode 为已验证 MaskMode 的有限编号，inverted 只有 0/1，f32 参数精确表示；未将任意 mask id 转浮点 |
| crates/motion-render/src/mask_plan.rs:230 | 核实保留 | mode 为已验证 MaskMode 的有限编号，inverted 只有 0/1，f32 参数精确表示；未将任意 mask id 转浮点 |
| crates/motion-render/src/measurement.rs:38 | 核实保留 | Instant.elapsed.as_micros 是 u128，此处实际收窄为 u64，原报告的 widened 标签不准确；需连续约 58 万年才超 u64 微秒，不作为现实挂起或诊断空洞修复，不改时钟契约 |
| crates/motion-render/src/measurement.rs:81 | 核实保留 | distribution 先滤非有限值，空集合返回 JSON null；非空时 p50/p95/p99 的 ceil 索引再减一并 min(len-1)，saturating_sub 是安全零基索引处理 |
| crates/motion-render/src/preview_input.rs:199 | 核实保留 | RGBA pixels.len 与已验证 YUV frame.bytes 的 usize 到 u64 是等宽/拓宽；像素数量有 exact length/device dimensions/source budget 检查，非数据丢位 |
| crates/motion-render/src/preview_input.rs:285 | 核实保留 | RGBA pixels.len 与已验证 YUV frame.bytes 的 usize 到 u64 是等宽/拓宽；像素数量有 exact length/device dimensions/source budget 检查，非数据丢位 |
| crates/motion-render/src/preview_memory.rs:81 | 核实保留 | failures 明确饱和并 min(PREVIEW_MEMORY_LEVELS=4) 表示有限降级等级，拒绝次数另存 refusal_count；达到最小质量后 blocked，不吞掉下一次错误 |
| crates/motion-render/src/preview_memory.rs:83 | 核实保留 | now_ms+30s 冷却截止点饱和取最晚时间是保守禁止过早重试，不会放宽分配预算；现有 preview_memory 回归覆盖冷却和降级恢复 |
| crates/motion-render/src/preview_memory.rs:100 | 核实保留 | successes 饱和后仍满足 >=120 的恢复探测门槛，成功复位 failures/recovery，begin/set policy 再更新；不因计数停在 u32::MAX 丢失可执行恢复 |
| crates/motion-render/src/quality.rs:155 | 核实保留 | surface_wait_us 减去一帧预算时钳零是有意忽略普通 FIFO/vsync 等待；仅超额等待加入 CPU 压力，saturating_add 取最保守高负载，不是显存或错误次数记账 |
| crates/motion-render/src/quality.rs:180 | 核实保留 | width/height 转 f32 参与 GPU 变换的屏幕密度估算；矩阵及计算结果非有限或非正时保留原 scale，其余仅选择既有分辨率桶，不改变模型坐标或输出身份标识 |
| crates/motion-render/src/timing.rs:58 | 核实保留 | 固定四个 timestamp slot、每槽四个 u64，resolve offset=slot×QUERY_RESOLVE_BUFFER_ALIGNMENT；usize/常量到 u64 为等宽/拓宽，slot 由 begin 有界产生。32 字节读取已有受检拒绝短 buffer，未把该安全 cast 计为错误 |
| crates/motion-render/src/timing.rs:65 | 核实保留 | 固定四个 timestamp slot、每槽四个 u64，resolve offset=slot×QUERY_RESOLVE_BUFFER_ALIGNMENT；usize/常量到 u64 为等宽/拓宽，slot 由 begin 有界产生。32 字节读取已有受检拒绝短 buffer，未把该安全 cast 计为错误 |
| crates/motion-render/src/timing.rs:102 | 核实保留 | 固定四个 timestamp slot、每槽四个 u64，resolve offset=slot×QUERY_RESOLVE_BUFFER_ALIGNMENT；usize/常量到 u64 为等宽/拓宽，slot 由 begin 有界产生。32 字节读取已有受检拒绝短 buffer，未把该安全 cast 计为错误 |
| crates/motion-render/src/video_gpu.rs:137 | 核实保留 | 已验证 Yuv420Frame.bytes 的 usize 到 u64 在 32/64 位目标不丢位，replacement_bytes 基于同一 inputs 集合扣除当前项并加入新帧；不把此 cast 当作账面饱和 |
| crates/motion-render/src/composition_plan.rs:30 | 核实保留 | 有效模型 layer.order 在 MAX_LAYERS=128 内，-(order+1) 的 i32 纹理 token 不溢出；sampled reference 的 expect 是另一个不变量边界，本项不声称消除任意手工嵌套 Scene 的 panic |
| crates/motion-render/src/effect_gpu.rs:208 | 核实保留 | identity_bytes 固定为 1024 字节，from_fn 的 i 为 0..1023，i/4 为 0..255，转 u8 恰好生成 256 级恒等 LUT |
| crates/motion-render/src/effect_gpu.rs:422 | 核实保留 | 正规曲线 LUT 有限分量先 clamp(0,1)、乘 255 并 round，再量化 u8，是 GPU 颜色编码契约；不将独立伪造的 NaN Scene LUT 当作已通过模型校验 |

三个 submission 站点按进程生命周期累计，健康 ledger、开关 tracking 和 clear_poison 都不复位；仅进程重启清零。日志保留恢复前的数值快照，不能作为半更新台账已经修复或统计可信的保证。独立 mask API 的三行功能边界仍明确保留；不称所有 NaN/任意 offset 入口已校验。验证见完成记录 §63。

## B1 · renderer 与 effect_plan（续审 181 项）

逐项关联原始 renderer 73 行、renderer/compositing 4 行和 effect_plan 104 行。处置：{"新增观测": 3, "功能边界": 24, "核实保留": 146, "已有覆盖": 8}。公开 writer、手工 Scene 和长期 program 缓存的边界仍保留；功能边界不算缺陷已修复。验证见完成记录 §64

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-render/src/effect_plan.rs:216 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:217 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:218 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:219 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:220 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:221 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:225 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:226 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:227 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:228 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:229 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:230 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:231 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:233 | 功能边界 | 正规 scratch_budget setter 限制在 host floor..384 MiB，可用 u32；公开 EffectFramePlan.scratch_budget 是 u64，write 仍只写低 u32，未检查调用者直接构造的大预算。保留独立 ABI 边界，不在仅补观测时修改编码格式 |
| crates/motion-render/src/effect_plan.rs:234 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:235 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:236 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:237 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:242 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:243 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:244 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:246 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:256 | 核实保留 | input/source/resource_input/matte 的负值编码资产/无 matte 哨兵，写入 u32 保存相同 32 位位型；消费侧按有符号 token 解释。不能用 try_from 拒绝合法负 token，§28 |
| crates/motion-render/src/effect_plan.rs:257 | 核实保留 | input/source/resource_input/matte 的负值编码资产/无 matte 哨兵，写入 u32 保存相同 32 位位型；消费侧按有符号 token 解释。不能用 try_from 拒绝合法负 token，§28 |
| crates/motion-render/src/effect_plan.rs:261 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:265 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:267 | 核实保留 | input/source/resource_input/matte 的负值编码资产/无 matte 哨兵，写入 u32 保存相同 32 位位型；消费侧按有符号 token 解释。不能用 try_from 拒绝合法负 token，§28 |
| crates/motion-render/src/effect_plan.rs:287 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:309 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:312 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:313 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:314 | 核实保留 | fingerprint 同时写低 32 位和下一字的高 32 位，完整保留 u64；不是高位丢失，§28 |
| crates/motion-render/src/effect_plan.rs:316 | 功能边界 | composition_plan::build 的外层 bundle 限制 32 MiB，可保证正规合成包这些偏移可用 u32；但公开 EffectFramePlan::write 自身只检查 out.len>=buffer_bytes，不检查 size/任意外部 offset/lut 是否能用 u32。保留独立 writer 功能/协议边界，不拿外层限额替所有调用方关闭问题 |
| crates/motion-render/src/effect_plan.rs:317 | 核实保留 | 正规 PlanBuilder 的层数≤128、每层效果≤16、每效果 pass≤8；sprites≤65536、平面顶点≤65536、vector/mask 顶点≤262144，曲线/批次/命令亦来自模型与几何预算，记录数量和 layer 索引可用 u32。不是声称公开可变 EffectFramePlan 能拒绝任意外部伪造值；其 writer 边界单列 |
| crates/motion-render/src/effect_plan.rs:330 | 核实保留 | input/source/resource_input/matte 的负值编码资产/无 matte 哨兵，写入 u32 保存相同 32 位位型；消费侧按有符号 token 解释。不能用 try_from 拒绝合法负 token，§28 |
| crates/motion-render/src/effect_plan.rs:729 | 功能边界 | programs 是跨 frame 保留的编译缓存，set_registry 会 truncate(3)，但同步新增 key 时没有独立 u32 数量上限；不能用单帧层数/pass 预算保证该长期索引。未制造数十亿 shader 耗尽内存，未声称此功能边界已修复 |
| crates/motion-render/src/effect_plan.rs:970 | 功能边界 | 正规 sampled layer.size 有模型约束，cast 后有 device_dimension 拒绝；独立 Scene/Layer 的非有限或负尺寸在该转 u32 前没有统一校验，零/NaN 不必触发 >device_dimension。保留手工 Scene 数值边界，不以几何预算代替 finite 校验 |
| crates/motion-render/src/effect_plan.rs:971 | 功能边界 | 正规 sampled layer.size 有模型约束，cast 后有 device_dimension 拒绝；独立 Scene/Layer 的非有限或负尺寸在该转 u32 前没有统一校验，零/NaN 不必触发 >device_dimension。保留手工 Scene 数值边界，不以几何预算代替 finite 校验 |
| crates/motion-render/src/effect_plan.rs:1028 | 功能边界 | mask source/post-effect snapshot 对 region×scale 使用 ceil().max(1)；正规输入为有限模型矩形，但独立 Scene 的 NaN/非正 source_rect 可被钳成 1。设备/字节预算不等于原始数值校验，保留此独立调用边界，没有改采样或返回值 |
| crates/motion-render/src/effect_plan.rs:1109 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1111 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1118 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1119 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1147 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1148 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1171 | 核实保留 | scene_generator::generate 在 append 前限制总 sprites≤65536，start 和本次新增数量均不超过该界；转换到 u32 不丢位，既有 generator/particle 真渲染回归 |
| crates/motion-render/src/effect_plan.rs:1172 | 核实保留 | scene_generator::generate 在 append 前限制总 sprites≤65536，start 和本次新增数量均不超过该界；转换到 u32 不丢位，既有 generator/particle 真渲染回归 |
| crates/motion-render/src/effect_plan.rs:1182 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1183 | 核实保留 | generator 使用正规非零 scene 合成尺寸，scale≤1；reserve_scratch 之后在 generate/提交 pass 之前 check_scratch 校验每槽 device_dimension 与总容量。保留相同取整，不为独立零尺寸 Scene 作全输入保证 |
| crates/motion-render/src/effect_plan.rs:1236 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1237 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1242 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1243 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1248 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1249 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1255 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1256 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1323 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1324 | 核实保留 | next 的 output_bounds/padding 先经 BoundsExpr::eval 每节点 finite 且 abs≤32768 校验，OutputBounds 保证宽高≥1；pad 另拒绝负值，正规 region 来自模型。各槽 reserve 后统一 check_scratch 尺寸/容量，再把同一已取整 u.size 写入 pass，不是无条件接受超限效果 |
| crates/motion-render/src/effect_plan.rs:1362 | 核实保留 | shader manifest 的 MAX_PASSES=8，i 为本效果 pass 枚举索引；i as u32 %2 仅选择两个 ping-pong scratch slot，不是无界累计 program 缓存 id |
| crates/motion-render/src/effect_plan.rs:1417 | 功能边界 | mask source/post-effect snapshot 对 region×scale 使用 ceil().max(1)；正规输入为有限模型矩形，但独立 Scene 的 NaN/非正 source_rect 可被钳成 1。设备/字节预算不等于原始数值校验，保留此独立调用边界，没有改采样或返回值 |
| crates/motion-render/src/effect_plan.rs:1540 | 核实保留 | 正规非零 Scene 合成宽高≤32768，scale≤1；计算两个 RGBA accumulator 字节后与 source_texture_budget 明确比较并拒绝。保留预算行为，不将该容量估算当作任意手工 Scene 的完整验证 |
| crates/motion-render/src/effect_plan.rs:1541 | 核实保留 | 正规非零 Scene 合成宽高≤32768，scale≤1；计算两个 RGBA accumulator 字节后与 source_texture_budget 明确比较并拒绝。保留预算行为，不将该容量估算当作任意手工 Scene 的完整验证 |
| crates/motion-render/src/renderer.rs:104 | 功能边界 | 正规 Project 合成尺寸非零，嵌套结果还有 source budget/render_target 尺寸拒绝；公开 encode 接受直接构造的 Scene，nested_size 自身未校验零父尺寸/异常缩放，不将任意 Scene 的浮点转 u32 或后续字节乘法算作全输入安全。保留功能边界，没有改返回值 |
| crates/motion-render/src/renderer.rs:1530 | 核实保留 | encode_local 先拒绝超过 MAX_LAYERS=128 的 scene，batch.layer/i 是当前 draw 索引；同一个对齐 uniform_stride×MAX_LAYERS buffer 已由 device 创建。正常动态偏移在实际 buffer 范围内，不将任意伪造 frame 当作已验证 |
| crates/motion-render/src/renderer.rs:1538 | 核实保留 | 正规层数 128、每层效果 16、每效果 shader pass 8、平面展开顶点 65536，matte 只有 alpha/luma 两类；批次与已执行 pass 计数受这些小于 u32 上限的预算约束。统计 cast 不涉及资产 id；不为独立伪造超预算 Scene/frame 保证 |
| crates/motion-render/src/renderer.rs:1672 | 核实保留 | 正规 Scene 尺寸为正且模型有限，render_scale 取 target/scene 比、min(1)、max(0.001) 后取整形成 GPU accumulator 尺寸；原计划与资源预算门仍保留。只核实正常模型与真实 GPU target，不声称独立 NaN/零尺寸 Scene 全校验 |
| crates/motion-render/src/renderer.rs:1673 | 核实保留 | 正规 Scene 尺寸为正且模型有限，render_scale 取 target/scene 比、min(1)、max(0.001) 后取整形成 GPU accumulator 尺寸；原计划与资源预算门仍保留。只核实正常模型与真实 GPU target，不声称独立 NaN/零尺寸 Scene 全校验 |
| crates/motion-render/src/renderer.rs:1864 | 核实保留 | encode_local 先拒绝超过 MAX_LAYERS=128 的 scene，batch.layer/i 是当前 draw 索引；同一个对齐 uniform_stride×MAX_LAYERS buffer 已由 device 创建。正常动态偏移在实际 buffer 范围内，不将任意伪造 frame 当作已验证 |
| crates/motion-render/src/renderer.rs:1947 | 核实保留 | encode_local 先拒绝超过 MAX_LAYERS=128 的 scene，batch.layer/i 是当前 draw 索引；同一个对齐 uniform_stride×MAX_LAYERS buffer 已由 device 创建。正常动态偏移在实际 buffer 范围内，不将任意伪造 frame 当作已验证 |
| crates/motion-render/src/renderer.rs:2014 | 核实保留 | 正规层数 128、每层效果 16、每效果 shader pass 8、平面展开顶点 65536，matte 只有 alpha/luma 两类；批次与已执行 pass 计数受这些小于 u32 上限的预算约束。统计 cast 不涉及资产 id；不为独立伪造超预算 Scene/frame 保证 |
| crates/motion-render/src/renderer/compositing.rs:96 | 核实保留 | extract 的 index 是 PlanBuilder 的 draw 索引，encode_local 层数受限 128；extract buffer 以相同 extract_stride×MAX_LAYERS 建立。u32 偏移来自已分配 uniform buffer 的元素范围，§63 同族 |
| crates/motion-render/src/effect_plan.rs:437 | 核实保留 | alpha_images pixels.len/rgba.len 的 usize→u64 在 32/64 位目标等宽/拓宽；set_alpha 检查非零尺寸、精确 rgba 长度及 32 MiB alpha 总缓存预算，不从转换本身丢失字节 |
| crates/motion-render/src/effect_plan.rs:441 | 核实保留 | alpha_images pixels.len/rgba.len 的 usize→u64 在 32/64 位目标等宽/拓宽；set_alpha 检查非零尺寸、精确 rgba 长度及 32 MiB alpha 总缓存预算，不从转换本身丢失字节 |
| crates/motion-render/src/effect_plan.rs:445 | 核实保留 | alpha_images pixels.len/rgba.len 的 usize→u64 在 32/64 位目标等宽/拓宽；set_alpha 检查非零尺寸、精确 rgba 长度及 32 MiB alpha 总缓存预算，不从转换本身丢失字节 |
| crates/motion-render/src/effect_plan.rs:822 | 核实保留 | shader clock 的秒/frame/seed 是既有 EffectUniform f32 ABI；局部时间仍以 f64 采样，主 timeline 仍为 f64。大时间/u32 seed 可能损失低位，不承诺全部种子一一映射，改变时钟/随机映射需独立渲染协议方案，不能当作资源 id 损坏修复 |
| crates/motion-render/src/effect_plan.rs:823 | 核实保留 | shader clock 的秒/frame/seed 是既有 EffectUniform f32 ABI；局部时间仍以 f64 采样，主 timeline 仍为 f64。大时间/u32 seed 可能损失低位，不承诺全部种子一一映射，改变时钟/随机映射需独立渲染协议方案，不能当作资源 id 损坏修复 |
| crates/motion-render/src/effect_plan.rs:928 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:929 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1012 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1031 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1137 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1162 | 核实保留 | shader clock 的秒/frame/seed 是既有 EffectUniform f32 ABI；局部时间仍以 f64 采样，主 timeline 仍为 f64。大时间/u32 seed 可能损失低位，不承诺全部种子一一映射，改变时钟/随机映射需独立渲染协议方案，不能当作资源 id 损坏修复 |
| crates/motion-render/src/effect_plan.rs:1164 | 核实保留 | shader clock 的秒/frame/seed 是既有 EffectUniform f32 ABI；局部时间仍以 f64 采样，主 timeline 仍为 f64。大时间/u32 seed 可能损失低位，不承诺全部种子一一映射，改变时钟/随机映射需独立渲染协议方案，不能当作资源 id 损坏修复 |
| crates/motion-render/src/effect_plan.rs:1300 | 核实保留 | shader clock 的秒/frame/seed 是既有 EffectUniform f32 ABI；局部时间仍以 f64 采样，主 timeline 仍为 f64。大时间/u32 seed 可能损失低位，不承诺全部种子一一映射，改变时钟/随机映射需独立渲染协议方案，不能当作资源 id 损坏修复 |
| crates/motion-render/src/effect_plan.rs:1301 | 核实保留 | shader clock 的秒/frame/seed 是既有 EffectUniform f32 ABI；局部时间仍以 f64 采样，主 timeline 仍为 f64。大时间/u32 seed 可能损失低位，不承诺全部种子一一映射，改变时钟/随机映射需独立渲染协议方案，不能当作资源 id 损坏修复 |
| crates/motion-render/src/effect_plan.rs:1303 | 核实保留 | shader clock 的秒/frame/seed 是既有 EffectUniform f32 ABI；局部时间仍以 f64 采样，主 timeline 仍为 f64。大时间/u32 seed 可能损失低位，不承诺全部种子一一映射，改变时钟/随机映射需独立渲染协议方案，不能当作资源 id 损坏修复 |
| crates/motion-render/src/effect_plan.rs:1364 | 核实保留 | i 是单效果最多 8 个 manifest pass 的下标，转 f32 作为 shader pass 参数可精确表示，非长期 program id |
| crates/motion-render/src/effect_plan.rs:1420 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1448 | 核实保留 | words[24] 的 layer.order≤127 或 assets 槽位≤256，f32 精确表示；视频/嵌套资产采用 -(order+1) token，与 renderer 既有纹理查找契约一致 |
| crates/motion-render/src/effect_plan.rs:1450 | 核实保留 | words[24] 的 layer.order≤127 或 assets 槽位≤256，f32 精确表示；视频/嵌套资产采用 -(order+1) token，与 renderer 既有纹理查找契约一致 |
| crates/motion-render/src/effect_plan.rs:1455 | 核实保留 | words[28:30] 保存当前帧 pass 范围；正规 128 层×16 效果×最多 8 shader pass 加转换/掩码，远小于 f32 精确整数上限 2^24。宿主 PlannedDraw 同时保留 usize pass_start/pass_end，不是把资源 id 转 float |
| crates/motion-render/src/effect_plan.rs:1456 | 核实保留 | words[28:30] 保存当前帧 pass 范围；正规 128 层×16 效果×最多 8 shader pass 加转换/掩码，远小于 f32 精确整数上限 2^24。宿主 PlannedDraw 同时保留 usize pass_start/pass_end，不是把资源 id 转 float |
| crates/motion-render/src/effect_plan.rs:1476 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1489 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1490 | 核实保留 | GPU f32 uniform 中保存正规合成/源尺寸、已检查 scratch 尺寸或 UV 比例；尺寸不是 u64 对象 id，0.001..1 分辨率桶/像素取整及 GPU 坐标 ABI 保留。手工非有限 source 的边界在 :970/:1028/:1417 单列 |
| crates/motion-render/src/effect_plan.rs:1551 | 核实保留 | scratch_dimensions 最多 4 个 mask 槽，usize→u64 是等宽/拓宽；mask outputs 与 mask+effect scratch 分别有 source/scratch 预算拒绝 |
| crates/motion-render/src/renderer.rs:18 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:230 | 新增观测 | GPU latch 的 uncaptured/device_lost/read 分别固定进程累计首次 Warn、第 60 次 Error，释放 guard 后输出恢复前 cause 是否存在及字节数、可用 renderer id 和作用域耗时；不复制原因、不清除 poison、不改变 None/首因锁存/设备丢失覆盖策略。真实中毒、公开 gpu_error/check_health、阈值、清除再中毒和 unwind 回归，§64 |
| crates/motion-render/src/renderer.rs:238 | 新增观测 | GPU latch 的 uncaptured/device_lost/read 分别固定进程累计首次 Warn、第 60 次 Error，释放 guard 后输出恢复前 cause 是否存在及字节数、可用 renderer id 和作用域耗时；不复制原因、不清除 poison、不改变 None/首因锁存/设备丢失覆盖策略。真实中毒、公开 gpu_error/check_health、阈值、清除再中毒和 unwind 回归，§64 |
| crates/motion-render/src/renderer.rs:245 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:276 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:327 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:464 | 已有覆盖 | 共享 texture_bytes 的删除减法保留，既有 trim 在提前返回及淘汰后均比较 images 实测总字节与账面值，首次/每 60 次 Warn、健康复位；§01 前置观测与 §63。不是减法内部即时日志，不等于修复漂移根因 |
| crates/motion-render/src/renderer.rs:594 | 新增观测 | GPU latch 的 uncaptured/device_lost/read 分别固定进程累计首次 Warn、第 60 次 Error，释放 guard 后输出恢复前 cause 是否存在及字节数、可用 renderer id 和作用域耗时；不复制原因、不清除 poison、不改变 None/首因锁存/设备丢失覆盖策略。真实中毒、公开 gpu_error/check_health、阈值、清除再中毒和 unwind 回归，§64 |
| crates/motion-render/src/renderer.rs:621 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:635 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:698 | 核实保留 | 同一 object/source/PTS/size 且既有 YUV 转换 ready 时返回 Ok(None)，表示无需新 encoder token。pending conversion 之前有明确 Err，正常缓存命中不应升级故障；既有 yuv_pipeline/submission 回归 |
| crates/motion-render/src/renderer.rs:715 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:716 | 核实保留 | 计算新旧视频/图像/嵌套纹理/蒙版替换所需的正增量，变小时 additional=0 是正确的保守预留；随后仍计算实际替换后总量并与 source budget 比较。不是从 texture_bytes 或累计 video_upload_bytes 中静默扣账 |
| crates/motion-render/src/renderer.rs:802 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:823 | 已有覆盖 | 共享 texture_bytes 的删除减法保留，既有 trim 在提前返回及淘汰后均比较 images 实测总字节与账面值，首次/每 60 次 Warn、健康复位；§01 前置观测与 §63。不是减法内部即时日志，不等于修复漂移根因 |
| crates/motion-render/src/renderer.rs:846 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:853 | 核实保留 | 计算新旧视频/图像/嵌套纹理/蒙版替换所需的正增量，变小时 additional=0 是正确的保守预留；随后仍计算实际替换后总量并与 source budget 比较。不是从 texture_bytes 或累计 video_upload_bytes 中静默扣账 |
| crates/motion-render/src/renderer.rs:870 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |
| crates/motion-render/src/renderer.rs:872 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |
| crates/motion-render/src/renderer.rs:1022 | 已有覆盖 | 共享 texture_bytes 的删除减法保留，既有 trim 在提前返回及淘汰后均比较 images 实测总字节与账面值，首次/每 60 次 Warn、健康复位；§01 前置观测与 §63。不是减法内部即时日志，不等于修复漂移根因 |
| crates/motion-render/src/renderer.rs:1063 | 已有覆盖 | 共享 texture_bytes 的删除减法保留，既有 trim 在提前返回及淘汰后均比较 images 实测总字节与账面值，首次/每 60 次 Warn、健康复位；§01 前置观测与 §63。不是减法内部即时日志，不等于修复漂移根因 |
| crates/motion-render/src/renderer.rs:1076 | 已有覆盖 | 共享 texture_bytes 的删除减法保留，既有 trim 在提前返回及淘汰后均比较 images 实测总字节与账面值，首次/每 60 次 Warn、健康复位；§01 前置观测与 §63。不是减法内部即时日志，不等于修复漂移根因 |
| crates/motion-render/src/renderer.rs:1111 | 已有覆盖 | 这是 trim 的 total 候选总量减去被淘汰字节，原报告泛称账面减法；同一 trim 退出前仍执行 check_texture_accounting，对共享账面漂移限频 Warn。未新增即时日志或校正统计 |
| crates/motion-render/src/renderer.rs:1115 | 核实保留 | image_clock 是 LRU 相对访问序号，u64 饱和不会回绕成最旧值；实际达到上限需极长期访问，饱和后的同序号择优不是现实输入故障。未把该终态计数器当作显存记账 |
| crates/motion-render/src/renderer.rs:1181 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:1187 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:1210 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:1309 | 核实保留 | 实际是 Instant.elapsed.as_micros 的 u128→u64 收窄，原报告 widened 描述不准确；要约 58 万年连续运行才超 u64 微秒，不作为现实挂起观测缺口修改时间契约，§63 同族 |
| crates/motion-render/src/renderer.rs:1368 | 核实保留 | 计算新旧视频/图像/嵌套纹理/蒙版替换所需的正增量，变小时 additional=0 是正确的保守预留；随后仍计算实际替换后总量并与 source budget 比较。不是从 texture_bytes 或累计 video_upload_bytes 中静默扣账 |
| crates/motion-render/src/renderer.rs:1418 | 已有覆盖 | 共享 texture_bytes 的删除减法保留，既有 trim 在提前返回及淘汰后均比较 images 实测总字节与账面值，首次/每 60 次 Warn、健康复位；§01 前置观测与 §63。不是减法内部即时日志，不等于修复漂移根因 |
| crates/motion-render/src/renderer.rs:1511 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1512 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1513 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1515 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1516 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1537 | 核实保留 | 实际是 Instant.elapsed.as_micros 的 u128→u64 收窄，原报告 widened 描述不准确；要约 58 万年连续运行才超 u64 微秒，不作为现实挂起观测缺口修改时间契约，§63 同族 |
| crates/motion-render/src/renderer.rs:1542 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:1558 | 已有覆盖 | 共享 texture_bytes 的删除减法保留，既有 trim 在提前返回及淘汰后均比较 images 实测总字节与账面值，首次/每 60 次 Warn、健康复位；§01 前置观测与 §63。不是减法内部即时日志，不等于修复漂移根因 |
| crates/motion-render/src/renderer.rs:1637 | 核实保留 | 计算新旧视频/图像/嵌套纹理/蒙版替换所需的正增量，变小时 additional=0 是正确的保守预留；随后仍计算实际替换后总量并与 source budget 比较。不是从 texture_bytes 或累计 video_upload_bytes 中静默扣账 |
| crates/motion-render/src/renderer.rs:1667 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1668 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1725 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1726 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1727 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1729 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1738 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1739 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1915 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1918 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1919 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1925 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1930 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1996 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1997 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:1999 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:2000 | 核实保留 | 这些位置参与目标尺寸、fit_viewport、比例、居中坐标或有限标志的 GPU f32 接口；当前直接/效果路径共同使用 fit_viewport 并保留实际 GPU 纹理/计划预算。不把接口精度转换当作丢失资源身份，亦不保证任意手工 Scene 的数值有效 |
| crates/motion-render/src/renderer.rs:2013 | 核实保留 | 实际是 Instant.elapsed.as_micros 的 u128→u64 收窄，原报告 widened 描述不准确；要约 58 万年连续运行才超 u64 微秒，不作为现实挂起观测缺口修改时间契约，§63 同族 |
| crates/motion-render/src/renderer.rs:2022 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:2023 | 核实保留 | 此处是 size_of/stride/Vec.len/frame.bytes 或有界上传字节数的 usize→u64，在支持的 32/64 位目标等宽/拓宽；输入另有像素长度/device/source budget 校验。video_upload_bytes 是累计流量，不是驻留纹理账面值 |
| crates/motion-render/src/renderer.rs:2152 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |
| crates/motion-render/src/renderer.rs:2158 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |
| crates/motion-render/src/renderer.rs:2172 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |
| crates/motion-render/src/renderer.rs:2174 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |
| crates/motion-render/src/renderer/compositing.rs:70 | 核实保留 | flags 来自有限 matte mode 的 2/4 位标志，size 为实际 GPU composition target；传入 DrawUniform 的 f32 是 GPU 坐标/标志接口，不是将 u64 资产身份收窄 |
| crates/motion-render/src/renderer/compositing.rs:71 | 核实保留 | flags 来自有限 matte mode 的 2/4 位标志，size 为实际 GPU composition target；传入 DrawUniform 的 f32 是 GPU 坐标/标志接口，不是将 u64 资产身份收窄 |
| crates/motion-render/src/renderer/compositing.rs:75 | 核实保留 | index×extract_stride 的 usize→u64 在支持的 32/64 位目标等宽/拓宽；实际索引和 buffer 范围的前提见该文件原始 :96 |
| crates/motion-render/src/effect_plan.rs:280 | 核实保留 | 经模型生成的有限曲线 LUT 先 clamp(0,1)×255 后 round 到 u8，是有意量化；手工 NaN LUT 不是本结论已校验的输入，§63 同族 |
| crates/motion-render/src/effect_plan.rs:933 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1034 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1080 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1084 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1144 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1145 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1149 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1344 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1345 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1355 | 核实保留 | work/output 是有限 scratch 槽号：work 为 1 或 4，output 为 0 或 work+1+i%2，最大 6；u32→i32 精确表示，不是资源身份或长期累加计数 |
| crates/motion-render/src/effect_plan.rs:1380 | 核实保留 | work/output 是有限 scratch 槽号：work 为 1 或 4，output 为 0 或 work+1+i%2，最大 6；u32→i32 精确表示，不是资源身份或长期累加计数 |
| crates/motion-render/src/effect_plan.rs:1383 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1386 | 核实保留 | work/output 是有限 scratch 槽号：work 为 1 或 4，output 为 0 或 work+1+i%2，最大 6；u32→i32 精确表示，不是资源身份或长期累加计数 |
| crates/motion-render/src/effect_plan.rs:1424 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/effect_plan.rs:1425 | 核实保留 | 正规 layer 索引≤127、asset 槽位≤256、LUT 数来自受限模型效果，转 i32 后按 -(asset+1)、draw+1、SOURCE_TOKEN-layer 或 -1 哨兵编码；不会溢出 i32。保留完整有符号 token，单帧预算不能替长期 program 索引背书 |
| crates/motion-render/src/renderer.rs:873 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |
| crates/motion-render/src/renderer.rs:2160 | 核实保留 | u8 通道/alpha 精确转 f32 后在 sRGB/linear 间预乘或去预乘；读回除 alpha 后 clamp(0,1)，最终 0..255 round 量化为 PNG/sRGB 字节。是明确颜色编码契约，不是资源 id 截断 |

## B1 · 图片代理、场景采样与像素边界（末批 107 项）

本批关联 B1 剩余107行，并补强已覆盖代理锁站点及写入/清理失败日志的解锁时序；新增日志安全修正不重复计为新站点。原始脚本未排除的4个测试数值候选在此明确保留

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-render/examples/effect_preview_probe.rs:88 | 核实保留 | 仅对几种builtin修改探针参数，其他effect保留自己的默认值，后续仍渲染该effect；wildcard没有忽略Result |
| crates/motion-render/examples/video_submit_probe.rs:111 | 功能边界 | 独立性能探针把GPU timestamp逆序/回绕差夹0，未新增驱动时间戳异常观测或实现回绕解释；不影响产品呈现，不把其0us解释为真实GPU耗时证明 |
| crates/motion-render/src/image_resources.rs:128 | 核实保留 | Preview 的 edge 先限制到 1..2048，longest>edge 时整数 ceil(width×edge/longest)<=edge；两个 u32 的乘积在 u64 内。这两处并不产生预览尺寸截断，Full 原样返回 |
| crates/motion-render/src/image_resources.rs:129 | 核实保留 | Preview 的 edge 先限制到 1..2048，longest>edge 时整数 ceil(width×edge/longest)<=edge；两个 u32 的乘积在 u64 内。这两处并不产生预览尺寸截断，Full 原样返回 |
| crates/motion-render/src/image_resources.rs:284 | 核实保留 | Read 字节数、Vec 长度、枚举像素 x 的 usize→u64 在支持的32/64位等宽或拓宽；解码输出另受128MiB预算及真实长度校验，finish_row 的 x≤有效源宽16384 |
| crates/motion-render/src/image_resources.rs:300 | 已有覆盖 | §09/52 已将 lock().ok 缓存丢弃改为恢复并校验 header/长度/hash；§65 补强恢复首 Warn/生命周期第60次 Error、解锁后输出及真实文件回归，clear_poison 与读取结果不变 |
| crates/motion-render/src/image_resources.rs:331 | 核实保留 | Read 字节数、Vec 长度、枚举像素 x 的 usize→u64 在支持的32/64位等宽或拓宽；解码输出另受128MiB预算及真实长度校验，finish_row 的 x≤有效源宽16384 |
| crates/motion-render/src/image_resources.rs:359 | 已有覆盖 | §09 去掉先删除目的再单独减 metadata.len 的旧逻辑；现在 checked_add 对除目的外的快照条目求和，原目的留到最终 rename。此旧减法已不存在，非新增GPU记账告警 |
| crates/motion-render/src/image_resources.rs:366 | 核实保留 | 磁盘代理 total 来自同一 old 快照的 checked_add，每个非目的项最多成功删除一次后减同一 bytes；失败直接 Err，当前饱和减不可能因这个快照序列少计。它不是 renderer.texture_bytes 驻留账本，也未声称重新扫描并发外部修改后的磁盘实值 |
| crates/motion-render/src/image_resources.rs:412 | 核实保留 | Read 字节数、Vec 长度、枚举像素 x 的 usize→u64 在支持的32/64位等宽或拓宽；解码输出另受128MiB预算及真实长度校验，finish_row 的 x≤有效源宽16384 |
| crates/motion-render/src/image_resources.rs:466 | 核实保留 | linear_lut 的 i 为0..255，f32精确表示，归一化得到色彩传递表；不是资源ID或尺寸收窄 |
| crates/motion-render/src/image_resources.rs:480 | 核实保留 | encode 对有限非负累积色彩先clamp0..1再round量化u8；alpha是有效像素alpha平均值的0..255量化，保留线性预乘/box滤波像素契约。不是任意非有限人工输入的验证 |
| crates/motion-render/src/image_resources.rs:483 | 核实保留 | 原像素 alpha 为u8，转f32并除255用于线性预乘，整数部分精确且量化规则保持 |
| crates/motion-render/src/image_resources.rs:491 | 核实保留 | Read 字节数、Vec 长度、枚举像素 x 的 usize→u64 在支持的32/64位等宽或拓宽；解码输出另受128MiB预算及真实长度校验，finish_row 的 x≤有效源宽16384 |
| crates/motion-render/src/image_resources.rs:492 | 核实保留 | Read 字节数、Vec 长度、枚举像素 x 的 usize→u64 在支持的32/64位等宽或拓宽；解码输出另受128MiB预算及真实长度校验，finish_row 的 x≤有效源宽16384 |
| crates/motion-render/src/image_resources.rs:493 | 核实保留 | finish_row 的 n 是分箱覆盖的源像素数，源宽高各≤16384，整数积≤2^28；f32可能舍入但作为滤波分母是既有数值契约，不能据此宣称全32位整数无损；既有整幅/代理/Adam7像素回归 |
| crates/motion-render/src/image_resources.rs:497 | 核实保留 | encode 对有限非负累积色彩先clamp0..1再round量化u8；alpha是有效像素alpha平均值的0..255量化，保留线性预乘/box滤波像素契约。不是任意非有限人工输入的验证 |
| crates/motion-render/src/image_resources.rs:614 | 核实保留 | 原像素 alpha 为u8，转f32并除255用于线性预乘，整数部分精确且量化规则保持 |
| crates/motion-render/src/image_resources.rs:78 | 核实保留 | upcoming_assets 只收集图片/文字栅格和递归合成，其他 Solid/Video/Audio/Null/Vector 不属于图片预取依赖；wildcard 是有意分流，不是丢失执行错误 |
| crates/motion-render/src/particle_emitter.rs:122 | 功能边界 | 正规采样有有限frame、正fps、rate≤10000、life≤120及20000粒子预算，正常出生区间可表示；直接手工 SampledEffect.local_frame/Scene.fps 未在此入口统一拒绝NaN/超大值，i64饱和后+1/区间计算边界保留，不以正常预算证明全部公有输入安全 |
| crates/motion-render/src/particle_emitter.rs:127 | 功能边界 | 正规采样有有限frame、正fps、rate≤10000、life≤120及20000粒子预算，正常出生区间可表示；直接手工 SampledEffect.local_frame/Scene.fps 未在此入口统一拒绝NaN/超大值，i64饱和后+1/区间计算边界保留，不以正常预算证明全部公有输入安全 |
| crates/motion-render/src/particle_emitter.rs:128 | 功能边界 | 正规采样有有限frame、正fps、rate≤10000、life≤120及20000粒子预算，正常出生区间可表示；直接手工 SampledEffect.local_frame/Scene.fps 未在此入口统一拒绝NaN/超大值，i64饱和后+1/区间计算边界保留，不以正常预算证明全部公有输入安全 |
| crates/motion-render/src/particle_emitter.rs:172 | 核实保留 | 正规sprite资产宽高1..16384，f32精确表示，取最长边归一化宽高比；缺资产仍Err。手工sprite_assets零尺寸不在模型验证证明内 |
| crates/motion-render/src/particle_emitter.rs:173 | 核实保留 | 正规sprite资产宽高1..16384，f32精确表示，取最长边归一化宽高比；缺资产仍Err。手工sprite_assets零尺寸不在模型验证证明内 |
| crates/motion-render/src/particle_emitter.rs:183 | 核实保留 | 有效age/life先clamp0..1再转f32作为寿命进度与颜色/大小插值；不是ID或保留完整时间精度的承诺 |
| crates/motion-render/src/particle_emitter.rs:197 | 核实保留 | 解析轨迹的duration/age/acceleration_time转f32供glam位置向量，之后检查clip与rect有限并Err；保留物理采样精度，不声称极端非有限中间值无损 |
| crates/motion-render/src/particle_emitter.rs:198 | 核实保留 | 解析轨迹的duration/age/acceleration_time转f32供glam位置向量，之后检查clip与rect有限并Err；保留物理采样精度，不声称极端非有限中间值无损 |
| crates/motion-render/src/particle_emitter.rs:199 | 核实保留 | 解析轨迹的duration/age/acceleration_time转f32供glam位置向量，之后检查clip与rect有限并Err；保留物理采样精度，不声称极端非有限中间值无损 |
| crates/motion-render/src/particle_emitter.rs:261 | 核实保留 | 正常出生粒子≤MAX_PARTICLES=20000，投影剔除后sorted更少，append前仍有frame MAX_SPRITES=65536检查；visible的u32不会截断正规输出 |
| crates/motion-render/src/particle_emitter.rs:268 | 核实保留 | remaining是保留birth缓存容量预算，不是已用字节；当前缓存容量大于MAX_SPRITES时剩余额度按0处理、驱逐其他缓存是有意fail-closed，不是统计少计 |
| crates/motion-render/src/particle_emitter.rs:48 | 核实保留 | birth.id 实际是i64；转u32只取低32位供wrapping随机hash，粒子排序/缓存仍保存完整i64 birth ID，负prewarm同样确定。会周期复用随机分布，不声称全寿命随机键唯一 |
| crates/motion-render/src/particle_emitter.rs:57 | 核实保留 | 正规 EffectParam 的离散shape由valid_value及整数验证，history读取持有该离散值，match只接受0/1/2，其余Err；手工伪造参数不是此模型证明范围 |
| crates/motion-render/src/particle_emitter.rs:77 | 核实保留 | 有限参数 inherit 与固定H=1/120 的物理采样分母转f32是既有速度数值算法，birth随后验证position/velocity/size有限；不宣称f64分母逐位无损 |
| crates/motion-render/src/position_path.rs:194 | 核实保留 | 正规Project宽高1..8192转f32精确，构建平面正交投影/原点平移；sample已检查有效frame并验证目标，不改变节点窗口、父坐标/相机及编辑契约 |
| crates/motion-render/src/position_path.rs:195 | 核实保留 | 正规Project宽高1..8192转f32精确，构建平面正交投影/原点平移；sample已检查有效frame并验证目标，不改变节点窗口、父坐标/相机及编辑契约 |
| crates/motion-render/src/position_path.rs:196 | 核实保留 | 正规Project宽高1..8192转f32精确，构建平面正交投影/原点平移；sample已检查有效frame并验证目标，不改变节点窗口、父坐标/相机及编辑契约 |
| crates/motion-render/src/position_path.rs:197 | 核实保留 | 正规Project宽高1..8192转f32精确，构建平面正交投影/原点平移；sample已检查有效frame并验证目标，不改变节点窗口、父坐标/相机及编辑契约 |
| crates/motion-render/src/position_path.rs:205 | 核实保留 | 正规Project宽高1..8192转f32精确，构建平面正交投影/原点平移；sample已检查有效frame并验证目标，不改变节点窗口、父坐标/相机及编辑契约 |
| crates/motion-render/src/position_path.rs:206 | 核实保留 | 正规Project宽高1..8192转f32精确，构建平面正交投影/原点平移；sample已检查有效frame并验证目标，不改变节点窗口、父坐标/相机及编辑契约 |
| crates/motion-render/src/position_path.rs:56 | 核实保留 | near是partition_point合法插入点；saturating_sub为选择最多512个可见key窗口时夹到首项，输出另显式keys_truncated，不是记账漂移或丢失编辑状态 |
| crates/motion-render/src/position_path.rs:57 | 核实保留 | near是partition_point合法插入点；saturating_sub为选择最多512个可见key窗口时夹到首项，输出另显式keys_truncated，不是记账漂移或丢失编辑状态 |
| crates/motion-render/src/scene.rs:282 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:283 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:284 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:285 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:336 | 核实保留 | VideoAsset.validate 限制0≤start<end≤duration≤3600000000us，可用i64表示；time先通过start/end比较，之后time as u64非负。手工跳过资产验证不在此证明内 |
| crates/motion-render/src/scene.rs:341 | 核实保留 | VideoAsset.validate 限制0≤start<end≤duration≤3600000000us，可用i64表示；time先通过start/end比较，之后time as u64非负。手工跳过资产验证不在此证明内 |
| crates/motion-render/src/scene.rs:410 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:411 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:55 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:551 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:552 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:578 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:579 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:59 | 核实保留 | 正规Project宽高1..8192，转f32精确，用于正交投影、屏幕坐标/裁切；不涉及资产ID。project_point 的任意相机除w/手工Scene零尺寸仍属原几何入口边界，不能据这些尺寸转换宣称投影全安全 |
| crates/motion-render/src/scene.rs:619 | 核实保留 | f64射线局部坐标归一化为f32 UV，接着要求UV与depth在0..1才加入命中；舍入契约保留，不改边缘命中容差或声称转换前后边界完全相同 |
| crates/motion-render/src/scene.rs:620 | 核实保留 | f64射线局部坐标归一化为f32 UV，接着要求UV与depth在0..1才加入命中；舍入契约保留，不改边缘命中容差或声称转换前后边界完全相同 |
| crates/motion-render/src/scene.rs:627 | 核实保留 | depth加入HitCandidate前已判定0..1，f32深度用于既有排序；小数有舍入，不是任意范围浮点截断 |
| crates/motion-render/src/scene.rs:672 | 核实保留 | 投影拖拽offset从f64转f32后立即逐项is_finite并Err("drag exceeds numeric range")，保留原有限舍入与坐标轴符号 |
| crates/motion-render/src/scene_generator.rs:101 | 核实保留 | rate/life先分别检查范围并排除NaN/Infinity，最大乘积1200000，ceil转usize安全；随后与MAX_PARTICLES=20000比较拒绝超预算，不是饱和后放行 |
| crates/motion-render/src/scene_generator.rs:170 | 核实保留 | rate/life先分别检查范围并排除NaN/Infinity，最大乘积1200000，ceil转usize安全；随后与MAX_PARTICLES=20000比较拒绝超预算，不是饱和后放行 |
| crates/motion-render/src/scene_generator.rs:184 | 功能边界 | 正常有限frame/正fps/粒子参数与20000预算限定活跃出生窗口；但直接公开generate接受的人工local_frame/fps并无统一finite验证，极值转i64及+1/区间算术边界保留，未改返回或添加调度超时 |
| crates/motion-render/src/scene_generator.rs:185 | 功能边界 | 正常有限frame/正fps/粒子参数与20000预算限定活跃出生窗口；但直接公开generate接受的人工local_frame/fps并无统一finite验证，极值转i64及+1/区间算术边界保留，未改返回或添加调度超时 |
| crates/motion-render/src/scene_generator.rs:190 | 核实保留 | 正规shape离散枚举已验证整数及0..2，代码另拒绝shape>2；人工SampledEffect负值/NaN先转u32的退化没有在本项修复，不声称任意参数安全 |
| crates/motion-render/src/scene_generator.rs:226 | 功能边界 | 正常有限frame/正fps/粒子参数与20000预算限定活跃出生窗口；但直接公开generate接受的人工local_frame/fps并无统一finite验证，极值转i64及+1/区间算术边界保留，未改返回或添加调度超时 |
| crates/motion-render/src/scene_generator.rs:232 | 核实保留 | 有效life≤120秒，包围球使用f32计算，随后radius.is_finite明确拒绝非有限；保留物理半径浮点精度而非身份/账本转换 |
| crates/motion-render/src/scene_generator.rs:233 | 核实保留 | 有效life≤120秒，包围球使用f32计算，随后radius.is_finite明确拒绝非有限；保留物理半径浮点精度而非身份/账本转换 |
| crates/motion-render/src/scene_generator.rs:252 | 核实保留 | 有效age换算f32供旧粒子轨迹；轨迹clip/rect后续finite检查，保留渲染数值精度，不修改host时间或宣称完整f64精度 |
| crates/motion-render/src/scene_generator.rs:254 | 核实保留 | 原报告称usize，源码实际i64 birth id；u32仅作为随机hash通道，birth_id仍以i64用于时间/排序。保留低32位周期分布，不当作全局唯一粒子身份 |
| crates/motion-render/src/scene_generator.rs:343 | 核实保留 | 正规合成宽高1..8192，f32可精确表示，分别用于正交投影、NDC尺寸及遮挡半径换算；零/非有限手工Scene和任意alpha输入的完整校验不在本项关闭范围 |
| crates/motion-render/src/scene_generator.rs:344 | 核实保留 | 正规合成宽高1..8192，f32可精确表示，分别用于正交投影、NDC尺寸及遮挡半径换算；零/非有限手工Scene和任意alpha输入的完整校验不在本项关闭范围 |
| crates/motion-render/src/scene_generator.rs:345 | 核实保留 | 正规合成宽高1..8192，f32可精确表示，分别用于正交投影、NDC尺寸及遮挡半径换算；零/非有限手工Scene和任意alpha输入的完整校验不在本项关闭范围 |
| crates/motion-render/src/scene_generator.rs:346 | 核实保留 | 正规合成宽高1..8192，f32可精确表示，分别用于正交投影、NDC尺寸及遮挡半径换算；零/非有限手工Scene和任意alpha输入的完整校验不在本项关闭范围 |
| crates/motion-render/src/scene_generator.rs:382 | 核实保留 | 正规合成宽高1..8192，f32可精确表示，分别用于正交投影、NDC尺寸及遮挡半径换算；零/非有限手工Scene和任意alpha输入的完整校验不在本项关闭范围 |
| crates/motion-render/src/scene_generator.rs:383 | 核实保留 | 正规合成宽高1..8192，f32可精确表示，分别用于正交投影、NDC尺寸及遮挡半径换算；零/非有限手工Scene和任意alpha输入的完整校验不在本项关闭范围 |
| crates/motion-render/src/scene_generator.rs:399 | 核实保留 | 镜头元素rays在SceneSettings/Element验证的有限离散范围内，转f32供sprite style数值，保留GPU ABI |
| crates/motion-render/src/scene_generator.rs:410 | 核实保留 | 最终out总长已检查≤MAX_SPRITES=65536，start在调用前保存且失败会truncate，因此visible=(out.len-start)的u32转换有界 |
| crates/motion-render/src/scene_generator.rs:427 | 核实保留 | 正规合成宽高1..8192，f32可精确表示，分别用于正交投影、NDC尺寸及遮挡半径换算；零/非有限手工Scene和任意alpha输入的完整校验不在本项关闭范围 |
| crates/motion-render/src/scene_generator.rs:44 | 功能边界 | PlanBuilder.set_alpha 正规路径检查正宽高/rgba长度/32MiB预算，clamped texel取样有界；公开AlphaImage字段和generate传入HashMap可手工不匹配/零尺寸，sample本身未新增校验，不能把正规缓存入口证明推广到全部调用 |
| crates/motion-render/src/scene_generator.rs:48 | 功能边界 | PlanBuilder.set_alpha 正规路径检查正宽高/rgba长度/32MiB预算，clamped texel取样有界；公开AlphaImage字段和generate传入HashMap可手工不匹配/零尺寸，sample本身未新增校验，不能把正规缓存入口证明推广到全部调用 |
| crates/motion-render/src/scene_generator.rs:49 | 功能边界 | PlanBuilder.set_alpha 正规路径检查正宽高/rgba长度/32MiB预算，clamped texel取样有界；公开AlphaImage字段和generate传入HashMap可手工不匹配/零尺寸，sample本身未新增校验，不能把正规缓存入口证明推广到全部调用 |
| crates/motion-render/src/scene_generator.rs:71 | 核实保留 | random在移除低8位后最多24bit整数，可由f32精确表示，再除2^24形成[0,1)确定性随机量；完整种子仍在hash之前参与 |
| crates/motion-render/src/selection_geometry.rs:16 | 功能边界 | time.floor转i32用于与shake shader一致的分段噪声；超长local_frame/大频率可能超i32历元并饱和，未添加全时长范围验证或改变噪声。模型有限不等于所有合法时间都落i32，保留此数值边界 |
| crates/motion-render/src/selection_geometry.rs:184 | 核实保留 | 正规Scene/Project尺寸1..8192在f32精确表示，用于屏幕overlay矩形、source size与像素坐标；polygon在clip非有限或w≤0时仍None，不改变轮廓或命中行为 |
| crates/motion-render/src/selection_geometry.rs:197 | 核实保留 | 正规Scene/Project尺寸1..8192在f32精确表示，用于屏幕overlay矩形、source size与像素坐标；polygon在clip非有限或w≤0时仍None，不改变轮廓或命中行为 |
| crates/motion-render/src/selection_geometry.rs:198 | 核实保留 | 正规Scene/Project尺寸1..8192在f32精确表示，用于屏幕overlay矩形、source size与像素坐标；polygon在clip非有限或w≤0时仍None，不改变轮廓或命中行为 |
| crates/motion-render/src/selection_geometry.rs:21 | 核实保留 | epoch as u32以原32位位型进入wrapping hash，负历元也须保持确定；是有符号hash编码而非截断资源ID |
| crates/motion-render/src/selection_geometry.rs:51 | 核实保留 | 秒与频率转f32对应固定内置shake.wgsl数值算法，种子numeric-float位型按既有ABI，较长时间/较大种子存在精度限制；不声称所有f64时刻/32bit种子可区分 |
| crates/motion-render/src/selection_geometry.rs:97 | 核实保留 | 正规Scene/Project尺寸1..8192在f32精确表示，用于屏幕overlay矩形、source size与像素坐标；polygon在clip非有限或w≤0时仍None，不改变轮廓或命中行为 |
| crates/motion-render/src/vector_mesh.rs:72 | 核实保留 | 跨batch共用BoundedGeometry，追加前限MAX_TRIANGLE_VERTICES=262144且受frame剩余额度，command start/end是已保存output长度，可用u32；实际Lyon预算回归覆盖，不把Vec capacity当作证明 |
| crates/motion-render/src/vector_mesh.rs:85 | 核实保留 | 跨batch共用BoundedGeometry，追加前限MAX_TRIANGLE_VERTICES=262144且受frame剩余额度，command start/end是已保存output长度，可用u32；实际Lyon预算回归覆盖，不把Vec capacity当作证明 |
| crates/motion-render/src/vector_mesh/budget.rs:195 | 核实保留 | 均位于cfg(test)预算夹具：i%3、矩形边插值、断言构造或极端点生成，用于真实Lyon峰值/裁剪回归；不属于产品静默降级，未修改既有断言 |
| crates/motion-render/src/vector_mesh/budget.rs:210 | 核实保留 | 均位于cfg(test)预算夹具：i%3、矩形边插值、断言构造或极端点生成，用于真实Lyon峰值/裁剪回归；不属于产品静默降级，未修改既有断言 |
| crates/motion-render/src/vector_mesh/budget.rs:215 | 核实保留 | 均位于cfg(test)预算夹具：i%3、矩形边插值、断言构造或极端点生成，用于真实Lyon峰值/裁剪回归；不属于产品静默降级，未修改既有断言 |
| crates/motion-render/src/vector_mesh/budget.rs:233 | 核实保留 | 均位于cfg(test)预算夹具：i%3、矩形边插值、断言构造或极端点生成，用于真实Lyon峰值/裁剪回归；不属于产品静默降级，未修改既有断言 |
| crates/motion-render/src/vector_mesh/budget.rs:28 | 核实保留 | 容量倍增的saturating_mul后立即min(limit)，所需needed先由vertex/triangle回调预算拒绝；饱和只决定增长策略，不会绕过输出上限或减少字节账面 |
| crates/motion-render/src/video_pixels.rs:241 | 核实保留 | usize平面长度转u64等宽/拓宽，validate已限制正尺寸≤8192且像素≤16Mi及phase/旋转，再与精确u64平面成本比较；非法plane仍Err |
| crates/motion-render/src/video_pixels.rs:242 | 核实保留 | usize平面长度转u64等宽/拓宽，validate已限制正尺寸≤8192且像素≤16Mi及phase/旋转，再与精确u64平面成本比较；非法plane仍Err |
| crates/motion-render/src/video_pixels.rs:271 | 核实保留 | YUV尺寸已≤请求Preview边界时没有必要构造新proxy，Ok(None)明确表示可复用原帧；self.validate先执行，非持续非就绪，无需失败计数 |
| crates/motion-render/src/video_pixels.rs:276 | 核实保留 | map先做u64乘除，再min(source-1)，source由PixelLayout验证正且≤8192，结果u32有界；非源像素值截断而是坐标采样 |
| crates/motion-render/src/video_pixels.rs:340 | 核实保留 | YUV转换RGB整数加舍入后clamp0..255再转u8，是必需的色域量化；保留颜色/布局/裁剪像素验收，不为正常超色域分量打每帧告警 |
| crates/motion-render/src/video_pixels.rs:341 | 核实保留 | YUV转换RGB整数加舍入后clamp0..255再转u8，是必需的色域量化；保留颜色/布局/裁剪像素验收，不为正常超色域分量打每帧告警 |
| crates/motion-render/src/video_pixels.rs:342 | 核实保留 | YUV转换RGB整数加舍入后clamp0..255再转u8，是必需的色域量化；保留颜色/布局/裁剪像素验收，不为正常超色域分量打每帧告警 |
| crates/motion-render/src/video_pixels.rs:78 | 核实保留 | RGB565通道先分别mask到5/6/5位，再乘255除31/63，结果0..255，u8转换有界，保留既有像素解码/旋转回归 |
| crates/motion-render/src/video_pixels.rs:79 | 核实保留 | RGB565通道先分别mask到5/6/5位，再乘255除31/63，结果0..255，u8转换有界，保留既有像素解码/旋转回归 |
| crates/motion-render/src/video_pixels.rs:80 | 核实保留 | RGB565通道先分别mask到5/6/5位，再乘255除31/63，结果0..255，u8转换有界，保留既有像素解码/旋转回归 |
| crates/motion-render/tests/common_effects.rs:85 | 核实保留 | 测试只为wipe/channel_mixer覆盖默认参数，其他builtin保留默认继续render，并assert_ne/alpha/零opacity验证；wildcard不是跳过effect或吞Err |

## B2 · Host 与核心编辑/表达式边界（41 项）

本批新增三个固定观测站点，关联9行候选；只补证据，保持返回值、编辑提交和回退策略。其他行逐项区分已有覆盖、正常契约和未关闭边界

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-core/src/editor.rs:1041 | 核实保留 | command成功可返回None表示无附加EditResult；仍经历批次prepare、验证、提交revision/history流程，Err另走rollback。None不是命令未执行或失败，保留命令目录契约 |
| crates/motion-core/src/expressions/mod.rs:15 | 核实保留 | ExpressionTarget为派生Serialize枚举，仅u64/enum/Option/string，无非有限浮点、自定义失败序列化器或JSON map键；当前to_string不存在可恢复序列化失败输入，保留原表达式错误target。OOM终止不当作可恢复Err验证 |
| crates/motion-core/src/expressions/transpile.rs:166 | 核实保留 | 只改写简单标识符复合赋值；成员赋值保留JS原语义，以免getter/index副作用重复执行。wildcard没有吞Result，保留既有表达式运行时回归 |
| crates/motion-core/src/expressions/transpile.rs:72 | 核实保留 | 源码明确其他completion形式返回undefined并由表达式结果类型检查报错；不是解析错误吞掉。保留JavaScript完成值支持范围，不扩展语言或记录正常AST分流 |
| crates/motion-core/src/history_accounting.rs:181 | 核实保留 | JSON字节累计饱和到usize::MAX是预算fail-closed上界，不能溢出后变小放行；值/键通过counted序列化累计，既有history预算回归与model序列化失败观测。不是renderer驻留减法少计，不分配usize极值对象做伪验收 |
| crates/motion-core/src/history_accounting.rs:182 | 核实保留 | JSON字节累计饱和到usize::MAX是预算fail-closed上界，不能溢出后变小放行；值/键通过counted序列化累计，既有history预算回归与model序列化失败观测。不是renderer驻留减法少计，不分配usize极值对象做伪验收 |
| crates/motion-core/src/history_accounting.rs:293 | 核实保留 | JSON字节累计饱和到usize::MAX是预算fail-closed上界，不能溢出后变小放行；值/键通过counted序列化累计，既有history预算回归与model序列化失败观测。不是renderer驻留减法少计，不分配usize极值对象做伪验收 |
| crates/motion-core/src/history_accounting.rs:294 | 核实保留 | JSON字节累计饱和到usize::MAX是预算fail-closed上界，不能溢出后变小放行；值/键通过counted序列化累计，既有history预算回归与model序列化失败观测。不是renderer驻留减法少计，不分配usize极值对象做伪验收 |
| crates/motion-core/src/history_accounting.rs:295 | 核实保留 | JSON字节累计饱和到usize::MAX是预算fail-closed上界，不能溢出后变小放行；值/键通过counted序列化累计，既有history预算回归与model序列化失败观测。不是renderer驻留减法少计，不分配usize极值对象做伪验收 |
| crates/motion-core/src/history_accounting.rs:296 | 核实保留 | JSON字节累计饱和到usize::MAX是预算fail-closed上界，不能溢出后变小放行；值/键通过counted序列化累计，既有history预算回归与model序列化失败观测。不是renderer驻留减法少计，不分配usize极值对象做伪验收 |
| crates/motion-host/src/ops/composition.rs:110 | 新增观测 | §66 在 with_session 正常/Err 出口观察既有 last_error：按Session累计带错误锁存的调用，首次Warn、第60次Error，之后抑制，出口无错误复位；包含session/frame/composition/有界原因及入口到出口耗时，registry解锁后输出。保留编辑已提交、Ok(snapshot)/Ok(false)及主错误语义；不是每次新采样失败计数，不声称捕获出口前被清除的错误或独立Session直接调用 |
| crates/motion-host/src/ops/composition.rs:268 | 新增观测 | §66 在 with_session 正常/Err 出口观察既有 last_error：按Session累计带错误锁存的调用，首次Warn、第60次Error，之后抑制，出口无错误复位；包含session/frame/composition/有界原因及入口到出口耗时，registry解锁后输出。保留编辑已提交、Ok(snapshot)/Ok(false)及主错误语义；不是每次新采样失败计数，不声称捕获出口前被清除的错误或独立Session直接调用 |
| crates/motion-host/src/ops/editing.rs:178 | 新增观测 | §66 在 with_session 正常/Err 出口观察既有 last_error：按Session累计带错误锁存的调用，首次Warn、第60次Error，之后抑制，出口无错误复位；包含session/frame/composition/有界原因及入口到出口耗时，registry解锁后输出。保留编辑已提交、Ok(snapshot)/Ok(false)及主错误语义；不是每次新采样失败计数，不声称捕获出口前被清除的错误或独立Session直接调用 |
| crates/motion-host/src/ops/editing.rs:39 | 新增观测 | §66 在 with_session 正常/Err 出口观察既有 last_error：按Session累计带错误锁存的调用，首次Warn、第60次Error，之后抑制，出口无错误复位；包含session/frame/composition/有界原因及入口到出口耗时，registry解锁后输出。保留编辑已提交、Ok(snapshot)/Ok(false)及主错误语义；不是每次新采样失败计数，不声称捕获出口前被清除的错误或独立Session直接调用 |
| crates/motion-host/src/ops/effects.rs:301 | 新增观测 | §66 在 with_session 正常/Err 出口观察既有 last_error：按Session累计带错误锁存的调用，首次Warn、第60次Error，之后抑制，出口无错误复位；包含session/frame/composition/有界原因及入口到出口耗时，registry解锁后输出。保留编辑已提交、Ok(snapshot)/Ok(false)及主错误语义；不是每次新采样失败计数，不声称捕获出口前被清除的错误或独立Session直接调用 |
| crates/motion-host/src/ops/effects.rs:310 | 核实保留 | last与now来自Instant，通常记录活动后再检查lease；饱和对传入较早Instant按未超期处理是原租约策略，不是系统墙钟回拨。保持gesture恢复时机，不新增正常租约告警 |
| crates/motion-host/src/ops/export.rs:56 | 已有覆盖 | 此前已将effect_resource_info的解码失败改为带资源路径Err，audio_voices错误带export audio plan原因传播；不再以0x0/false伪装有效计划。沿用现有failure_boundaries等真实失败回归，本批不重复改变返回值 |
| crates/motion-host/src/ops/export.rs:95 | 已有覆盖 | 此前已将effect_resource_info的解码失败改为带资源路径Err，audio_voices错误带export audio plan原因传播；不再以0x0/false伪装有效计划。沿用现有failure_boundaries等真实失败回归，本批不重复改变返回值 |
| crates/motion-host/src/ops/fonts.rs:87 | 核实保留 | font_atlas/font_text接口缺省或非string字体使用builtin_id的既有参数契约；显式string ID找不到仍Err(font is not imported)。不改变参数校验、字体选择或为正常缺省打日志 |
| crates/motion-host/src/ops/media.rs:220 | 核实保留 | 导入名称为可选参数，无名称按媒体类型生成默认名，源文件验证/事务另传播错误；不是失败回退，不增加正常导入告警 |
| crates/motion-host/src/ops/media.rs:36 | 已有覆盖 | 当前打开媒体后metadata错误直接传播，随后仍检查is_file；原.ok丢失size/is_file分支已不存在，沿用已有媒体文件失败回归 |
| crates/motion-host/src/ops/preview.rs:212 | 新增观测 | §66 在 with_session 正常/Err 出口观察既有 last_error：按Session累计带错误锁存的调用，首次Warn、第60次Error，之后抑制，出口无错误复位；包含session/frame/composition/有界原因及入口到出口耗时，registry解锁后输出。保留编辑已提交、Ok(snapshot)/Ok(false)及主错误语义；不是每次新采样失败计数，不声称捕获出口前被清除的错误或独立Session直接调用 |
| crates/motion-host/src/ops/project.rs:128 | 已有覆盖 | 此前集中save_previous：legacy validate_assets失败明确Warn带root/原因后按旧契约跳过保存，显式Save/Discard遵照调用者选择；正常目录切换测试已有覆盖，不将选择Discard计作失败 |
| crates/motion-host/src/ops/project.rs:21 | 已有覆盖 | 此前集中save_previous：legacy validate_assets失败明确Warn带root/原因后按旧契约跳过保存，显式Save/Discard遵照调用者选择；正常目录切换测试已有覆盖，不将选择Discard计作失败 |
| crates/motion-host/src/ops/snapshot.rs:253 | 已有覆盖 | 此前audio_voices失败输出has_audio:null及audio_error，原因变化时Warn、恢复清除旧原因，非静默false。本批未调整该既有日志位于audio_snapshot_error RefCell和Session业务锁内的时序，不能概称所有host日志已解锁 |
| crates/motion-host/src/preview_inputs.rs:145 | 新增观测 | §66 保留mtime不可用时None及长度比较退化；固定进程生命周期首Warn/第60次Error/之后抑制，健康文件不复位，重启清零，无动态路径表。上下文source ID/有界路径及原因；with_session内至多两个阈值事件随线程作用域延迟到解锁后输出，含unwind。NTFS自然失败未重现，测试向生产helper注入Unsupported IO结果，健康metadata为真实文件 |
| crates/motion-host/src/preview_inputs.rs:471 | 功能边界 | 真实failed状态携带decoder具体error并传播；内部未知状态或缺失error仍替代为通用preview video decode failed，未加独立协议异常站点/原始状态观测。本批不声称异常内部状态完整可定位，也不改变JSON/返回契约 |
| crates/motion-host/src/preview_inputs.rs:75 | 核实保留 | 枚举ID直接来自同一不可变Project的根及compositions，composition_view只会在ID不存在时Err，当前循环每个ID均可查到；不是实际Result失败被遗漏。手工重复ID造成根/首条遮蔽属于模型验证契约，不改输入选择行为 |
| crates/motion-host/src/session.rs:288 | 新增观测 | §66 在 with_session 正常/Err 出口观察既有 last_error：按Session累计带错误锁存的调用，首次Warn、第60次Error，之后抑制，出口无错误复位；包含session/frame/composition/有界原因及入口到出口耗时，registry解锁后输出。保留编辑已提交、Ok(snapshot)/Ok(false)及主错误语义；不是每次新采样失败计数，不声称捕获出口前被清除的错误或独立Session直接调用 |
| crates/motion-host/src/session.rs:538 | 核实保留 | 视频prepare已返回就绪帧，renderer upload的None为同一源/同一帧已驻留无需重复上传；不是decode pending。既有视频上传/缓存验证和GPU计账观测保持，不对正常复用告警 |
| crates/motion-host/src/session.rs:655 | 核实保留 | acquire与present计时为同一began包围的串行不重叠区间，正常微秒和不超过wall；饱和减仅稳健计时夹限，u64微秒量级远超实际进程寿命。as_micros收窄的极端长期回绕不据此声称逐位无损，不将正常计时与GPU驻留少计混同 |
| crates/motion-host/src/session.rs:656 | 核实保留 | acquire与present计时为同一began包围的串行不重叠区间，正常微秒和不超过wall；饱和减仅稳健计时夹限，u64微秒量级远超实际进程寿命。as_micros收窄的极端长期回绕不据此声称逐位无损，不将正常计时与GPU驻留少计混同 |
| crates/motion-host/src/session.rs:680 | 核实保留 | acquire与present计时为同一began包围的串行不重叠区间，正常微秒和不超过wall；饱和减仅稳健计时夹限，u64微秒量级远超实际进程寿命。as_micros收窄的极端长期回绕不据此声称逐位无损，不将正常计时与GPU驻留少计混同 |
| crates/motion-host/src/session.rs:763 | 已有覆盖 | 已有host surface FailureCounter：Lost/Outdated重配置及Timeout保持Ok(None)，失败首Debug/连续60次Error，成功获取复位，携带surface状态；不是每帧日志，本批未模拟真实驱动Lost/Outdated故障 |
| crates/motion-host/src/session.rs:822 | 新增观测 | §66 resource_info 对根目录canonicalize失败累计报告次数，首Warn/60Error/抑制，所有根可解析才复位；解锁后带无法解析数量、首session/路径/系统原因/耗时。原报告仍排除该session，无法确定其是否属于请求目录，明确标记membership unknown；真实缺失目录回归，不修复聚合结果 |
| crates/motion-host/src/video_cache.rs:69 | 核实保留 | FrameCache私有frames/bytes，同一不可变Arc帧先精确累计、移除时减同一bytes，retain_window重新求和，budget≤36MiB且≤5帧；当前没有可独立修改账本的外部路径。保留真实cache窗口/驱逐回归，不当作已重现统计漂移 |
| crates/motion-host/src/video_decode_policy.rs:14 | 核实保留 | wanted<=last已提前返回needs_seek，后面的wanted-last为正、饱和不会触发；submitted_until决定排空或seek，保留真实前进/回退策略测试 |
| crates/motion-host/src/video_frames.rs:139 | 已有覆盖 | 此前panic_reason保留worker panic载荷，WorkerObservation及DecodeFlight发布open/decode开始和在途状态，错误带object/source/sequence/PTS/phase/elapsed，stdout安静；已有真实受控panic/阻塞解码回归 |
| crates/motion-host/src/video_frames.rs:144 | 功能边界 | VideoDecoder.seeks定义累计seek数，同一decoder在worker内保留，正常后端计数单调；公开trait可实现递减或u64回绕，当前delta饱和0且无独立计数回退观测。保留数值/metrics边界，不声称所有第三方后端遵约或重置已被定位 |
| crates/motion-host/src/video_frames.rs:363 | 已有覆盖 | 当前generation返回Result，中毒状态明确Err并保留object，不再用.ok()?制造新generation；计数checked_add拒绝耗尽。沿用poisoned_video_state_is_an_error_without_fabricating_a_new_generation等回归，不修改既有替换媒体退休stream顺序 |
| crates/motion-host/src/video_frames.rs:501 | 已有覆盖 | 此前Stream.request对pending轮询有计数、首Debug/第60次Error和in_flight phase/elapsed，成功/换generation复位；仍Ok(None)给调用方重试，不对每次正常pending无条件日志 |

## B3 · 桌面引擎、音频与应用回退（122 项）

逐项使用原始path:line关联当前生产入口。新增观测 39、核实保留 51、已有覆盖 28、功能边界 4。正常缺省/导航不当作故障，功能边界保持原行为；新增代码和实际验证见完成记录§68–70

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| desktop/app/src/audio_output.rs:102 | 新增观测 | §68 fail仍保留第一个错误，锁中毒仅向固定原子事件发布生命周期恢复次数，watchdog循环/退出时排空后记录；UI error读取用recovered保持into_inner。真实中毒/120次写入及第一原因保留回归；未向实时callback加入日志/IO |
| desktop/app/src/audio_output.rs:111 | 核实保留 | watchdog的now与callback心跳来自同一Instant epoch；跨线程读取时心跳可能晚于本次捕获now，此时saturating_sub为0避免假超时。实际超过2s仍fail，已有真实无callback和持续callback回归，不把并发顺序差当作时钟倒退 |
| desktop/app/src/audio_output.rs:147 | 核实保留 | device_frames_at以输出延迟锚点作前后外推，now早于未来playback锚点时减法夹零是合法预滚，随后min(submitted)、observed_frames.fetch_max保持单调；加法饱和亦先夹到已提交帧，不丢内部资源记账 |
| desktop/app/src/audio_output.rs:149 | 核实保留 | device_frames_at以输出延迟锚点作前后外推，now早于未来playback锚点时减法夹零是合法预滚，随后min(submitted)、observed_frames.fetch_max保持单调；加法饱和亦先夹到已提交帧，不丢内部资源记账 |
| desktop/app/src/audio_output.rs:187 | 核实保留 | 音频开始位置从调用方播放帧换算为非负sample，播放结束另以total_samples限制；保留既有起点夹取及设备时钟契约。任意公开手工非法Project不由此转换证明有效 |
| desktop/app/src/audio_output.rs:212 | 核实保留 | 未找到48k支持配置时改用设备默认配置是有意的设备能力选择；默认配置查询仍传播Err，随后检查rate/channels/format并由Producer重采样。不是supported_output_configs的Err被吞掉 |
| desktop/app/src/audio_output.rs:284 | 新增观测 | §68 fail仍保留第一个错误，锁中毒仅向固定原子事件发布生命周期恢复次数，watchdog循环/退出时排空后记录；UI error读取用recovered保持into_inner。真实中毒/120次写入及第一原因保留回归；未向实时callback加入日志/IO |
| desktop/app/src/audio_output.rs:302 | 已有覆盖 | stop仅join已结束worker且现已Error记录panic_reason；producer在next之前Info start/flush，panic保留payload与阶段、elapsed，EOS/stop有结束原因。沿用producer真实阻塞/初始化panic/解码失败回归，不把未结束worker detach视为join成功 |
| desktop/app/src/audio_output.rs:325 | 已有覆盖 | stop仅join已结束worker且现已Error记录panic_reason；producer在next之前Info start/flush，panic保留payload与阶段、elapsed，EOS/stop有结束原因。沿用producer真实阻塞/初始化panic/解码失败回归，不把未结束worker detach视为join成功 |
| desktop/app/src/audio_output.rs:332 | 核实保留 | Producer达到经项目时长计算的total后明确EOS；shared.done/ready和observation.stopped("eof")区分结束与未就绪。回调尾部静音是正常播放终态，不把Ok(None)一概当作失败 |
| desktop/app/src/audio_output.rs:399 | 新增观测 | §68 callback_latency遇到CPAL playback早于callback仍回退0，仅发布固定原子计数/首与60阈值位，watchdog解锁后Warn/Error并抑制；成功时间戳复位，同一轮poll前多episode会合并。受控None经过实际转换入口验证零延迟与两次episode，未声称自然制造驱动倒序 |
| desktop/app/src/audio_output.rs:453 | 核实保留 | callback的now+latency饱和保护u64纳秒时钟，实际Instant epoch及设备延迟远低于约584年；原始函数不涉及显存记账。保留现有时间接口，无新增正常回调日志 |
| desktop/app/src/audio_output.rs:500 | 核实保留 | Producer output_index由项目总帧数、48k源采样及设备rate计算，u128中间乘法避免溢出；原商是源sample索引且在有效项目时长内可用u64。任意伪造巨大内部index不在此有效路径证明内 |
| desktop/app/src/audio_output.rs:502 | 核实保留 | Producer达到经项目时长计算的total后明确EOS；shared.done/ready和observation.stopped("eof")区分结束与未就绪。回调尾部静音是正常播放终态，不把Ok(None)一概当作失败 |
| desktop/app/src/audio_output.rs:505 | 核实保留 | remainder先对设备rate取模，再转换为f32插值比例，结果在0..1范围；这是PCM线性重采样的既定浮点精度，不作为整数身份标识丢位或静默Err |
| desktop/app/src/cursors.rs:49 | 核实保留 | Default/Text/Wait本来使用系统cursor，CUSTOM仅十种交互光标且按prepare缓存；未找到自定义图像时选择同语义系统cursor属刻意兼容策略，不是资源加载Err。保留游标形状及DPI缓存 |
| desktop/app/src/diagnostics.rs:103 | 已有覆盖 | started_unix_ms与micros现先min(u64::MAX)再转换，report暴露始点与elapsed；已无原始无保护u128收窄。约5.8亿年级别毫秒边界不新增热点日志 |
| desktop/app/src/diagnostics.rs:115 | 已有覆盖 | invalidDSN现Warn且不回显凭据；reporter spawn失败增加io_errors并Error。reporter工作前Info/flush，写入失败含阶段和路径且请求有结果；沿用实际文件写失败/进程checkpoint回归，OS spawn失败未自然复现 |
| desktop/app/src/diagnostics.rs:117 | 已有覆盖 | invalidDSN现Warn且不回显凭据；reporter spawn失败增加io_errors并Error。reporter工作前Info/flush，写入失败含阶段和路径且请求有结果；沿用实际文件写失败/进程checkpoint回归，OS spawn失败未自然复现 |
| desktop/app/src/diagnostics.rs:125 | 新增观测 | 原context/set_context/set_gpu已非try_lock丢弃；§68对现有Mutex中毒恢复按固定进程site记录首Warn/第60次Error，业务guard释放后才写日志。真实context/state中毒回归验证字段与report保留；GPU同一recovered实现，未伪造自然适配器故障 |
| desktop/app/src/diagnostics.rs:127 | 新增观测 | 原context/set_context/set_gpu已非try_lock丢弃；§68对现有Mutex中毒恢复按固定进程site记录首Warn/第60次Error，业务guard释放后才写日志。真实context/state中毒回归验证字段与report保留；GPU同一recovered实现，未伪造自然适配器故障 |
| desktop/app/src/diagnostics.rs:130 | 新增观测 | 原context/set_context/set_gpu已非try_lock丢弃；§68对现有Mutex中毒恢复按固定进程site记录首Warn/第60次Error，业务guard释放后才写日志。真实context/state中毒回归验证字段与report保留；GPU同一recovered实现，未伪造自然适配器故障 |
| desktop/app/src/diagnostics.rs:135 | 新增观测 | 原context/set_context/set_gpu已非try_lock丢弃；§68对现有Mutex中毒恢复按固定进程site记录首Warn/第60次Error，业务guard释放后才写日志。真实context/state中毒回归验证字段与report保留；GPU同一recovered实现，未伪造自然适配器故障 |
| desktop/app/src/diagnostics.rs:204 | 新增观测 | 原context/set_context/set_gpu已非try_lock丢弃；§68对现有Mutex中毒恢复按固定进程site记录首Warn/第60次Error，业务guard释放后才写日志。真实context/state中毒回归验证字段与report保留；GPU同一recovered实现，未伪造自然适配器故障 |
| desktop/app/src/diagnostics.rs:96 | 已有覆盖 | invalidDSN现Warn且不回显凭据；reporter spawn失败增加io_errors并Error。reporter工作前Info/flush，写入失败含阶段和路径且请求有结果；沿用实际文件写失败/进程checkpoint回归，OS spawn失败未自然复现 |
| desktop/app/src/dialogs.rs:308 | 核实保留 | dialog只消费其关心的Action/按键，其余由既有事件路由忽略；不是Result错误分支，不改键盘焦点或引入无关事件噪声 |
| desktop/app/src/dialogs.rs:329 | 核实保留 | 字体候选滚动max=len-5、向上scroll-1使用饱和处理短列表及顶部，是正常导航下界；不涉及资源计数，保留既有输入交互 |
| desktop/app/src/dialogs.rs:343 | 核实保留 | 字体候选滚动max=len-5、向上scroll-1使用饱和处理短列表及顶部，是正常导航下界；不涉及资源计数，保留既有输入交互 |
| desktop/app/src/dialogs.rs:345 | 核实保留 | 字体候选滚动max=len-5、向上scroll-1使用饱和处理短列表及顶部，是正常导航下界；不涉及资源计数，保留既有输入交互 |
| desktop/app/src/dialogs/composition.rs:160 | 核实保留 | 这里解析尚在编辑中的输入作时码显示/临时aspect基线，失败保留文本或旧aspect；最终submit另验证字段并显示错误，未把默认值写入工程。无效中间输入是正常编辑状态，不记录用户文本 |
| desktop/app/src/dialogs/composition.rs:199 | 核实保留 | 这里解析尚在编辑中的输入作时码显示/临时aspect基线，失败保留文本或旧aspect；最终submit另验证字段并显示错误，未把默认值写入工程。无效中间输入是正常编辑状态，不记录用户文本 |
| desktop/app/src/dialogs/composition.rs:200 | 核实保留 | 这里解析尚在编辑中的输入作时码显示/临时aspect基线，失败保留文本或旧aspect；最终submit另验证字段并显示错误，未把默认值写入工程。无效中间输入是正常编辑状态，不记录用户文本 |
| desktop/app/src/dialogs/composition.rs:307 | 核实保留 | composition弹窗对未处理Key保留无操作，文本/提交/关闭各有显式分支；不是枚举错误或失败被清除，不对普通键盘事件告警 |
| desktop/app/src/dialogs/composition.rs:312 | 核实保留 | 这里解析尚在编辑中的输入作时码显示/临时aspect基线，失败保留文本或旧aspect；最终submit另验证字段并显示错误，未把默认值写入工程。无效中间输入是正常编辑状态，不记录用户文本 |
| desktop/app/src/engine.rs:1298 | 已有覆盖 | PRESENT_SURFACE_FAILURES对Lost/Outdated/Timeout持续失败计数，configure后仍保留Ok(false)，60次Error/成功复位，Present热点不无条件记录start/done。headless条件不能自然保证复现驱动Lost/Timeout，沿用受控surface回归与实际健康Present安静验证 |
| desktop/app/src/engine.rs:1299 | 已有覆盖 | PRESENT_SURFACE_FAILURES对Lost/Outdated/Timeout持续失败计数，configure后仍保留Ok(false)，60次Error/成功复位，Present热点不无条件记录start/done。headless条件不能自然保证复现驱动Lost/Timeout，沿用受控surface回归与实际健康Present安静验证 |
| desktop/app/src/engine.rs:1312 | 核实保留 | last_present_us的u128→u64微秒需要单次Present持续约58万年才越界；慢Present仍按100ms门控Debug，挂起由handler前InFlight/start证据定位。保留计时接口，不混同现实丢帧 |
| desktop/app/src/engine.rs:1320 | 新增观测 | §68 paint_pool中毒仍遵守原策略：take返回空PaintList、recycle丢弃，不改为恢复缓存；三个固定进程生命周期site首Warn/60Error，match释放PoisonError内guard后日志。真实Engine中毒120次验证空返回/不入池，worker回收共用真实helper；未伪称驱动Present自然中毒 |
| desktop/app/src/engine.rs:1351 | 已有覆盖 | Engine Drop的join及freeze worker catch_unwind现保留motion_media::panic_reason并Error；freeze worker Info start/flush先于冻结与通道等待。panic hook另含栈，既有真实panic/终态回归 |
| desktop/app/src/engine.rs:1436 | 已有覆盖 | 当前capabilities_request使用parse_request?及serde_json::from_value.map_err?，非法请求明确Err并由worker request/error日志关联；已无.ok().unwrap_or_default查询降级，现有MCP非法请求回归 |
| desktop/app/src/engine.rs:1439 | 已有覆盖 | 当前capabilities_request使用parse_request?及serde_json::from_value.map_err?，非法请求明确Err并由worker request/error日志关联；已无.ok().unwrap_or_default查询降级，现有MCP非法请求回归 |
| desktop/app/src/engine.rs:384 | 新增观测 | §68 paint_pool中毒仍遵守原策略：take返回空PaintList、recycle丢弃，不改为恢复缓存；三个固定进程生命周期site首Warn/60Error，match释放PoisonError内guard后日志。真实Engine中毒120次验证空返回/不入池，worker回收共用真实helper；未伪称驱动Present自然中毒 |
| desktop/app/src/engine.rs:386 | 新增观测 | §68 paint_pool中毒仍遵守原策略：take返回空PaintList、recycle丢弃，不改为恢复缓存；三个固定进程生命周期site首Warn/60Error，match释放PoisonError内guard后日志。真实Engine中毒120次验证空返回/不入池，worker回收共用真实helper；未伪称驱动Present自然中毒 |
| desktop/app/src/engine.rs:390 | 新增观测 | §68 paint_pool中毒仍遵守原策略：take返回空PaintList、recycle丢弃，不改为恢复缓存；三个固定进程生命周期site首Warn/60Error，match释放PoisonError内guard后日志。真实Engine中毒120次验证空返回/不入池，worker回收共用真实helper；未伪称驱动Present自然中毒 |
| desktop/app/src/engine.rs:443 | 核实保留 | 未设置MOTION_DIAGNOSTICS_PATH时选择应用目录诊断文件是正常配置默认；new/report/export的真实IO失败已有独立观测，不把未设置环境变量当作故障 |
| desktop/app/src/engine.rs:471 | 已有覆盖 | worker startup各失败在ready.send前Error，初始snapshot失败不再.ok吞掉；ready接收端退出则释放SessionLease。原请求契约允许对已退出接收者丢弃Err，panic hook/worker终态另覆盖，respond保持不变 |
| desktop/app/src/engine.rs:486 | 已有覆盖 | worker startup各失败在ready.send前Error，初始snapshot失败不再.ok吞掉；ready接收端退出则释放SessionLease。原请求契约允许对已退出接收者丢弃Err，panic hook/worker终态另覆盖，respond保持不变 |
| desktop/app/src/engine.rs:491 | 已有覆盖 | worker startup各失败在ready.send前Error，初始snapshot失败不再.ok吞掉；ready接收端退出则释放SessionLease。原请求契约允许对已退出接收者丢弃Err，panic hook/worker终态另覆盖，respond保持不变 |
| desktop/app/src/engine.rs:494 | 已有覆盖 | 当前Engine::start的total_memory为u64并直接传configure_memory，原u64 as i64收窄已不存在；预算配置失败有startup Error与ready Err。未把天文级内存值视为本轮新增功能修复 |
| desktop/app/src/engine.rs:495 | 已有覆盖 | worker startup各失败在ready.send前Error，初始snapshot失败不再.ok吞掉；ready接收端退出则释放SessionLease。原请求契约允许对已退出接收者丢弃Err，panic hook/worker终态另覆盖，respond保持不变 |
| desktop/app/src/engine.rs:533 | 新增观测 | §68终态发布failure与退出factory.take改用recovered，into_inner和资源释放原策略保持，先释放业务guard再记录固定site生命周期首Warn/60Error；公共helper真实锁中毒与unwind/try_lock探针回归，不声称自然触发worker退出factory中毒 |
| desktop/app/src/engine.rs:542 | 新增观测 | §68终态发布failure与退出factory.take改用recovered，into_inner和资源释放原策略保持，先释放业务guard再记录固定site生命周期首Warn/60Error；公共helper真实锁中毒与unwind/try_lock探针回归，不声称自然触发worker退出factory中毒 |
| desktop/app/src/engine.rs:68 | 功能边界 | FrameStamp将UI viewport ceil转换为缓存尺寸，正规的winit物理窗口到逻辑尺寸为有限正值；手工UiFrame含NaN/极大rect的独立入口仍能发生饱和与缓存键碰撞。本批不改键协议或补全任意浮点输入校验，不标为全输入已修复 |
| desktop/app/src/engine.rs:992 | 已有覆盖 | Engine Drop的join及freeze worker catch_unwind现保留motion_media::panic_reason并Error；freeze worker Info start/flush先于冻结与通道等待。panic hook另含栈，既有真实panic/终态回归 |
| desktop/app/src/engine/failure.rs:109 | 新增观测 | §68 check_running/stopped_error中毒保持into_inner和原始终态原因，固定site生命周期Warn1/Error60；真实Engine的failure锁中毒、空值/原始终态值回归，以及解锁再日志探针 |
| desktop/app/src/engine/failure.rs:126 | 新增观测 | §68 check_running/stopped_error中毒保持into_inner和原始终态原因，固定site生命周期Warn1/Error60；真实Engine的failure锁中毒、空值/原始终态值回归，以及解锁再日志探针 |
| desktop/app/src/engine/failure.rs:128 | 核实保留 | 仅当worker结束且尚无更具体终态原因时提供 reopen project 通用提示；已发布原因优先，poll_reply断连另Error，属于终态兜底，不声称能恢复未发布的不可获知原因 |
| desktop/app/src/main.rs:140 | 核实保留 | winit EventLoopProxy::send_event的失败表示event loop已关闭，EngineChanged唤醒是退出阶段best effort；活跃loop排队由winit负责。保留无接收者终止协议，不制造每命令关闭日志 |
| desktop/app/src/main.rs:290 | 核实保留 | 分别为缺省CLI标志、缺省project/layout路径与无关WindowEvent：可选配置和事件路由正常契约；需要参数但缺值仍由CLI Err及desktop failed日志处理，不把未使用事件当成静默失败 |
| desktop/app/src/main.rs:292 | 新增观测 | §68启动恢复send_diagnostics偏好若无可用endpoint，Warn记录enabled及拒绝原因；set_enabled原Err和disabled状态保留。无效DSN另有去凭据Warn，不自动启用上传 |
| desktop/app/src/main.rs:537 | 核实保留 | 分别为缺省CLI标志、缺省project/layout路径与无关WindowEvent：可选配置和事件路由正常契约；需要参数但缺值仍由CLI Err及desktop failed日志处理，不把未使用事件当成静默失败 |
| desktop/app/src/main.rs:64 | 核实保留 | 分别为缺省CLI标志、缺省project/layout路径与无关WindowEvent：可选配置和事件路由正常契约；需要参数但缺值仍由CLI Err及desktop failed日志处理，不把未使用事件当成静默失败 |
| desktop/app/src/main.rs:75 | 核实保留 | 分别为缺省CLI标志、缺省project/layout路径与无关WindowEvent：可选配置和事件路由正常契约；需要参数但缺值仍由CLI Err及desktop failed日志处理，不把未使用事件当成静默失败 |
| desktop/app/src/main.rs:788 | 核实保留 | request_inner_size返回Option<PhysicalSize>表示同步已完成或异步等待Resized，不是Result失败；smoke发起resize后继续真实事件循环验证，不能把None当作拒绝窗口操作 |
| desktop/app/src/mcp.rs:164 | 核实保留 | JSON-RPC缺省params与tool arguments按原协议提供空object，后续工具/schema继续校验必需字段；不向stdout写日志，不改变MCP工具契约 |
| desktop/app/src/mcp.rs:211 | 核实保留 | JSON-RPC缺省params与tool arguments按原协议提供空object，后续工具/schema继续校验必需字段；不向stdout写日志，不改变MCP工具契约 |
| desktop/app/src/mcp.rs:237 | 核实保留 | read_line在EOF且无未处理字节时Ok(None)明确结束服务，不是没有数据的暂态；MCP服务start/stop以及非法JSON/IO分别有日志，实际EOF进程验收 |
| desktop/app/src/movie_export.rs:45 | 已有覆盖 | catch_unwind现用panic_reason保留payload，start Info/flush先于snapshot.export，最终failed/cancelled/succeeded含revision/path/elapsed。未开启ffmpeg明确Err；不声称这台机器自然发生编码器panic |
| desktop/app/src/movie_export.rs:47 | 新增观测 | §68进度/最终状态/读取三个固定site的中毒沿用into_inner，统一recovered释放status guard后发生命周期Warn1/Error60。公共真实中毒/unwind探针覆盖guard顺序；未将受控helper等同完整FFmpeg中毒重现 |
| desktop/app/src/movie_export.rs:50 | 已有覆盖 | catch_unwind现用panic_reason保留payload，start Info/flush先于snapshot.export，最终failed/cancelled/succeeded含revision/path/elapsed。未开启ffmpeg明确Err；不声称这台机器自然发生编码器panic |
| desktop/app/src/movie_export.rs:51 | 新增观测 | §68进度/最终状态/读取三个固定site的中毒沿用into_inner，统一recovered释放status guard后发生命周期Warn1/Error60。公共真实中毒/unwind探针覆盖guard顺序；未将受控helper等同完整FFmpeg中毒重现 |
| desktop/app/src/movie_export.rs:74 | 新增观测 | §68进度/最终状态/读取三个固定site的中毒沿用into_inner，统一recovered释放status guard后发生命周期Warn1/Error60。公共真实中毒/unwind探针覆盖guard顺序；未将受控helper等同完整FFmpeg中毒重现 |
| desktop/app/src/node_editor.rs:1003 | 已有覆盖 | set_operation构建port map现逐node match，Err Warn记录edited_node/node/revision/原原因，保留该节点被过滤与兼容connection pruning，不改变编辑功能。该路径按用户编辑触发，不是每帧热日志 |
| desktop/app/src/node_editor.rs:1191 | 新增观测 | §69 arrange_without_history验证失败用固定editor/site生命周期观测，Warn1/Error60保留epoch/revision/原因，仍不改positions/history；真实Unsupported节点120次布局验证回归 |
| desktop/app/src/node_preview.rs:560 | 功能边界 | clock uniform的seconds/frame是既定f32 shader ABI，长时间时间精度会降低；不是JSON身份ID。未扩展双精度或重置时钟，不把日志工作算成长时精度修复 |
| desktop/app/src/node_preview.rs:561 | 功能边界 | clock uniform的seconds/frame是既定f32 shader ABI，长时间时间精度会降低；不是JSON身份ID。未扩展双精度或重置时钟，不把日志工作算成长时精度修复 |
| desktop/app/src/node_preview.rs:608 | 功能边界 | 正常viewport由物理窗口/逻辑rect生成，GPU切片范围仍受实际纹理限制；公开手工UiFrame的负值/NaN/无穷rect转u32的完整拒绝尚未加入。本批不改scissor/viewport行为，保留外部调用边界 |
| desktop/app/src/node_tasks.rs:215 | 已有覆盖 | node worker已有Info start/flush，catch_unwind保留panic_reason并发布Outcome::Failed，完成/失败含job/revision/elapsed，异步结果断连另有观测。原generic-only路径已不存在 |
| desktop/app/src/node_tasks.rs:218 | 已有覆盖 | node worker已有Info start/flush，catch_unwind保留panic_reason并发布Outcome::Failed，完成/失败含job/revision/elapsed，异步结果断连另有观测。原generic-only路径已不存在 |
| desktop/app/src/node_tasks.rs:524 | 核实保留 | 把worker status.error转换为Outcome::Failed是终态类型适配，非Some(None)吞Err；缺失具体原因才使用通用failed文本，worker执行起点与原始故障在独立日志中保留 |
| desktop/app/src/project_lifecycle.rs:103 | 新增观测 | §68工程切换成功后remember_project失败记录Error，保留已替换state与原有继续流程，不回滚工程或改变提示结构；记录changed=true和原原因 |
| desktop/app/src/project_lifecycle.rs:110 | 核实保留 | cancel_interaction返回旧工程待发送Action序列，不是Result；注释明确旧模型已替换，故意丢弃旧撤销命令以免影响新工程。仍清UI手柄，不改历史契约 |
| desktop/app/src/project_lifecycle.rs:124 | 已有覆盖 | 当前match project_ticket失败Error记录root、旧revision、retained_stale_ticket和主操作原因；保留prompt与用户重选契约，既有工程竞态回归 |
| desktop/app/src/shell/acceptance.rs:192 | 核实保留 | 未提供自定义sheet时使用内建default_stylesheet是正常主题选择；加载override失败独立被样式入口观测，不是从Err返回默认sheet |
| desktop/app/src/shell/acceptance.rs:515 | 核实保留 | 截图测试elapsed微秒到u64需约58万年单次执行才超界；保留测试报告精度与原断言，不加热点告警 |
| desktop/app/src/shell/acceptance.rs:571 | 核实保留 | 未提供自定义sheet时使用内建default_stylesheet是正常主题选择；加载override失败独立被样式入口观测，不是从Err返回默认sheet |
| desktop/app/src/shell/acceptance.rs:598 | 核实保留 | 仅smoke截图流程绘制floating_chrome不消费交互Action，Input不含真实编辑请求；实际运行Shell仍路由Action。验收采样不是丢弃产品操作Err |
| desktop/app/src/shell/actions.rs:367 | 核实保留 | 未选择override时采用默认样式属正常可选配置；不是加载Err被吞掉，已有style override失效保留旧sheet回归 |
| desktop/app/src/shell/actions.rs:407 | 新增观测 | §68原drag_window的OS Result失败现在Warn保留原因，仍丢弃结果继续原操作，不新增交互提示/重试或布局变化 |
| desktop/app/src/shell/audio_analysis.rs:278 | 新增观测 | §68 cancel/release异步request入队失败单独Error附operation/request_id，同时保留取消UI与无回复等待的既有语义；成功入队后的业务失败由worker error记录，respond语义不变 |
| desktop/app/src/shell/audio_analysis.rs:387 | 已有覆盖 | 当前release request后已使用?传播派发失败；discard的是返回Receiver，因为生命周期清理不等待结果，worker仍记录执行Err，不改变协议的无人接收回复 |
| desktop/app/src/shell/audio_analysis.rs:423 | 核实保留 | 音频检测时间映射后先filter begin<=f<end，再round结果形成marker；begin/end来自合法clip/project帧范围，u32可表示。不把过滤到范围外的参考点当作decode失败 |
| desktop/app/src/shell/audio_analysis.rs:445 | 已有覆盖 | 未知准备状态当前由checked_task_state校验，match兜底明确Err包含request/state；不再空分支，running是正常等待，failed/cancelled保持原原因 |
| desktop/app/src/shell/design_acceptance.rs:433 | 核实保留 | 真实window比率截图后裁剪既定panel区域，panel_size/window_size来自非零可见窗口；u32为image crop像素接口，图像读取/保存仍?。仅验收流程，不修改产品尺寸/布局 |
| desktop/app/src/shell/design_acceptance.rs:434 | 核实保留 | 真实window比率截图后裁剪既定panel区域，panel_size/window_size来自非零可见窗口；u32为image crop像素接口，图像读取/保存仍?。仅验收流程，不修改产品尺寸/布局 |
| desktop/app/src/shell/lifecycle.rs:19 | 新增观测 | §68 close及close prompt的node_expect派发失败现Error记录operation和原因，仍继续既有关闭/提示流程，既有节点关闭竞态测试保持 |
| desktop/app/src/shell/lifecycle.rs:81 | 新增观测 | §68 close及close prompt的node_expect派发失败现Error记录operation和原因，仍继续既有关闭/提示流程，既有节点关闭竞态测试保持 |
| desktop/app/src/shell/media.rs:32 | 核实保留 | 图片导入起点来自session当前帧floor，正规snapshot经host范围校验；workflow亦min(project.frames-1)。任意手工损坏的JSON不由该cast证明有效，不改导入落点 |
| desktop/app/src/shell/media.rs:57 | 核实保留 | TryRecvError::Empty表示异步媒体请求尚未完成，保留receiver稍后poll；Disconnected/Err已有失败分支。正常未就绪不产生每帧日志 |
| desktop/app/src/shell/nodes.rs:275 | 核实保留 | 保存绑定节点时validate().is_ok控制是否等待canonical install，非法或尚未可编译的草稿仍可走store_node_source存储，属于允许保存草稿的契约；用户显式Validate/Compile另保留Diagnostic，不强制拒绝保存 |
| desktop/app/src/shell/nodes.rs:372 | 已有覆盖 | 安装后state/catalogue刷新现match失败Error，包含installed=true/phase/epoch/revision及原原因，保留上次快照，不将安装成功改成失败 |
| desktop/app/src/shell/nodes.rs:375 | 已有覆盖 | 安装后state/catalogue刷新现match失败Error，包含installed=true/phase/epoch/revision及原原因，保留上次快照，不将安装成功改成失败 |
| desktop/app/src/shell/nodes.rs:428 | 已有覆盖 | node source receiver断连的细节由worker start/panic hook/engine failure日志保留，poll的generic断连仍经后续Err日志/报告输出。通道本身不携带panic payload，不能声称从Disconnected中恢复原始载荷 |
| desktop/app/src/shell/nodes.rs:62 | 核实保留 | 新建未绑定source没有key时生成时间戳ID是正常创建策略；已绑定key优先，不是解析Err丢失。SystemTime失败仍传播Err |
| desktop/app/src/shell/presentation.rs:265 | 新增观测 | §68取消/过期close后schedule_node_compile失败现Error保留operation及原因，仍保持原不传播返回值的恢复行为，不自动重试 |
| desktop/app/src/shell/presentation.rs:271 | 新增观测 | §68取消/过期close后schedule_node_compile失败现Error保留operation及原因，仍保持原不传播返回值的恢复行为，不自动重试 |
| desktop/app/src/shell/presentation.rs:511 | 核实保留 | winit request_inner_size的None表示异步尺寸请求，与Err无关；最终Resized驱动原GPU resize与工作区geometry。保留窗口恢复行为 |
| desktop/app/src/shell/presentation.rs:721 | 新增观测 | §68 floating位置查询与move/resize持久化两个固定进程连续episode，失败Warn1/Error60，成功复位，日志带window或path及原因；原None位置/内存内geometry保留，不加入UI反馈或写盘重试 |
| desktop/app/src/shell/presentation.rs:733 | 新增观测 | §68 floating位置查询与move/resize持久化两个固定进程连续episode，失败Warn1/Error60，成功复位，日志带window或path及原因；原None位置/内存内geometry保留，不加入UI反馈或写盘重试 |
| desktop/app/src/shell/presentation.rs:763 | 新增观测 | §68 floating位置查询与move/resize持久化两个固定进程连续episode，失败Warn1/Error60，成功复位，日志带window或path及原因；原None位置/内存内geometry保留，不加入UI反馈或写盘重试 |
| desktop/app/src/shell/styles.rs:62 | 新增观测 | §68所选文件metadata/modified失败有每StyleSource连续stamp计数，成功/未选文件复位；reload另独立计数，Info start先于解析/font，Warn1/Error60抑制，成功复位且保留旧sheet与原消息。真实缺文件/坏JSON/修复回归；modified不支持未自然重现 |
| desktop/app/src/shell/styles.rs:63 | 新增观测 | §68所选文件metadata/modified失败有每StyleSource连续stamp计数，成功/未选文件复位；reload另独立计数，Info start先于解析/font，Warn1/Error60抑制，成功复位且保留旧sheet与原消息。真实缺文件/坏JSON/修复回归；modified不支持未自然重现 |
| desktop/app/src/shell/styles.rs:91 | 新增观测 | §68所选文件metadata/modified失败有每StyleSource连续stamp计数，成功/未选文件复位；reload另独立计数，Info start先于解析/font，Warn1/Error60抑制，成功复位且保留旧sheet与原消息。真实缺文件/坏JSON/修复回归；modified不支持未自然重现 |
| desktop/app/src/workflow.rs:14 | 核实保留 | image_info来自实际image decoder尺寸u32转换为JSON u64，且有编码格式/像素及资源预算检查；此处u64读回u32不截断正常decoder输出。不是任意外部提供的JSON宽高 |
| desktop/app/src/workflow.rs:17 | 核实保留 | image_info来自实际image decoder尺寸u32转换为JSON u64，且有编码格式/像素及资源预算检查；此处u64读回u32不截断正常decoder输出。不是任意外部提供的JSON宽高 |
| desktop/app/src/workflow.rs:33 | 核实保留 | 导入落点显式floor并min(project.frames-1)，项目frames正且受模型校验；保留既有取当前帧/末帧夹取语义，不改变导入历史 |
| desktop/app/src/workspace.rs:88 | 新增观测 | §68应用Workspace::load与UI框架不同：读全文件后保留64KiB限制、JSON/ownership检查及migrate_design；NotFound正常安静，其他失败固定连续site Warn1/Error60，成功/正常缺文件复位。坏文件不重写，实际FS/阈值/恢复回归 |
| desktop/app/src/workspace.rs:90 | 新增观测 | §68应用Workspace::load与UI框架不同：读全文件后保留64KiB限制、JSON/ownership检查及migrate_design；NotFound正常安静，其他失败固定连续site Warn1/Error60，成功/正常缺文件复位。坏文件不重写，实际FS/阈值/恢复回归 |
| desktop/app/src/workspace.rs:96 | 新增观测 | §68应用Workspace::load与UI框架不同：读全文件后保留64KiB限制、JSON/ownership检查及migrate_design；NotFound正常安静，其他失败固定连续site Warn1/Error60，成功/正常缺文件复位。坏文件不重写，实际FS/阈值/恢复回归 |
| desktop/app/src/workspace/manager.rs:158 | 核实保留 | TextEdit::input返回编辑响应/提交状态，不是Result；name.text在调用中更新，后续Save按钮消费名字并校验。忽略响应不等于丢弃按键Err，不改输入行为 |
| desktop/app/src/workspace/presets.rs:215 | 新增观测 | §68 select_preset中None缺省安静，Some(saved)反序列化失败由restore站点记录，存在但ownership/面板ID无效则preset.validate固定进程生命周期Warn1/Error60；保持.filter(valid)/默认preset回退。真实重复面板layout经实际select_preset 120次验证默认恢复、健康preset交错不重复告警 |
| desktop/app/src/workspace/presets.rs:78 | 新增观测 | §68 SavedLayout::restore反序列化失败固定进程生命周期首Warn/60Error保留原因及None回退；正常布局与相邻坏布局交错不重置，serde序列化原expect仍由panic hook观测，不改保存结构 |

## B4 · 面板与节点视图（159 项）

逐项使用原始path:line关联当前生产入口。核实保留 110、新增观测 13、功能边界 2、已有覆盖 34。正常缺省/导航不当作故障，功能边界保持原行为；新增代码和实际验证见完成记录§68–70

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| desktop/app/src/node_editor.rs:1058 | 核实保留 | check_node先用root_document求总output数，再减去被替换旧node的output数，检查新操作后总数<=1；合法被编辑node属于同根树，总数>=旧数，饱和仅保护计数下界，不是逐帧资源统计 |
| desktop/app/src/node_editor.rs:669 | 核实保留 | cancel_canvas_interaction只对Nodes恢复document、Marquee恢复选择；Wire/None取消无需撤销模型，drag.take已终止它们，通配是明确的无持久编辑分支 |
| desktop/app/src/node_editor.rs:895 | 核实保留 | connections.len.saturating_add(1)>max_connections为安全预算比较，达到usize上界仍拒绝超预算，不会将溢出伪装成0并放宽连接预算 |
| desktop/app/src/node_editor/inline.rs:407 | 核实保留 | controls按NodeOp生成有枚举选择的操作，其余Float/Color等由常量控件处理，没有对应choices时返回空列表为正常类型分流，Unsupported另有inspector日志，不增加无关控件 |
| desktop/app/src/node_editor/view/add_menu.rs:123 | 核实保留 | 菜单高亮按Up/Down饱和并在entries生成后min(len-1)，空候选另处理；这是正常键盘导航和过滤后修正，无Err与资源计数 |
| desktop/app/src/node_editor/view/add_menu.rs:42 | 核实保留 | 菜单高亮按Up/Down饱和并在entries生成后min(len-1)，空候选另处理；这是正常键盘导航和过滤后修正，无Err与资源计数 |
| desktop/app/src/node_editor/view/add_menu.rs:43 | 核实保留 | 菜单高亮按Up/Down饱和并在entries生成后min(len-1)，空候选另处理；这是正常键盘导航和过滤后修正，无Err与资源计数 |
| desktop/app/src/node_editor/view/add_menu.rs:44 | 核实保留 | 新增菜单只关心导航/确认/退出键，其余文本由query输入处理，通配不代表丢弃模型错误；保留键盘交互 |
| desktop/app/src/node_editor/view/canvas.rs:17 | 新增观测 | §69 cards.sockets及paint_canvas.labels各固定editor/site生命周期Warn1/Error60，原node_port_specs Err仍跳过端口，保留节点/文档/历史；原因与node有界。实际Unsupported节点通过真实cards/paint_canvas重复120次验证，不是只测logger |
| desktop/app/src/node_editor/view/canvas.rs:236 | 新增观测 | §69 cards.sockets及paint_canvas.labels各固定editor/site生命周期Warn1/Error60，原node_port_specs Err仍跳过端口，保留节点/文档/历史；原因与node有界。实际Unsupported节点通过真实cards/paint_canvas重复120次验证，不是只测logger |
| desktop/app/src/node_editor/view/canvas.rs:262 | 核实保留 | 公开组端口若无用户别名则使用PortSpec的中英文默认名是正常label回退；缺失端口查询本身现有独立观测，不将没有别名视为故障 |
| desktop/app/src/node_editor/view/canvas.rs:289 | 功能边界 | Parameter节点引用缺失parameter时标签回退ID仍保留；§69端口查询Err已可定位，未自动补回parameter/改文档或修改标签，悬挂引用功能边界不算修复 |
| desktop/app/src/node_editor/view/context_menu.rs:114 | 新增观测 | §69 context.ports观测真实node_port_specs失败，保留无端口菜单项的既有行为；固定editor/site生命周期Warn1/Error60，实际context菜单回归，不改菜单内容或顺序 |
| desktop/app/src/node_editor/view/context_menu.rs:164 | 核实保留 | context menu选中项按Up/Down饱和并min(entries-1)，可见start=selected-(rows-1)在顶部夹零是正常滚动窗口，不丢模型命令 |
| desktop/app/src/node_editor/view/context_menu.rs:165 | 核实保留 | context menu选中项按Up/Down饱和并min(entries-1)，可见start=selected-(rows-1)在顶部夹零是正常滚动窗口，不丢模型命令 |
| desktop/app/src/node_editor/view/context_menu.rs:166 | 核实保留 | 菜单未识别按键保持原状态，Enter/Escape/导航/鼠标各有显式处理；非错误通配，不改变快捷键路由 |
| desktop/app/src/node_editor/view/context_menu.rs:173 | 核实保留 | 可见rows=entries.len.min(max(1,按bounds高度计算的行数))，实际bounds为有限屏幕矩形且rows不超过菜单项；转换是现有滚动窗口计算，不产生未受限分配或丢失错误 |
| desktop/app/src/node_editor/view/context_menu.rs:181 | 核实保留 | context menu选中项按Up/Down饱和并min(entries-1)，可见start=selected-(rows-1)在顶部夹零是正常滚动窗口，不丢模型命令 |
| desktop/app/src/node_editor/view/control_popup.rs:115 | 核实保留 | 当前对应枚举popup的Up/Down项索引夹取及空列表防护，items变化后仍限制到合法范围；正常导航，不把0下界当作失败 |
| desktop/app/src/node_editor/view/control_popup.rs:116 | 核实保留 | 当前对应枚举popup的Up/Down项索引夹取及空列表防护，items变化后仍限制到合法范围；正常导航，不把0下界当作失败 |
| desktop/app/src/node_editor/view/inspector.rs:1047 | 核实保留 | apply_field先专门处理Constant/ParamDefault再进入其他Binding匹配，其余在这一提前分流中不做操作；真正匹配与参数校验在后续代码，未丢Diagnostic |
| desktop/app/src/node_editor/view/inspector.rs:181 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:229 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:255 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:286 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:299 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:332 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:349 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:384 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:424 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:474 | 已有覆盖 | 当前十个编辑触发点均调用record_inspector_edit，Diagnostic以Warn记录revision与原原因；set_operation/set_parameter_default验证失败保持模型和历史。已有实际inspector拒绝编辑回归，不是忽略let _ |
| desktop/app/src/node_editor/view/inspector.rs:563 | 核实保留 | NodeOp按适用属性生成控件，未匹配变体无需专用参数编辑；Unsupported已有inspector.unsupported观测，端口部分独立处理，不把无专用控件视为Err |
| desktop/app/src/node_editor/view/inspector.rs:565 | 新增观测 | §69 inspector.ports/help_ports分别记录真实查询失败，保留省略连接/帮助端口的分支，首Warn/60Error抑制。真实Unsupported节点draw_inspector回归，文档/revision不变 |
| desktop/app/src/node_editor/view/inspector.rs:791 | 新增观测 | §69 inspector.ports/help_ports分别记录真实查询失败，保留省略连接/帮助端口的分支，首Warn/60Error抑制。真实Unsupported节点draw_inspector回归，文档/revision不变 |
| desktop/app/src/node_editor/view/interaction.rs:137 | 核实保留 | 分别为搜索状态只处理Backspace/Escape/Enter及画布只消费本Scope快捷键，其他输入交由既有路由；不是失败返回，保留输入与焦点行为 |
| desktop/app/src/node_editor/view/interaction.rs:20 | 新增观测 | §69公共NodeEditor::request对commit_field拒绝按request.field固定site观测，Warn1/Error60记录request/Diagnostic code，不记录输入文本，保留Field.error/notice与Err、不生成Action；已有键盘真实拒绝UI回归及新增120次实际request阈值回归 |
| desktop/app/src/node_editor/view/interaction.rs:38 | 核实保留 | 分别为搜索状态只处理Backspace/Escape/Enter及画布只消费本Scope快捷键，其他输入交由既有路由；不是失败返回，保留输入与焦点行为 |
| desktop/app/src/node_editor/view/mod.rs:447 | 核实保留 | 状态栏优先notice/diagnostic，没有消息时显示节点帮助提示是正常空状态，不是错误原因被覆盖；显式Diagnostic仍优先 |
| desktop/app/src/node_editor/view/mod.rs:554 | 核实保留 | apply_field的Constant/ParamDefault提前返回，其余Binding进入后续专用逻辑；此通配是分阶段路由，后续Err由commit_field保留并由request记录，不是吞Err |
| desktop/app/src/node_editor/view/toolbar.rs:74 | 新增观测 | §69 toolbar request复用公共request.field观测，无需修改按钮或Action路由；保留无效字段阻止保存/编译和可见错误，原因code及request在默认级别可定位 |
| desktop/app/src/panels/align.rs:9 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/animation_controls.rs:109 | 核实保留 | selected_key为有符号合成时间，负的预滚key无法转换为KeySelection的u32时返回None是现有编辑范围契约；局部负关键帧仍可采样/展示，不通过日志修改其可选中性 |
| desktop/app/src/panels/animation_controls.rs:141 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/animation_controls.rs:313 | 核实保留 | 未显式选中key时选播放头之前最近key或第一项是既定导航，随后空keys单独显示NoKeys并return；不是Result失败 |
| desktop/app/src/panels/animation_controls.rs:334 | 核实保留 | 前一关键帧按钮selected.saturating_sub(1)同时enabled=selected>0，顶部禁用为正常导航，不是计数漂移 |
| desktop/app/src/panels/animation_controls.rs:403 | 已有覆盖 | ease/curve解析当前统一observed_easing，缺省字段按合法默认保持安静，非空坏值以固定site Warn1/Error60并保留Easing默认；真实坏曲线/缺省交错回归已存在 |
| desktop/app/src/panels/animation_controls.rs:404 | 已有覆盖 | ease/curve解析当前统一observed_easing，缺省字段按合法默认保持安静，非空坏值以固定site Warn1/Error60并保留Easing默认；真实坏曲线/缺省交错回归已存在 |
| desktop/app/src/panels/animation_controls.rs:615 | 核实保留 | 曲线拖拽的当前匹配处理Quadratic/Cubic，其他形状不属于该handle操作，保持原Easing；验证路径另有graph_handles.validation，不扩大曲线编辑功能 |
| desktop/app/src/panels/animation_data.rs:40 | 已有覆盖 | track_value/key_frame/key_range/key_decode均由observe!包装，保留None早退及原serde原因、offset/frame/key_index和类型；固定进程site生命周期Warn1/Error60，真实坏value/key/整数范围观测已有回归 |
| desktop/app/src/panels/animation_data.rs:49 | 已有覆盖 | track_value/key_frame/key_range/key_decode均由observe!包装，保留None早退及原serde原因、offset/frame/key_index和类型；固定进程site生命周期Warn1/Error60，真实坏value/key/整数范围观测已有回归 |
| desktop/app/src/panels/animation_data.rs:57 | 已有覆盖 | track_value/key_frame/key_range/key_decode均由observe!包装，保留None早退及原serde原因、offset/frame/key_index和类型；固定进程site生命周期Warn1/Error60，真实坏value/key/整数范围观测已有回归 |
| desktop/app/src/panels/audio.rs:227 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/audio.rs:273 | 新增观测 | §69 audio.level_track只对存在且格式错误的音量轨迹观测，原Track::constant(0)保持；缺省/null轨迹仍正常且安静。固定进程生命周期Warn1/Error60，真实坏轨迹与正常缺省交错120次回归 |
| desktop/app/src/panels/audio.rs:661 | 核实保留 | 音频key/marker拖动先按clip begin/last或project frames-1 clamp再转u32，起点与边界来自合法clip/project；保留取整/首尾限制，不是任意无界浮点输入 |
| desktop/app/src/panels/audio.rs:826 | 核实保留 | 音频key/marker拖动先按clip begin/last或project frames-1 clamp再转u32，起点与边界来自合法clip/project；保留取整/首尾限制，不是任意无界浮点输入 |
| desktop/app/src/panels/auxiliary.rs:747 | 核实保留 | auxiliary renderer仅处理其注册的Character/Paragraph等辅助面板，其余Panel由主分发器绘制，通配不产生该面板的内容是正常路由，不是业务错误 |
| desktop/app/src/panels/camera_controls.rs:243 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/effect_controls.rs:1384 | 已有覆盖 | effects.parameter/curve_track/curve_channel_parameter包装serde与缺失curve，缺轨None保留；channel额外校验有限、整数、1..5并观测原索引降级。固定site生命周期限频，已有实际效果曲线回归 |
| desktop/app/src/panels/effect_controls.rs:1403 | 已有覆盖 | effects.parameter/curve_track/curve_channel_parameter包装serde与缺失curve，缺轨None保留；channel额外校验有限、整数、1..5并观测原索引降级。固定site生命周期限频，已有实际效果曲线回归 |
| desktop/app/src/panels/effect_controls.rs:1553 | 功能边界 | enum dropdown的sample-min转usize保持既有floor/饱和与Dropdown绘制夹取，正常schema提供离散范围；任意伪造/动画产生的越界或非整数value未在此新增完整拒绝，不借观测声称所有枚举采样精度问题已修复 |
| desktop/app/src/panels/effect_controls.rs:1693 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/effect_controls.rs:1703 | 新增观测 | §69颜色popup对找到effect但EffectParam反序列化失败记录effect.color_track，保留关闭popup和无Action，附object/effect/param，固定生命周期Warn1/Error60。实际坏track通过color_overlay回归 |
| desktop/app/src/panels/effect_controls.rs:440 | 核实保留 | 向上移动效果菜单index.saturating_sub(1)配合菜单enabled/合法effect索引保护首项，原顺序与undo保留；不是资源统计 |
| desktop/app/src/panels/effect_controls.rs:778 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/effect_controls/curve.rs:319 | 核实保留 | 曲线鼠标操作只支持已有左键拖动/右键删除，Middle等无操作是明确交互范围；模型修改仍验证端点、点数与顺序，非Err丢弃 |
| desktop/app/src/panels/effect_controls/library.rs:475 | 核实保留 | 无选中effect或未提供说明时显示既定帮助文字是正常空状态/可选元数据，不是反序列化或安装失败；真实目录/manifest加载失败独立有日志 |
| desktop/app/src/panels/expressions.rs:56 | 已有覆盖 | ExpressionTarget的非空serde失败由expressions.target观测，表达式validate也包装；保留忽略坏项及原原因，固定site限频，已有实际坏target回归 |
| desktop/app/src/panels/expressions.rs:97 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/fx_console.rs:116 | 核实保留 | 搜索候选高亮和start偏移用饱和/min约束列表边界与滚动窗口，空候选已有处理；正常键盘导航，不把首项/尾项视为失败 |
| desktop/app/src/panels/fx_console.rs:67 | 核实保留 | 搜索候选高亮和start偏移用饱和/min约束列表边界与滚动窗口，空候选已有处理；正常键盘导航，不把首项/尾项视为失败 |
| desktop/app/src/panels/fx_console.rs:72 | 核实保留 | 搜索候选高亮和start偏移用饱和/min约束列表边界与滚动窗口，空候选已有处理；正常键盘导航，不把首项/尾项视为失败 |
| desktop/app/src/panels/fx_console.rs:74 | 核实保留 | 搜索候选高亮和start偏移用饱和/min约束列表边界与滚动窗口，空候选已有处理；正常键盘导航，不把首项/尾项视为失败 |
| desktop/app/src/panels/fx_console.rs:75 | 核实保留 | 控制台只消费导航/确认/退出，其他输入用于搜索文本或忽略；非Result错误，不改快捷键/焦点 |
| desktop/app/src/panels/fx_console.rs:81 | 核实保留 | 可见候选数先min(items.len)及min(10)，再按非负窗口高度计算；结果受候选数限制，不产生超大索引/分配。小窗口0行是既有视觉边界，未改布局 |
| desktop/app/src/panels/graph_box.rs:259 | 核实保留 | 没选择override时用默认StyleSheet读取motion.drag_threshold是正常语义token回退，未写颜色/尺寸字面量或新增style名 |
| desktop/app/src/panels/graph_box.rs:327 | 核实保留 | transform先拒绝非有限dx/dy，并把左右frame bounds夹到0..total-1，按相对比例产生有序key后验证严格递增；正规total来自u32项目帧数，round cast在界内，保留原拒绝重复帧行为 |
| desktop/app/src/panels/graph_box.rs:534 | 核实保留 | last_frame是合法project.frames-1，转u64再+1用来筛选区间key；u32帧范围可在u64精确表示，既有范围过滤，不丢整数身份 |
| desktop/app/src/panels/graph_handles.rs:124 | 已有覆盖 | Easing ease/curve现在由observed_easing对存在的坏数据分别限频并保留默认/None；拖动阶段validate另有固定site。沿用真实坏曲线、联动与默认字段安静回归，不再仅.ok丢掉原因 |
| desktop/app/src/panels/graph_handles.rs:125 | 已有覆盖 | Easing ease/curve现在由observed_easing对存在的坏数据分别限频并保留默认/None；拖动阶段validate另有固定site。沿用真实坏曲线、联动与默认字段安静回归，不再仅.ok丢掉原因 |
| desktop/app/src/panels/graph_handles.rs:308 | 核实保留 | 当前drag.frame已是u32，as u64用于查找JSON key是拓宽；原报告将其称float→u64不准确，保留查找与拖拽契约 |
| desktop/app/src/panels/graph_handles.rs:323 | 已有覆盖 | Easing ease/curve现在由observed_easing对存在的坏数据分别限频并保留默认/None；拖动阶段validate另有固定site。沿用真实坏曲线、联动与默认字段安静回归，不再仅.ok丢掉原因 |
| desktop/app/src/panels/graph_handles.rs:324 | 已有覆盖 | Easing ease/curve现在由observed_easing对存在的坏数据分别限频并保留默认/None；拖动阶段validate另有固定site。沿用真实坏曲线、联动与默认字段安静回归，不再仅.ok丢掉原因 |
| desktop/app/src/panels/graph_keys.rs:110 | 核实保留 | key拖动目标round后clamp到0..last_frame，项目上限为u32范围；受控鼠标坐标异常另由gesture/transform分支限制，不在此改变帧落点 |
| desktop/app/src/panels/graph_keys.rs:115 | 核实保留 | begin参数frame及next均为u32，as u64用于JSON查找/占用检查是无损拓宽；不是报告所说float→u64，不新增噪声日志 |
| desktop/app/src/panels/graph_keys.rs:37 | 核实保留 | begin参数frame及next均为u32，as u64用于JSON查找/占用检查是无损拓宽；不是报告所说float→u64，不新增噪声日志 |
| desktop/app/src/panels/graph_keys.rs:92 | 核实保留 | 正常未选override时取内建StyleSheet的拖拽阈值token；无Err来源，不修改交互容差 |
| desktop/app/src/panels/helpers.rs:52 | 核实保留 | timecode是显示格式化器，负帧显式max(0)后floor再按fps>=1显示，不回写工程/关键帧；正常有效项目范围可用u64，保留既有负时间显示语义 |
| desktop/app/src/panels/keyboard.rs:266 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/keyboard.rs:90 | 新增观测 | §69 keyboard对node request的失败共享request.field默认级别观测，仍保留现有Field.error/notice与无Action；不改变快捷键优先级或UI状态 |
| desktop/app/src/panels/layer_actions.rs:1205 | 核实保留 | Move Down目标index.saturating_sub(1)在底层边界夹取，菜单enabled/有效list约束，保留既有层级重排和undo，不是失败分支 |
| desktop/app/src/panels/layer_actions.rs:1301 | 已有覆盖 | track_matte非空serde失败已有layer_menu.matte固定site观测，保留None及原原因；缺省matte正常无日志。已有真实坏matte回归 |
| desktop/app/src/panels/layer_actions.rs:532 | 核实保留 | 父菜单定位及高亮可见性以saturating_sub处理top/scroll边界，随后限制在有效entries窗口；正常导航不是资源统计漂移 |
| desktop/app/src/panels/layer_actions.rs:610 | 核实保留 | 菜单count按实际entries.len、有限bounds高度限制，scroll按0..entries.len-count clamp后转usize；原可视行/滚动契约，不产生无界容器或模型身份丢失 |
| desktop/app/src/panels/layer_actions.rs:631 | 核实保留 | 菜单count按实际entries.len、有限bounds高度限制，scroll按0..entries.len-count clamp后转usize；原可视行/滚动契约，不产生无界容器或模型身份丢失 |
| desktop/app/src/panels/layer_actions.rs:652 | 核实保留 | 父菜单定位及高亮可见性以saturating_sub处理top/scroll边界，随后限制在有效entries窗口；正常导航不是资源统计漂移 |
| desktop/app/src/panels/layer_actions.rs:96 | 核实保留 | context_frame先floor再clamp(0,last_frame)，有效项目帧范围为u32；这是命令既有落点边界，不吞Result |
| desktop/app/src/panels/menus.rs:260 | 核实保留 | 菜单visible_rows按有效屏幕矩形max32/max1后min(entries.len)，usize表示行索引不损正常范围；不改原小窗口布局 |
| desktop/app/src/panels/menus.rs:263 | 核实保留 | 菜单项减少及向上滚动通过len-visible_rows/scroll-1饱和夹取，是正常列表导航，不涉及失败次数或显存 |
| desktop/app/src/panels/menus.rs:282 | 核实保留 | 菜单项减少及向上滚动通过len-visible_rows/scroll-1饱和夹取，是正常列表导航，不涉及失败次数或显存 |
| desktop/app/src/panels/menus.rs:76 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/mod.rs:1055 | 核实保留 | 节点标题在layer_target None时显示未绑定草稿，这是合法草稿状态；不把尚未选择图层当作渲染错误，不修改文案 |
| desktop/app/src/panels/parent_controls.rs:198 | 核实保留 | parent候选列表不足8项时max scroll为0，向上滚动在顶部夹零；正常下拉导航，不改变绑定/选项过滤 |
| desktop/app/src/panels/parent_controls.rs:200 | 核实保留 | parent候选列表不足8项时max scroll为0，向上滚动在顶部夹零；正常下拉导航，不改变绑定/选项过滤 |
| desktop/app/src/panels/parent_controls.rs:219 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls.rs:1019 | 新增观测 | §69 path.layer/path.inspector_layer分别包装实际Layer serde失败，仍返回None且不填inspector cache；附object/revision，进程site生命周期Warn1/Error60。真实坏层120次经两个入口回归；不改变路径/蒙版产品逻辑 |
| desktop/app/src/panels/path_controls.rs:1132 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls.rs:1179 | 核实保留 | 数值字段开始scrub时仅把可解析有限文本作为临时值，否则保留旧已提交数值；文本Commit阶段另保留Field错误/校验。正常中间输入不记录文本或改变拖拽状态 |
| desktop/app/src/panels/path_controls.rs:1240 | 核实保留 | 数值字段开始scrub时仅把可解析有限文本作为临时值，否则保留旧已提交数值；文本Commit阶段另保留Field错误/校验。正常中间输入不记录文本或改变拖拽状态 |
| desktop/app/src/panels/path_controls.rs:1261 | 核实保留 | open path边数len-1在0/1节点时饱和产生0边，closed且len>2另加闭合边；有意退化几何为空，不是模型错误或资源记账丢失 |
| desktop/app/src/panels/path_controls.rs:259 | 新增观测 | §69 path.layer/path.inspector_layer分别包装实际Layer serde失败，仍返回None且不填inspector cache；附object/revision，进程site生命周期Warn1/Error60。真实坏层120次经两个入口回归；不改变路径/蒙版产品逻辑 |
| desktop/app/src/panels/path_controls.rs:624 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls.rs:645 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls.rs:964 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/groups.rs:281 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/groups.rs:579 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/groups.rs:662 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/groups/color.rs:102 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/groups/keys.rs:255 | 核实保留 | 右键命中已有key优先用hit，未命中才由鼠标/ppf/offset算夹取后的帧，这是正常空白建key菜单落点，非Result失败回退 |
| desktop/app/src/panels/path_controls/groups/keys.rs:258 | 核实保留 | 右键空白落帧及key拖动目标按0..total-1 clamp后转u32，total来自有效project.frames；保留composition/local offset与已有碰撞拒绝，不改关键帧交互 |
| desktop/app/src/panels/path_controls/groups/keys.rs:325 | 核实保留 | 右键空白落帧及key拖动目标按0..total-1 clamp后转u32，total来自有效project.frames；保留composition/local offset与已有碰撞拒绝，不改关键帧交互 |
| desktop/app/src/panels/path_controls/groups/keys.rs:415 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/groups/parameters.rs:711 | 核实保留 | 参数树对没有可编辑参数的GroupItem跳过构造，geometry/fill/stroke/trim/repeater各有专用处理；空组等无需额外字段，不吞业务Result |
| desktop/app/src/panels/path_controls/mask_inspector.rs:247 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/mask_keys.rs:261 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/path_controls/mask_keys.rs:343 | 核实保留 | 右键优先hit已存在key，否则计算0..total-1内新落点为正常空白交互，不是读取/解析错误 |
| desktop/app/src/panels/path_controls/mask_keys.rs:346 | 核实保留 | mask key菜单/拖拽按0..total-1夹取并round，Project范围可用u32；负局部时间用offset映射，编辑helper另checked i32转换。原碰撞/pose原子替换语义保留 |
| desktop/app/src/panels/path_controls/mask_keys.rs:405 | 核实保留 | mask key菜单/拖拽按0..total-1夹取并round，Project范围可用u32；负局部时间用offset映射，编辑helper另checked i32转换。原碰撞/pose原子替换语义保留 |
| desktop/app/src/panels/path_controls/mask_path.rs:117 | 已有覆盖 | observed_edit已保留edit的原始Error并FailureCounter升级，首次notice只给已有UI反馈；Move/Copy/Curve/Set失败不推进拖拽结果，已有真实缺key/冲突/overflow/pose回归。沿用之前明确批准的功能修正，不归为本轮新增 |
| desktop/app/src/panels/path_controls/mask_path.rs:59 | 核实保留 | edit前置检查只针对需要旧key/空目标/维度约束的Action，其余Animate/Set等交由后续具体Track方法校验；非通配吞Result，失败统一返回并observed_edit记录 |
| desktop/app/src/panels/paths.rs:17 | 已有覆盖 | paths.layer实际反序列化由observe!记录object/phase和原原因，仍跳过无效层；进程site生命周期Warn1/Error60，已存在实际坏层回归 |
| desktop/app/src/panels/project.rs:401 | 核实保留 | 项目菜单只生成CompositionSettings/ShowPanel的Action，其余在这一分发点不应出现；当前entries构造与match相符，通配不是丢失运行时Result，不扩大菜单命令 |
| desktop/app/src/panels/properties.rs:432 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/properties.rs:783 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/properties.rs:859 | 已有覆盖 | 数值scrub起点非法文本现在Warn记录object/property/axis并保留既定值，不记录完整文本；真正commit验证与错误保留，已有数值编辑回归。按用户操作触发，不是每帧无条件日志 |
| desktop/app/src/panels/timeline.rs:1435 | 核实保留 | 移动候选先由原鼠标/snap构造、clamp_delta按全部被移clip边界限制，转换到i64以表达正负delta，随后saturating_sub初始帧；正常窗口/项目范围远小于i64，保留多层对齐移动契约 |
| desktop/app/src/panels/timeline.rs:1967 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/timeline.rs:1974 | 核实保留 | 没有axis时仅用property标题，有axis才附X/Y/Z；正常标签可选项，无Err和未实现属性吞掉 |
| desktop/app/src/panels/timeline.rs:230 | 核实保留 | property axis None时选择全部已知axes而不是单axis，是合法整属性选择；Some(axis)才一条轴，不是失败回退 |
| desktop/app/src/panels/timeline.rs:273 | 已有覆盖 | timeline.masks/vector/vector_size对存在的坏JSON已有固定site限频观测，保留空mask/None vector/默认size；真实坏三类与正常行交错120次验证默认可见，缺省vector本身安静 |
| desktop/app/src/panels/timeline.rs:283 | 已有覆盖 | timeline.masks/vector/vector_size对存在的坏JSON已有固定site限频观测，保留空mask/None vector/默认size；真实坏三类与正常行交错120次验证默认可见，缺省vector本身安静 |
| desktop/app/src/panels/timeline.rs:284 | 已有覆盖 | timeline.masks/vector/vector_size对存在的坏JSON已有固定site限频观测，保留空mask/None vector/默认size；真实坏三类与正常行交错120次验证默认可见，缺省vector本身安静 |
| desktop/app/src/panels/timeline_columns.rs:283 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |
| desktop/app/src/panels/timeline_columns.rs:291 | 已有覆盖 | matte_choice/matte_layers对非空TrackMatte及整个layers反序列化有observe!，原None/禁用候选回退保留；固定进程site生命周期Warn1/Error60，真实matte/layer坏数据回归 |
| desktop/app/src/panels/timeline_columns.rs:324 | 新增观测 | §69 timeline.blend对存在且错误的mode观测原serde原因，保留BlendMode默认；未提供mode/null安静，固定site生命周期Warn1/Error60。真实坏枚举与正常缺省交错回归 |
| desktop/app/src/panels/timeline_columns.rs:360 | 已有覆盖 | matte_choice/matte_layers对非空TrackMatte及整个layers反序列化有observe!，原None/禁用候选回退保留；固定进程site生命周期Warn1/Error60，真实matte/layer坏数据回归 |
| desktop/app/src/panels/timeline_columns.rs:363 | 已有覆盖 | matte_choice/matte_layers对非空TrackMatte及整个layers反序列化有observe!，原None/禁用候选回退保留；固定进程site生命周期Warn1/Error60，真实matte/layer坏数据回归 |
| desktop/app/src/panels/timeline_graph.rs:551 | 核实保留 | 网格时间标签把非负帧除以有效fps后格式化whole seconds/subframe；帧来自合法project区间，u32足够，属于显示精度而非关键帧ID。长时间标签的舍入格式保持原样 |
| desktop/app/src/panels/timeline_graph.rs:552 | 核实保留 | 网格时间标签把非负帧除以有效fps后格式化whole seconds/subframe；帧来自合法project区间，u32足够，属于显示精度而非关键帧ID。长时间标签的舍入格式保持原样 |
| desktop/app/src/panels/timeline_layout.rs:43 | 核实保留 | 虚拟可见行先floor/ceil/max0，last再min(count)；first>last自然空区间，实际height读正样式token。正常裁剪索引，不因窗口尺寸产生未受限分配，不改样式或命中区 |
| desktop/app/src/panels/timeline_layout.rs:44 | 核实保留 | 虚拟可见行先floor/ceil/max0，last再min(count)；first>last自然空区间，实际height读正样式token。正常裁剪索引，不因窗口尺寸产生未受限分配，不改样式或命中区 |
| desktop/app/src/panels/timeline_shortcuts.rs:107 | 已有覆盖 | 快捷键Easing解析已共享observed_easing，缺省默认与非空坏值分别处理，原错误可定位且限频。已有shortcut坏曲线/合法缺省及undo测试 |
| desktop/app/src/panels/timeline_shortcuts.rs:108 | 已有覆盖 | 快捷键Easing解析已共享observed_easing，缺省默认与非空坏值分别处理，原错误可定位且限频。已有shortcut坏曲线/合法缺省及undo测试 |
| desktop/app/src/panels/timeline_shortcuts.rs:185 | 核实保留 | key.frame/目标frame当前均来自u32 KeySelection或round后已限制的u32，as u64只用于JSON key相等/占用查询，是整数拓宽；原机械报告float标签不适用于这些当前类型 |
| desktop/app/src/panels/timeline_shortcuts.rs:27 | 核实保留 | key.frame/目标frame当前均来自u32 KeySelection或round后已限制的u32，as u64只用于JSON key相等/占用查询，是整数拓宽；原机械报告float标签不适用于这些当前类型 |
| desktop/app/src/panels/timeline_shortcuts.rs:272 | 核实保留 | key.frame/目标frame当前均来自u32 KeySelection或round后已限制的u32，as u64只用于JSON key相等/占用查询，是整数拓宽；原机械报告float标签不适用于这些当前类型 |
| desktop/app/src/panels/timeline_shortcuts.rs:282 | 核实保留 | key.frame/目标frame当前均来自u32 KeySelection或round后已限制的u32，as u64只用于JSON key相等/占用查询，是整数拓宽；原机械报告float标签不适用于这些当前类型 |
| desktop/app/src/panels/timeline_shortcuts.rs:450 | 核实保留 | 操作入口已将frame限制在当前project有效区间，frame.floor.max0后送既有编辑命令；负预滚区间在earliest key菜单逻辑显式显示无法编辑，不改范围契约 |
| desktop/app/src/panels/transform_controls.rs:325 | 核实保留 | 拖动中按Handle选择Resize/Rotate/Move/Anchor相应分支，其余无当前适用变换；有效坐标/范围验证决定是否发Action，不吞Result或改变精确命中容差 |
| desktop/app/src/panels/transform_controls.rs:348 | 核实保留 | 正常snapshot的frame由host ops/preview.rs验证有限、>=0且<project.frames，Project.frames是受模型校验的正u32；这里floor/max(0)后转u32生成现有编辑命令或缓存上下文，不截断正规时间范围。不为手工伪造JSON/NaN UI状态提供全输入保证，不更改取帧与交互 |

## B5 · UI 框架绘制、工作区与输入边界（30 项）

本批只补框架观测，未改panels、布局或控件行为；新增三个固定site，覆盖7行候选，其余逐项区分正常契约与自定义样式边界

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| desktop/ui/src/dock.rs:176 | 核实保留 | 删除已有tab后维持选中同一面板或最近邻，空dock索引按0表示，由调用方处理空状态；饱和为有意tab导航边界，不是队列或资源统计漂移，不改变所有权/选择行为 |
| desktop/ui/src/dock.rs:178 | 核实保留 | 删除已有tab后维持选中同一面板或最近邻，空dock索引按0表示，由调用方处理空状态；饱和为有意tab导航边界，不是队列或资源统计漂移，不改变所有权/选择行为 |
| desktop/ui/src/paint.rs:1073 | 新增观测 | §67 Painter对显式order的越界Glyph及缺失Shape范围累计本次无效操作数/首类型与索引，按render而非逐op计数，首Warn/连续60次Error/抑制，健康render独立复位；原skip与legacy raw-buffer兼容顺序保留，实际GPU像素回归验证不改变输出 |
| desktop/ui/src/paint.rs:1143 | 新增观测 | §67 仅确有glyph draw batch且atlas_view缺失时按Painter/render记录，首Warn/60Error/抑制，成功或无glyph批次复位；记录glyph数量，仍跳过glyph批次，原solid照常绘制。真实GPU验证文字丢弃、atlas上传后恢复，正常无文字不是失败 |
| desktop/ui/src/paint.rs:323 | 核实保留 | 裁剪后不足3顶点的polygon没有可生成三角形；1..len.saturating_sub(1)安全地产生空循环，避免索引下溢，退化不可见不是失败。保留实际像素/裁剪回归 |
| desktop/ui/src/paint/strokes.rs:153 | 核实保留 | 线条裁剪得到0/1/2顶点时triangle fan为空是有效退化几何；饱和界避免越界，不影响正常AA轮廓，不添加正常空几何日志 |
| desktop/ui/src/style.rs:211 | 已有覆盖 | 此前StyleSheet共享的有界Failures对缺失颜色key输出首Warn/生命周期第60次Error，保留透明回退；同一不可变sheet克隆共享、新sheet重置，最多128普通key加overflow槽，不在每帧无条件记录。沿用真实style/glyph默认级别回归 |
| desktop/ui/src/text.rs:81 | 已有覆盖 | 当前from_family对CJK fallback字体加载失败显式Warn并保留原原因/所选family，仍返回无fallback atlas；不是静默if let Ok。已有glyph缺字/图集观测不能证明此机器实际缺失所有CJK字体，本批不声称已自然重现系统字体库失败 |
| desktop/ui/src/widgets.rs:283 | 核实保留 | 仅在按下尚未启动的数值拖拽时取dirty文本作候选起点，解析失败/非有限/越界保留已提交current，未发模型修改；真正文本Commit失败另设置TextEdit.error并保留输入。属于既有输入编辑契约，不对用户临时无效文本加每帧告警或泄露输入内容 |
| desktop/ui/src/widgets.rs:284 | 核实保留 | 仅在按下尚未启动的数值拖拽时取dirty文本作候选起点，解析失败/非有限/越界保留已提交current，未发模型修改；真正文本Commit失败另设置TextEdit.error并保留输入。属于既有输入编辑契约，不对用户临时无效文本加每帧告警或泄露输入内容 |
| desktop/ui/src/widgets.rs:285 | 核实保留 | 仅在按下尚未启动的数值拖拽时取dirty文本作候选起点，解析失败/非有限/越界保留已提交current，未发模型修改；真正文本Commit失败另设置TextEdit.error并保留输入。属于既有输入编辑契约，不对用户临时无效文本加每帧告警或泄露输入内容 |
| desktop/ui/src/widgets.rs:67 | 核实保留 | 当前源码为Key::Left将caret饱和在首字符，原报告称backspace并不准确；正常光标移动边界，不是编辑失败，无需告警 |
| desktop/ui/src/widgets/composite.rs:111 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:234 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:493 | 核实保留 | 下拉列表收缩或打开时将highlight夹到当前items范围，空列表另禁用并关闭popup；原有选择/键盘导航契约，不吞模型Err |
| desktop/ui/src/widgets/composite.rs:506 | 核实保留 | 下拉列表收缩或打开时将highlight夹到当前items范围，空列表另禁用并关闭popup；原有选择/键盘导航契约，不吞模型Err |
| desktop/ui/src/widgets/composite.rs:514 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:641 | 核实保留 | 滚动偏移先按方向饱和增减，随后min(items.len-count)夹到可见窗口；顶部/底部滚动为正常交互，无资源少计或失败，不新增UI行为 |
| desktop/ui/src/widgets/composite.rs:643 | 核实保留 | 滚动偏移先按方向饱和增减，随后min(items.len-count)夹到可见窗口；顶部/底部滚动为正常交互，无资源少计或失败，不新增UI行为 |
| desktop/ui/src/widgets/composite.rs:658 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:682 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:815 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:885 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:89 | 功能边界 | 这些checkbox/label.small/调用方anchor/popup/row/button.icon并非compile强制类，手工host sheet可在resolve(...).unwrap时panic；桌面已有panic hook记录文件/行号/强制backtrace，默认sheet包含这些类，但本批不扩展编译必需类、不改回退与布局，也不宣称自定义sheet已全输入安全。独立motion-ui宿主若未安装日志/panic hook不由desktop保证 |
| desktop/ui/src/widgets/composite.rs:897 | 核实保留 | 评分左键盘操作在0下界饱和是合法值范围；右方向另min(max)，保留键盘交互，不把正常下界当作静默失败 |
| desktop/ui/src/workspace.rs:282 | 新增观测 | §67 框架Workspace::load分别记录read/size/json/validation阶段，固定进程连续降级报告首Warn/60Error/抑制，成功或正常NotFound复位；路径和原因各≤256字符。保留读全文件后16KiB过滤、默认布局返回与不重写坏文件；此处不同于desktop/app自有Workspace loader，不重复借用其旧观测证明。真实目录读失败/超限/JSON/重复所有权回归 |
| desktop/ui/src/workspace.rs:283 | 新增观测 | §67 框架Workspace::load分别记录read/size/json/validation阶段，固定进程连续降级报告首Warn/60Error/抑制，成功或正常NotFound复位；路径和原因各≤256字符。保留读全文件后16KiB过滤、默认布局返回与不重写坏文件；此处不同于desktop/app自有Workspace loader，不重复借用其旧观测证明。真实目录读失败/超限/JSON/重复所有权回归 |
| desktop/ui/src/workspace.rs:284 | 新增观测 | §67 框架Workspace::load分别记录read/size/json/validation阶段，固定进程连续降级报告首Warn/60Error/抑制，成功或正常NotFound复位；路径和原因各≤256字符。保留读全文件后16KiB过滤、默认布局返回与不重写坏文件；此处不同于desktop/app自有Workspace loader，不重复借用其旧观测证明。真实目录读失败/超限/JSON/重复所有权回归 |
| desktop/ui/src/workspace.rs:285 | 新增观测 | §67 框架Workspace::load分别记录read/size/json/validation阶段，固定进程连续降级报告首Warn/60Error/抑制，成功或正常NotFound复位；路径和原因各≤256字符。保留读全文件后16KiB过滤、默认布局返回与不重写坏文件；此处不同于desktop/app自有Workspace loader，不重复借用其旧观测证明。真实目录读失败/超限/JSON/重复所有权回归 |
| desktop/ui/src/workspace.rs:286 | 新增观测 | §67 框架Workspace::load分别记录read/size/json/validation阶段，固定进程连续降级报告首Warn/60Error/抑制，成功或正常NotFound复位；路径和原因各≤256字符。保留读全文件后16KiB过滤、默认布局返回与不重写坏文件；此处不同于desktop/app自有Workspace loader，不重复借用其旧观测证明。真实目录读失败/超限/JSON/重复所有权回归 |

## B7 · motion-graph / motion-model

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-graph/src/document.rs:313 | 已有覆盖 | 仓内 ports::semantic 及桌面 canvas 查询前均经 node_port_specs；其 node_ports 前置检查用 graph.ports(false/true)? 返回边界节点的原始 UnknownNode。独立 group_port 的 Option 查询契约保留；此条核实边界错误传播，不声称 canvas 的全部 if-let Err 出口已记日志 |
| crates/motion-graph/src/export.rs:225 | 已有覆盖 | StagedFile::drop 已记录非 NotFound 清理失败及路径/原因，见完成记录 §14 |
| crates/motion-graph/src/ports.rs:120 | 已有覆盖 | node_port_specs 的 node_ports 前置检查会对不存在的 Parameter 返回 UnknownParameter；不会进入后续空 units 回退。合法无单位参数仍使用 generic semantic |
| crates/motion-model/src/effects.rs:373 | 核实保留 | seed 是独立的 u32 随机参数，id 是 u64 身份标识；构造器以低 32 位初始化默认种子，不覆盖或收窄 id。相同种子并不等于实例身份冲突；若要求默认种子唯一，需要另定功能策略 |
| crates/motion-model/src/model.rs:725 | 新增观测 | serialized_bytes 保留 usize::MAX，首次 Warn、第 60 次 Error，含已写字节、类型及最多 256 字符的 serde 原因；真实非法 JSON map key 回归 |
| crates/motion-model/src/animation.rs:49 | 核实保留 | Tween::add 是运算原语，空间切线的 validate_range/set_spatial 在相加后检查 finite 并返回 non-finite spatial tangent。采样中的非有限结果另由 track.sample 观测；不把所有算术收窄都视为故障 |
| crates/motion-model/src/animation.rs:55 | 新增观测 | 有限端点及合法过冲曲线可产生超出 f32 的插值结果；Track::sample 记录非有限结果和 frame/首尾 key/数量/类型，不修改原值。验证后轨道的真实 Infinity 回归通过 |
| crates/motion-model/src/animation.rs:275 | 核实保留 | 有序唯一 i32 key 与 partition_point 分支保证当前插值时间位于相邻 key 之间，归一化 t 在 0..1；f32 精度是当前采样接口，不是溢出 |
| crates/motion-model/src/animation.rs:278 | 核实保留 | 曲线输入已归一化；合法 Bezier 控制值、端点和 velocity 面积均受 validate 限制，Elastic 参数也有限。允许 progress 过冲；由其产生的非有限属性结果在 Track::sample 观测 |
| crates/motion-model/src/color_curves.rs:95 | 核实保留 | Transfer::at 的 f64 结果先 clamp(0,1) 后转 f32；正规输入由 CurveObject 校验。保留 LUT/颜色通道精度契约 |
| crates/motion-model/src/composition.rs:356 | 核实保留 | audio_plan::visit 中 first=begin.max(clip_first).max(0)，并在 first>=last 时跳过；begin_sample 不会由负数转换 |
| crates/motion-model/src/composition.rs:357 | 核实保留 | 同一 first<last 分支保证 last>first>=0，end_sample 不会由负数转换 |
| crates/motion-model/src/composition.rs:969 | 核实保留 | retime 递归先处理对象中的 keys、markers、clip 时间，再遍历容器；字符串/数字/布尔/null 是遍历终点，无需对所有标量任意重定时 |
| crates/motion-model/src/composition.rs:1014 | 核实保留 | outside_keys 只枚举 keys 数组里的 frame；非容器分支是正常递归终点，不是漏掉对象键 |
| crates/motion-model/src/composition.rs:1113 | 核实保留 | trim 对对象 keys 做 retain，再递归对象/数组；叶标量保持原值，最后反序列化 camera，错误仍传播 |
| crates/motion-model/src/curve.rs:87 | 核实保留 | Easing::sample 在转换前 clamp t 到 0..1；NaN 不是此收窄导致，合法时间由 host 入口检查，模型已有 NaN 采样契约回归 |
| crates/motion-model/src/effects.rs:138 | 核实保留 | CurveTrack 的 upper 分支、唯一有序 i32 key 保证 t 在相邻 key 间；无需把标准 f32 插值精度记为故障 |
| crates/motion-model/src/effects.rs:141 | 核实保留 | 同 animation.rs:278 的合法有限曲线边界；颜色 LUT 为 0..1 数值，没有 f32 极大端点。保留曲线过冲与当前精度 |
| crates/motion-model/src/effects.rs:188 | 核实保留 | discrete 参数在首个 key 前持有第一个 key，是显式采样策略；非空检查后以 saturating_sub 防止索引下溢 |
| crates/motion-model/src/effects.rs:284 | 核实保留 | DFS 的 Some(1) 拒绝环、Some(2) 跳过已完成节点，None/其他值进入首次访问；通配不是丢弃分支 |
| crates/motion-model/src/effects.rs:660 | 核实保留 | 关闭曲线动画时取当前冻结 LUT，非空 keys 才取 upper.saturating_sub(1)；首 key 之前持首值，与采样契约一致 |
| crates/motion-model/src/masks.rs:155 | 核实保留 | disabled、MaskMode::None、开放路径明确不产生有效 mask；Ok(None) 是正常禁用语义，不增加每帧故障日志 |
| crates/motion-model/src/vector.rs:429 | 核实保留 | 非 trim 参数继续查 stroke/dashes，均不匹配才返回 vector modifier parameter missing；通配并未吞掉最终错误 |
| crates/motion-model/src/vector/groups/sampling.rs:188 | 核实保留 | chunks.len().saturating_mul(count)<=4096，溢出得到 usize::MAX 后拒绝而非放行；count 还受 copies 0..1024 限制 |
| crates/motion-model/src/vector/groups/sampling.rs:189 | 核实保留 | paints 的饱和乘积与 MAX_PAINT_BATCHES 比较，溢出同样拒绝并传播 Err，不是静默通过预算 |
| crates/motion-model/src/vector/groups/sampling.rs:198 | 核实保留 | 节点数饱和乘积与 MAX_EXPANDED_NODES 比较，溢出拒绝；既有 State::check_budget 也约束输入状态 |
| crates/motion-model/src/vector/path_ops.rs:153 | 新增观测 | cut 统一检查所有输出坐标/切线的 finite；非有限时记录切割起止、总长及节点数量，保留原 Some/None 和数值 |
| crates/motion-model/src/vector/path_ops.rs:157 | 新增观测 | 同 cut 输出检查，覆盖 outgoing.x；普通模型 sample/groups transform 有坐标范围检查，但公开 path_ops 接受调用者直接构造的 SampledPath |
| crates/motion-model/src/vector/path_ops.rs:158 | 新增观测 | 同 cut 输出检查，覆盖 outgoing.y |
| crates/motion-model/src/vector/path_ops.rs:162 | 新增观测 | 同 cut 输出检查，覆盖终点 x；真实有限 f32::MAX 节点/切线可裁切出 Infinity，回归保留这一结果 |
| crates/motion-model/src/vector/path_ops.rs:163 | 新增观测 | 同 cut 输出检查，覆盖终点 y |
| crates/motion-model/src/vector/path_ops.rs:164 | 新增观测 | 同 cut 输出检查，覆盖 incoming.x |
| crates/motion-model/src/vector/path_ops.rs:165 | 新增观测 | 同 cut 输出检查，覆盖 incoming.y |

## B8 · motion-effects / motion-desktop-media

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-desktop-media/src/av.rs:58 | 已有覆盖 | §35 rotation 解析保留原值和原因，缺省标签才是 0；显示矩阵继续优先 |
| crates/motion-desktop-media/src/av.rs:100 | 已有覆盖 | §35 AudioTrack.error 保留每条不可用音轨的原因；恢复失败向上返回，不只保留 0 rate/channels |
| crates/motion-desktop-media/src/av.rs:379 | 已有覆盖 | Decoder::decode_to 当前以 i64::try_from(time) 传播 decoded timestamp overflow，seek 也用 try_from(wanted)，不再是 as i64 |
| crates/motion-desktop-media/src/movie.rs:105 | 已有覆盖 | §34 AAC 回读现在只接受 EOF/EAGAIN 正常停止，其他 receive_frame 错误直接携带原因 panic；不是任意 Err 都通过测试 |
| crates/motion-desktop-media/src/movie.rs:392 | 核实保留 | ExportControl::cancel 的中毒恢复用于继续设置取消标志，不启动或重试任务；check 会返回 export cancellation state poisoned，真实 panic 由桌面 hook 记录。保留关闭/取消的防御行为，不把恢复改成二次 panic |
| crates/motion-desktop-media/src/movie.rs:550 | 已有覆盖 | §34 冻结前 modified() 错误带相对路径和 capture/before-copy 阶段传播，不再把失败映射为 None |
| crates/motion-desktop-media/src/movie.rs:579 | 已有覆盖 | §34 冻结后的 modified() 同样带 after-copy 阶段传播；不改变字节与时间戳一致性检查 |
| crates/motion-desktop-media/src/movie.rs:697 | 已有覆盖 | §34 export_destination 只将 NotFound 当作尚未存在的新输出；其他 canonicalize 失败返回原因，保留源文件保护 |
| crates/motion-effects/src/package.rs:307 | 已有覆盖 | §07 内置缓存发布经 atomic_write 独占临时文件和 Drop 清理；失败保留原始错误，清理另有路径/原因日志 |
| crates/motion-effects/src/package.rs:373 | 已有覆盖 | §07 安装包使用相同 atomic_write；已有自有临时文件与 foreign 临时文件隔离回归 |
| crates/motion-effects/src/package.rs:409 | 已有覆盖 | §07 disabled.json 使用相同原子发布；失败不提交内存 disabled 集合 |
| crates/motion-effects/src/package.rs:434 | 已有覆盖 | §07 uninstall 已先提交 metadata，再删包；删除失败恢复旧禁用状态，恢复再失败保留两次原因。不是原先 delete-then-register 顺序 |
| crates/motion-desktop-media/src/av.rs:398 | 核实保留 | Decoder::open 校验非空 timestamps 的 first==video_start_us、严格递增且 last<video_end_us；frame 拒绝 target<video_start_us。因此 partition_point 至少为 1，saturating_sub 不会把无前置时间戳偷偷变为首帧 |
| crates/motion-effects/src/shader.rs:148 | 核实保留 | work.saturating_mul 后立即 ensure(work<=65536)，溢出变成 u64::MAX 必然拒绝；饱和不会放过过大的 shader 工作量 |

B8 的代码处置复用此前 §07/34/35 的真实失败及可选 ffmpeg 验收证据；本批不宣称重新运行了 ffmpeg 功能全套，也未修改这些生产文件。

## B9 · Python 工具

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| tools/compare_export_frames.py:35 | 核实保留 | nb_frames/color_space 是可选 ffprobe 元数据，缺失以 JSON null 明确留在报告；passed 由实际 RGB/前景误差决定，尺寸与帧字节数另有拒绝。没有用默认值掩盖测量失败 |
| tools/generate_creative_library.py:120 | 已有覆盖 | §05 GLOW_EDGES 注入现用 replace_required；源模板匹配数量错误抛 ValueError，不静默 no-op |
| tools/generate_creative_library.py:122 | 已有覆盖 | §05 bright 移除同样使用 replace_required |
| tools/generate_creative_library.py:141 | 已有覆盖 | §05 EDGE_RAYS 注入同样使用 replace_required |
| tools/generate_creative_library.py:143 | 已有覆盖 | §05 bright 替换同样使用 replace_required |
| tools/generate_creative_library.py:160 | 已有覆盖 | §05 GLINT ray 循环注入同样使用 replace_required |
| tools/generate_creative_library.py:162 | 已有覆盖 | §05 GLINT 循环界限与步长的两次替换均使用 replace_required |
| tools/generate_creative_library.py:163 | 已有覆盖 | §05 GLINT 闭合括号注入同样使用 replace_required |
| tools/generate_creative_library.py:203 | 已有覆盖 | §05 KALEIDO radius 改写同样使用 replace_required |
| tools/generate_spatial_library.py:81 | 已有覆盖 | §05 shake 模板改写使用 replace_required；真实临时库生成回归保留 clamp 注入 |
| tools/generate_spatial_library.py:86 | 已有覆盖 | §05 transform_blur 模板改写使用 replace_required；预期匹配数量不符直接拒绝 |
| tools/validate_provenance.py:60 | 已有覆盖 | §05 UnicodeDecodeError 先加入 release-facing text is not valid UTF-8 错误，再继续收集其他文件；最终退出码 1，非法编码不能静默绕过扫描 |

B9 没有新增生产修改；专项重跑 `python -m unittest discover -s tools -p test_ci_silent_failures.py`，覆盖缺失/歧义模板、实际 creative/spatial 模板、递归 panels 扫描和非法 UTF-8，证据 b60-tools-observation.log。

## B6 · motion-media

本批关联全部 56 个候选：新增观测 15、已有覆盖 27、核实保留 14、部分覆盖 0。另补音频/视频 commit 两个未单列候选的主失败出口；不能用清理日志代替返回 Ok(failed status) 的主原因。

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-media/src/jobs.rs:84 | 已有覆盖 | Stage::drop 调用 publication::cleanup_stage，非 NotFound 删除失败 Warn，包含路径/原始 IO 原因；完成记录 §08 |
| crates/motion-media/src/jobs.rs:139 | 新增观测 | audio.registry.contains 在恢复 guard 释放后记录站点、request、entries、布尔结果和 poisoned_registry 原因；首次 Warn、进程累计第 60 次 Error，保持 into_inner 和原布尔结果；实际中毒查询及可重入 logger 回归 |
| crates/motion-media/src/jobs.rs:258 | 已有覆盖 | 音频导入/缓存 worker 在 provider/decoder 前 Info start 并 flush，spawn Err 以 request/operation/phase=spawn/耗时记录 Error；完成记录 §32 |
| crates/motion-media/src/jobs.rs:262 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:343 | 新增观测 | audio.import.result_lock 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/jobs.rs:365 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:447 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:454 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:455 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:465 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:466 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:467 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:498 | 已有覆盖 | 音频导入/缓存 worker 在 provider/decoder 前 Info start 并 flush，spawn Err 以 request/operation/phase=spawn/耗时记录 Error；完成记录 §32 |
| crates/motion-media/src/jobs.rs:503 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:550 | 新增观测 | audio.cache.result_lock 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/jobs.rs:556 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:561 | 已有覆盖 | PCM/waveform 使用 publish_cache_pair，第二文件失败回滚并恢复旧缓存；回滚失败保留 recovery 目录和双重原因，真实事务回归；完成记录 §08 |
| crates/motion-media/src/jobs.rs:589 | 已有覆盖 | Drop 改用独占 get_mut，registry 中毒仍向已有任务发送取消，已有真实阻塞 source + 中毒关闭回归；完成记录 §08。§62 另补 audio.drop.registry 与 audio.drop.task 的释放锁后观测 |
| crates/motion-media/src/jobs.rs:591 | 新增观测 | audio.drop.task 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/mp4_timing.rs:541 | 核实保留 | 缺 trex 时 tfhd 可覆盖默认 duration/size，trun 也可逐样本提供；selected duration()/android_sample_end() 对缺失或零 duration 明确 Err，不能将所有无 trex 的合法片段报为失败；完成记录 §08 |
| crates/motion-media/src/packages.rs:65 | 新增观测 | package.registry.contains 在恢复 guard 释放后记录站点、request、entries、布尔结果和 poisoned_registry 原因；首次 Warn、进程累计第 60 次 Error，保持 into_inner 和原布尔结果；实际中毒查询及可重入 logger 回归 |
| crates/motion-media/src/packages.rs:121 | 新增观测 | package.start.active_task 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/packages.rs:156 | 已有覆盖 | 工程包 worker 在归档准备前 Info start + flush，spawn 失败 Error 含 request/operation/phase/耗时；完成记录 §32 |
| crates/motion-media/src/packages.rs:160 | 已有覆盖 | 归档 panic payload 保留有界原因，失败完成及取消后丢弃结果前均经 TaskLog Error；完成记录 §32/44。不宣称本轮另做包原生 panic 注入 |
| crates/motion-media/src/packages.rs:175 | 新增观测 | package.worker.progress_task 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/packages.rs:187 | 新增观测 | package.worker.result_task 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/packages.rs:198 | 已有覆盖 | 归档 panic payload 保留有界原因，失败完成及取消后丢弃结果前均经 TaskLog Error；完成记录 §32/44。不宣称本轮另做包原生 panic 注入 |
| crates/motion-media/src/packages.rs:215 | 新增观测 | package.start.spawn_failure_task 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62。spawn 失败使用生产状态发布 helper 的受控 IO 错误注入，未耗尽 OS 线程资源；主错误同样延迟至外层 registry 释放后输出 |
| crates/motion-media/src/packages.rs:229 | 新增观测 | package.drop.registry 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/packages.rs:232 | 新增观测 | package.drop.task 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/video.rs:77 | 已有覆盖 | Stage::drop 调用 cleanup_stage，非 NotFound 清理失败 Warn 含路径及 IO 原因；完成记录 §08 |
| crates/motion-media/src/video.rs:116 | 新增观测 | video.registry.contains 在恢复 guard 释放后记录站点、request、entries、布尔结果和 poisoned_registry 原因；首次 Warn、进程累计第 60 次 Error，保持 into_inner 和原布尔结果；实际中毒查询及可重入 logger 回归 |
| crates/motion-media/src/video.rs:227 | 已有覆盖 | 视频导入/缓存 worker 在 open/probe 前 Info start + flush，spawn Err 有 TaskLog Error；完成记录 §32 |
| crates/motion-media/src/video.rs:229 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:258 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:383 | 已有覆盖 | published 路径逆序经 rollback_files 清理，主因与清理失败并存；最终 Ok(failed status) 的 commit 主错误本批另补 Error；完成记录 §08/61 |
| crates/motion-media/src/video.rs:402 | 已有覆盖 | 视频导入/缓存 worker 在 open/probe 前 Info start + flush，spawn Err 有 TaskLog Error；完成记录 §32 |
| crates/motion-media/src/video.rs:407 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:442 | 已有覆盖 | timestamps/thumbnail 改用同一 publish_cache_pair 事务，第二文件失败恢复先前缓存；完成记录 §08 |
| crates/motion-media/src/video.rs:451 | 新增观测 | video.cache.result_lock 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/video.rs:456 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:477 | 新增观测 | video.drop.registry 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/video.rs:480 | 新增观测 | video.drop.task 以固定计数记录 into_inner 恢复；首次 Warn、进程累计第 60 次 Error，业务锁释放后输出 request/operation/scope_elapsed_ms；真实锁中毒与原结果/取消语义回归，见 §62 |
| crates/motion-media/src/decode.rs:120 | 核实保留 | leading+frames 饱和后立刻与 max_frames 比较；max_frames 由受限 cache bytes 得到，u64::MAX 必然拒绝，属于保守预算而非漏算 |
| crates/motion-media/src/decode.rs:214 | 核实保留 | packet_start-edit.skip 是编辑后本地时间，preroll 早于可见区时故意钳零；下方 skip 独立去除 priming/overlap，末尾校验 audible，正常剪辑不逐包警告 |
| crates/motion-media/src/decode.rs:243 | 核实保留 | (edit.skip+audible)-packet_start 表示已写区域与本包重叠；后到包没有重叠时为零，并受 min(frames) 约束；checked_add 仍拒绝时间溢出 |
| crates/motion-media/src/decode.rs:246 | 核实保留 | 非 MP4 已解码区超过 priming 后，剩余 skip 正常为零；min(frames) 防止跨包裁切，不属于丢失负时间 |
| crates/motion-media/src/decode.rs:249 | 核实保留 | 编辑区已全部输出后，剩余 count 故意为零；继续解码并检查源完整性，最后 audible<edit.frames 明确 Err |
| crates/motion-media/src/matroska.rs:98 | 核实保留 | 颜色子元素先经过五个已知 ID 的 matches 守卫，随后 match 已覆盖这五个 ID，通配在该守卫下不可达 |
| crates/motion-media/src/matroska.rs:103 | 核实保留 | 这是有限容器元数据提取器，TrackEntry 中与 number/codec delay/video colour 无关的元素允许跳过；不是完整 demux，解析范围外元素不等于错误 |
| crates/motion-media/src/mp4.rs:67 | 核实保留 | len.saturating_sub(at)<8 的分支直接返回 truncated MP4 header；循环每次仅接受 n<=len-at，at 不会静默越过文件尾 |
| crates/motion-media/src/onsets.rs:21 | 核实保留 | begin 已 clamp 到音源范围；分析向前扩四个 RMS bucket 时在文件起点钳零，属于正常窗口边缘 |
| crates/motion-media/src/onsets.rs:69 | 核实保留 | RMS novelty 前后 25 bucket 的局部统计窗口在数组左边缘钳零，右边缘也以 min 限制 |
| crates/motion-media/src/onsets.rs:76 | 核实保留 | 局部峰前后 3 bucket 的正常数组边缘处理；保留能量参考算法，不制造近开头的故障警告 |
| crates/motion-media/src/packages.rs:168 | 核实保留 | total_bytes+archive overhead 饱和后用于 available_space 下界判断，溢出得到最大需求而不会降低需求；保守拒绝不属于失去预算 |
| crates/motion-media/src/video_timing.rs:60 | 核实保留 | 仓内 desktop/Android 两个调用方均对 PTS sort_unstable + dedup 并拒绝空数组；两点以上则末点大于首点，倒序下溢回退在这些调用方不可达。公开函数对外部任意 PTS 的原 fallback 保留，不声称增加全序校验 |

§62 已补齐原 12 个部分覆盖行的独立恢复观测，并扩展相邻 audio.drop.registry 与 video.import.result_lock；保留 into_inner 和中毒标记，未修复或掩盖中毒起因。本批三个 membership 站点的计数按进程生命周期累计，健康的其他 session/查询和调用方 clear_poison 不复位，进程重启才清零；无动态键或保留原因缓存，request 最多 128 字符。原 lock、contains、布尔返回及中毒标记均保留。

commit 的 Info start 在任务 registry/状态锁及文件发布之前，返回后在释放业务锁后报告主失败；实际第一次 ready 提交的失败记 Error，重读既有失败终态只做普通完成，不重复放大同一故障。保持 Ok(status)、取消/终态、回滚、落盘和 history 契约，不增加重试。验证见完成记录 §61。

## 计数、行为与验收边界

§60 的三个模型站点各有一个固定进程级原子计数器，无动态键或原因缓存；进程重启清零。健康的其他轨道/路径/序列化不复位计数，避免混合内容永久阻止升级。首次 Warn、第 60 次 Error 之后抑制；这是 lifetime_count，不是连续失败、帧数或一秒钟。正常路径没有日志 IO，错误上下文仅在阈值构造。§61 的三个 registry membership 站点采用相同的进程累计边界，§62 的 14 个恢复站点沿用进程累计策略；具体释放锁后输出边界见 B6 和完成记录。

本表不宣称为任意未校验的公共模型数据增加了功能校验；返回 Infinity、usize::MAX、None、Ok(failed status) 和现有 Err 的行为均保留。未更改 panels、UI、重试、事务或线程模型。验证记录见完成文档 §60/61/62。
