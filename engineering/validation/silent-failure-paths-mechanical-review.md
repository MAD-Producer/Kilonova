# 静默失败机械候选逐项复核

本表与 `silent-failure-paths-followup.md` 的 161 个辅助编号分开计数。原始 `silent-failure-paths-exhaustive.md` 列出 832 个候选；候选不是已确认缺陷。用原始路径与行号作稳定标识，行号不代表修改后的源码位置。

当前已映射 B6 全部 56 项、B7 全部 33 项、B8 全部 14 项和 B9 全部 12 项，共 115 个候选：新增观测关联 12 行、已有覆盖 52 行、核实保留 39 行、部分覆盖 12 行。新增映射关联 6 个站点，另补两个媒体 commit 主失败出口；候选行不等于独立缺陷。其余 717 个候选尚未逐行关联到本表，包含此前批次已经处理的站点，不能称为 717 处缺失观测。12 个部分覆盖行也不计为已关闭。

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

本批关联全部 56 个候选：新增观测 3、已有覆盖 27、核实保留 14、部分覆盖 12。另补音频/视频 commit 两个未单列候选的主失败出口；不能用清理日志代替返回 Ok(failed status) 的主原因。

| 原始位置 | 处置 | 当前源码证据与限制 |
| --- | --- | --- |
| crates/motion-media/src/jobs.rs:84 | 已有覆盖 | Stage::drop 调用 publication::cleanup_stage，非 NotFound 删除失败 Warn，包含路径/原始 IO 原因；完成记录 §08 |
| crates/motion-media/src/jobs.rs:139 | 新增观测 | audio.registry.contains 在恢复 guard 释放后记录站点、request、entries、布尔结果和 poisoned_registry 原因；首次 Warn、进程累计第 60 次 Error，保持 into_inner 和原布尔结果；实际中毒查询及可重入 logger 回归 |
| crates/motion-media/src/jobs.rs:258 | 已有覆盖 | 音频导入/缓存 worker 在 provider/decoder 前 Info start 并 flush，spawn Err 以 request/operation/phase=spawn/耗时记录 Error；完成记录 §32 |
| crates/motion-media/src/jobs.rs:262 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:343 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/jobs.rs:365 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:447 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:454 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:455 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:465 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:466 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:467 | 已有覆盖 | 导入发布失败调用 rollback_files，非 NotFound 清理失败 Warn 且附到主错误；该主错误存入 Ok(status).error 的最终 commit 出口本批另补 Error，不声称仅回滚日志就覆盖主失败；完成记录 §08/61 |
| crates/motion-media/src/jobs.rs:498 | 已有覆盖 | 音频导入/缓存 worker 在 provider/decoder 前 Info start 并 flush，spawn Err 以 request/operation/phase=spawn/耗时记录 Error；完成记录 §32 |
| crates/motion-media/src/jobs.rs:503 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:550 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/jobs.rs:556 | 已有覆盖 | catch_unwind 的 payload 经 panic_reason 保留最多 2048 字符；完成时释放状态锁再 TaskLog::finish(Error)，取消路径也保留真实失败；完成记录 §32/44 |
| crates/motion-media/src/jobs.rs:561 | 已有覆盖 | PCM/waveform 使用 publish_cache_pair，第二文件失败回滚并恢复旧缓存；回滚失败保留 recovery 目录和双重原因，真实事务回归；完成记录 §08 |
| crates/motion-media/src/jobs.rs:589 | 已有覆盖 | Drop 改用独占 get_mut，registry 中毒仍向已有任务发送取消，已有真实阻塞 source + 中毒关闭回归；完成记录 §08。逐站点恢复日志仍按 jobs:591 的部分覆盖边界登记 |
| crates/motion-media/src/jobs.rs:591 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/mp4_timing.rs:541 | 核实保留 | 缺 trex 时 tfhd 可覆盖默认 duration/size，trun 也可逐样本提供；selected duration()/android_sample_end() 对缺失或零 duration 明确 Err，不能将所有无 trex 的合法片段报为失败；完成记录 §08 |
| crates/motion-media/src/packages.rs:65 | 新增观测 | package.registry.contains 在恢复 guard 释放后记录站点、request、entries、布尔结果和 poisoned_registry 原因；首次 Warn、进程累计第 60 次 Error，保持 into_inner 和原布尔结果；实际中毒查询及可重入 logger 回归 |
| crates/motion-media/src/packages.rs:121 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/packages.rs:156 | 已有覆盖 | 工程包 worker 在归档准备前 Info start + flush，spawn 失败 Error 含 request/operation/phase/耗时；完成记录 §32 |
| crates/motion-media/src/packages.rs:160 | 已有覆盖 | 归档 panic payload 保留有界原因，失败完成及取消后丢弃结果前均经 TaskLog Error；完成记录 §32/44。不宣称本轮另做包原生 panic 注入 |
| crates/motion-media/src/packages.rs:175 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/packages.rs:187 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/packages.rs:198 | 已有覆盖 | 归档 panic payload 保留有界原因，失败完成及取消后丢弃结果前均经 TaskLog Error；完成记录 §32/44。不宣称本轮另做包原生 panic 注入 |
| crates/motion-media/src/packages.rs:215 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/packages.rs:229 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/packages.rs:232 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/video.rs:77 | 已有覆盖 | Stage::drop 调用 cleanup_stage，非 NotFound 清理失败 Warn 含路径及 IO 原因；完成记录 §08 |
| crates/motion-media/src/video.rs:116 | 新增观测 | video.registry.contains 在恢复 guard 释放后记录站点、request、entries、布尔结果和 poisoned_registry 原因；首次 Warn、进程累计第 60 次 Error，保持 into_inner 和原布尔结果；实际中毒查询及可重入 logger 回归 |
| crates/motion-media/src/video.rs:227 | 已有覆盖 | 视频导入/缓存 worker 在 open/probe 前 Info start + flush，spawn Err 有 TaskLog Error；完成记录 §32 |
| crates/motion-media/src/video.rs:229 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:258 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:383 | 已有覆盖 | published 路径逆序经 rollback_files 清理，主因与清理失败并存；最终 Ok(failed status) 的 commit 主错误本批另补 Error；完成记录 §08/61 |
| crates/motion-media/src/video.rs:402 | 已有覆盖 | 视频导入/缓存 worker 在 open/probe 前 Info start + flush，spawn Err 有 TaskLog Error；完成记录 §32 |
| crates/motion-media/src/video.rs:407 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:442 | 已有覆盖 | timestamps/thumbnail 改用同一 publish_cache_pair 事务，第二文件失败恢复先前缓存；完成记录 §08 |
| crates/motion-media/src/video.rs:451 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/video.rs:456 | 已有覆盖 | 导入/缓存 catch_unwind payload 已保留有界 panic 原因，状态发布及取消分支均在释放状态锁后记录 Error；完成记录 §32/44 |
| crates/motion-media/src/video.rs:477 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
| crates/motion-media/src/video.rs:480 | 部分覆盖 | 原 into_inner 恢复策略保留；桌面 panic hook 可记中毒起因，后台真实结果/取消错误已有 §32/44 覆盖，但此处恢复本身尚无独立站点计数。部分位置还持有外层 registry/任务锁，不在持锁区直接补日志；后续需逐站点延迟到释放全部业务锁后观察 |
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

12 个部分覆盖行仍待补独立恢复观测；映射完成不等于这些行已关闭。本批三个 membership 站点的计数按进程生命周期累计，健康的其他 session/查询和调用方 clear_poison 不复位，进程重启才清零；无动态键或保留原因缓存，request 最多 128 字符。原 lock、contains、布尔返回及中毒标记均保留。

commit 的 Info start 在任务 registry/状态锁及文件发布之前，返回后在释放业务锁后报告主失败；实际第一次 ready 提交的失败记 Error，重读既有失败终态只做普通完成，不重复放大同一故障。保持 Ok(status)、取消/终态、回滚、落盘和 history 契约，不增加重试。验证见完成记录 §61。

## 计数、行为与验收边界

§60 的三个模型站点各有一个固定进程级原子计数器，无动态键或原因缓存；进程重启清零。健康的其他轨道/路径/序列化不复位计数，避免混合内容永久阻止升级。首次 Warn、第 60 次 Error 之后抑制；这是 lifetime_count，不是连续失败、帧数或一秒钟。正常路径没有日志 IO，错误上下文仅在阈值构造。§61 的三个 registry membership 站点采用相同的进程累计边界，具体恢复策略和剩余部分覆盖见 B6。

本表不宣称为任意未校验的公共模型数据增加了功能校验；返回 Infinity、usize::MAX、None、Ok(failed status) 和现有 Err 的行为均保留。未更改 panels、UI、重试、事务或线程模型。验证记录见完成文档 §60/61。
