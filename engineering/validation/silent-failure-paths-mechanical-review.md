# 静默失败机械候选逐项复核

本表与 `silent-failure-paths-followup.md` 的 161 个辅助编号分开计数。原始 `silent-failure-paths-exhaustive.md` 列出 832 个候选；候选不是已确认缺陷。用原始路径与行号作稳定标识，行号不代表修改后的源码位置。

首批映射 B7 的全部 33 个候选、B8 的全部 14 个候选和 B9 的全部 12 个候选：新增观测关联 9 行、已有覆盖 25 行、核实保留 25 行。9 行对应 3 个新增观测站点，不是 9 个独立缺陷。其余 773 个候选尚未逐行关联到本表，包含此前批次已经处理的站点，不能称为 773 处缺失观测。

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

## 计数、行为与验收边界

三个新增站点各有一个固定进程级原子计数器，无动态键或原因缓存；进程重启清零。健康的其他轨道/路径/序列化不复位计数，避免混合内容永久阻止升级。首次 Warn、第 60 次 Error 之后抑制；这是 lifetime_count，不是连续失败、帧数或一秒钟。正常路径没有日志 IO，错误上下文仅在阈值构造。

本表不宣称为任意未校验的公共模型数据增加了功能校验；返回 Infinity、usize::MAX、None 和现有 Err 的行为均保留。未更改 panels、UI、重试、事务或线程模型。验证记录见完成文档 §60。
