# 静默失败机械候选逐项复核

本表与 `silent-failure-paths-followup.md` 的 161 个辅助编号分开计数。原始 `silent-failure-paths-exhaustive.md` 列出 832 个候选；候选不是已确认缺陷。用原始路径与行号作稳定标识，行号不代表修改后的源码位置。

当前已映射 B1 的 77 项、B6 全部 56 项、B7 全部 33 项、B8 全部 14 项和 B9 全部 12 项，共 192 个候选：新增观测关联 27 行、已有覆盖 54 行、核实保留 108 行、功能边界 3 行。§63 新增 3 个固定 submission 锁恢复站点；此前 §60–62 的观测与处置保留。其余 640 个候选尚未逐行关联到本表，包含此前已覆盖的站点，不能称为 640 处缺失观测。映射或观测完成不代表功能风险已经修复，3 行功能边界不计为全输入已关闭。

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
