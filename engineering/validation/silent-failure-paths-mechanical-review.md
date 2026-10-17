# 静默失败机械候选逐项复核

本表与 `silent-failure-paths-followup.md` 的 161 个辅助编号分开计数。原始 `silent-failure-paths-exhaustive.md` 列出 832 个候选；候选不是已确认缺陷。用原始路径与行号作稳定标识，行号不代表修改后的源码位置。

当前已映射 B1 的 258 项、B6 全部 56 项、B7 全部 33 项、B8 全部 14 项和 B9 全部 12 项，共 373 个候选：核实保留 254 行、功能边界 27 行、新增观测 30 行、已有覆盖 62 行。§63–64 新增六个 submission/GPU latch 锁恢复站点；此前处置保留。其余 459 个候选尚未逐行关联到本表，包含此前已覆盖的站点，不等于 459 个缺陷；功能边界不代表全输入已关闭。

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
