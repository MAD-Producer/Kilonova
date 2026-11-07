# 原生合成帧缓存存储与生命周期验证（#65）

本轮接续仓库中 `45241fb3` 的未接入草稿，基于主线 `b134bc6f` 验证和完善。
这是 #65 的原生存储层；Session 尚未调用缓存，#65 和 #79 均保持开放。
不改变预览调度、UI、输出变换、粒子时序或工程格式，不宣称已经加速预览。

## 已实现的边界

- 保存最终合成的独立纹理副本，不含 swapchain、面板、选区和其他 UI 覆盖。
  缓存副本没有 render-attachment usage；覆盖原渲染目标不会改写已保存的像素。
- key 保留精确小数帧的 f64 位、真实格式和尺寸，以及固定大小的工程/设备代际、
  内容和输出上下文。仅将等价的正负零帧规范化；不取整，不自动跨帧复用。
  内容和输出 digest 目前仍由调用方提供，不能把这项接口当成宿主权威 key 已完成。
- LRU 最多八帧，支持零配额/零帧和运行时缩限。复制前淘汰候选并使用现有共享
  AllocationBudget admission；不把“LRU 已删除”当成 GPU 显存已经释放。
- 新增 `CompositionFrames` 纹理类别和 `composition_frames` 原生预算类别。
  被消费者保留或 GPU 命令使用的淘汰条目继续记在同一类别中，直到真实持有者退役。
- 命中、未命中、安装、淘汰、配额拒绝、过时代际次数及缓存峰值占用均为实例生存期
  饱和计数，失效/清空不重置。resident/peak resident 只描述 LRU 自身，不替代共享
  预算中的当前及峰值占用；保留的外部句柄可能使二者不同。
- 旧代际完成先被拒绝，再检查设备所有者，避免访问已退休设备。普通 miss、禁用、
  显式配额拒绝和过时代际不是故障；实际分配/提交失败保留 typed resource rejection。
- 读取缓存也检查现有 GPU 健康状态。复制前核对真实纹理尺寸，避免可修改的宽高字段
  让非法 copy 抵达 GPU。错误文本包含阶段、文档/设备代际、帧、尺寸、格式及原始原因，source
  链仍保留原始 RenderError。本模块不新增逐帧文件日志、重试、清除故障或设备重启。

## 已执行的定向验证

同一份源码分别使用 `WGPU_BACKEND=vulkan` 和 `WGPU_BACKEND=dx12`，在实际
RTX 4070 Laptop 上执行九项不同测试；每次九项通过、零忽略。四精度检查原有的
manual 标记没有删除，使用 `--include-ignored` 明确执行，日志记录实际 adapter。
重复后端运行不重复计算不同测试数量。

最终同一源码的完整 `motion-render --lib` 在 Vulkan 使用 `--include-ignored`，
342 passed、0 failed、0 ignored。缓存九项检查另在 DX12 通过；合计覆盖 342 项
不同测试，不把重复后端运行叠加到数量中。

命令：`cargo test -p motion-render --lib renderer::composition_frames -- --test-threads=1 --include-ignored --nocapture`。

完整回归：`cargo test -p motion-render --lib -- --test-threads=1 --include-ignored --nocapture`。

同一源码的渲染器、宿主及带 FFmpeg 桌面全部目标编译检查通过：
`cargo check -p motion-render -p motion-host -p motion-desktop --all-targets --features motion-desktop/ffmpeg`。
既有 dead-code、unused 和 private-interface 警告保留，没有将“编译通过”表述为消除了这些警告。

| 验证项 | 实际证据与限制 |
| --- | --- |
| 时间与布局 | 相邻 f64 小数帧、四种 working format、每个 context 分量、非法时间/尺寸和容量上限 |
| 四种精度 | UNORM8/UNORM16/Float16/Float32 原始字节独立复制；浮点保留 HDR、负值和 alpha，无 SDR 转换 |
| LRU 与取消 | 触碰后淘汰最旧项、字节配额、禁用/恢复、设备所有者、旧代际完成和保留句柄 |
| 共享预算 | 真实 admission 拒绝及恢复；淘汰后一个实际编码的 GPU consumer 仍保留配额，提交完成后退役 |
| 类别与尺寸 | 独立 budget/ledger 类别，淘汰后类别字节仍在；伪造 public width 在分配/提交之前被拒绝 |
| 实际 GPU 故障 | 对真实 16 字节 buffer 越界写入；缓存 hit/miss 均返回已有 validation 原因和阶段，不清除 GPU 错误 |
| 真实合成像素 | 32×32 Float16 静态/动画工程，0.25/0.75/1.25/1.75 帧；完整 RGBA 与关闭缓存后重新编码的结果相等 |

动画场景同一整数帧内的两个样本像素不同，能揭示取整 key 错误。每个场景有四次 miss、
四次 hit、四次安装，LRU 峰值 32768 字节，禁用后 resident 为零且峰值保留。
这不是 Session 实际命中率、吞吐量或端到端峰值/延迟测量。

## 保留的失败与修正

首轮旧草稿五项通过。新增的真实 queue validation 对照在 v2 中七项通过、一项失败：
原始原因虽然在 source 链中，但 `with_diagnostic` 的 display 只有阶段说明。
修正 lookup 和 native-copy 的消息构造，使原始原因同时保留在显示文本和错误链中。
失败断言没有放宽，修正后两种后端的九项检查均通过。原始失败未覆盖或删除。

证据目录：`artifacts/issue-goal-current/composition-frame-cache/`，包括逐阶段 stdout、
stderr、完整 Rust/TOML/WGSL 源码的运行前后 SHA-256、命令、编译器和后端环境。
`draft-native`、`native-vulkan-v2`、v3 与最终 v4 是不同源码阶段，不能合计为同一源码验收。
v4 补齐错误的精确帧和代际上下文，并保留原始原因断言及完整错误链断言。

## 错误处理审计

| 编号 | 归类 | 证据与处置 |
| --- | --- | --- |
| C65-01 | 新功能边界 | 复制与读取的 Err 保留阶段和原始原因；实际 queue validation 对照验证 display/source |
| C65-02 | 正常分支 | miss、禁用、显式配额及旧代际返回正常结果并计数，不新增 Warn/Error |
| C65-03 | 既有覆盖 | 原生 GPU callback 和共享预算沿用现有实现；本轮没有替换 logger 或故障恢复策略 |
| C65-04 | 新增观测 | 仅暴露有界的 LRU 统计和独立预算类别，没有新增文件/控制台日志点 |
| C65-05 | 待验收 | Session 接线后用实际默认 logger 核对文件、Error stderr 和 stdout 隔离；当前返回错误的测试不证明这三项 |

## #65 仍需交付

1. 宿主构造权威 key：工程/合成、输入内容版本（包括同名换源）、插件 hash、求值后
   参数/表达式、相机/observer、蒙版/矢量/matte、格式与实际质量；不在 Present 做文件哈希。
2. 接通 Session 正常、exposure 和多视图的合成缓存读取/安装，UI 与选区覆盖独立重绘。
   需先协调正在开发多视图/预渲染的分支，不能覆盖其未提交改动。
3. 为静态内容建立时间不变性证明；未知 shader 时间依赖、表达式和粒子历史保守处理。
   当前不会根据“前后像素看起来相似”推断跨帧可复用。
4. 实际编辑、undo/redo、同名换源、质量和文档/设备切换的全像素失效验收，含晚到完成。
5. 完整应用静态/动态冷暖对照、命中率、驱逐、共享预算峰值和端到端成本测量；
   Metal 与其他硬件仍待验证。不承诺固定加速倍数。
6. 在这套代际和原生持有契约之上继续 #79 的节点/组版本域、下游脏传播和中间结果缓存。

没有新增依赖、第三方资产或产品 UI 变化。没有修改原有断言、忽略规则、生产预算或期限。
