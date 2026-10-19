# 图层渲染采样：契约与实施证据

Issue：#255。本工作基于 `f32c64de`，保留在 `feat/layer-render-sampling`。本文是进行中的交付记录，不能据此关闭 issue，也不表示三个开关都已支持。

## 当前要求与状态

| 要求 | 当前证据 | 状态 |
| --- | --- | --- |
| 模型存储、旧工程迁移、撤销 | LayerSampling、SetLayerSampling、格式 21；真实核心命令、保存/重开、undo/redo、手势取消测试 | 已实现并测试 |
| 连续栅格化实际像素 | 原创圆形在 256×256 输出中放大八倍；真实 GPU capture、透明边界、覆盖面积、反复切换和冻结模型重开 | 矢量已实现；其他来源仍需逐项能力界定 |
| 连续栅格化预算和预览 | 投影密度影响实际源纹理尺寸及 mesh/cache 身份；默认与预览计划一致；低预算拒绝且恢复预算可继续渲染 | 已实现并测试 |
| 帧混合 | 解码器要求真实相邻 PTS 双帧；GPU 在线性预乘空间混合；预览代理、桌面捕获与冻结组合读帧接入；实际 VFR、YUV、取消、冻结协议和默认日志用例 | 已实现核心链路；最终能力声明、UI 及原生界面验收待完成 |
| 运动模糊 | 模型有中心快门、2–16 个中点采样；预览最多四个；透明样本保留原始权重 | 逐层时间采样、效果/遮罩/父级/嵌套时钟、GPU 累积、冻结输出、图像与成本仍待接入 |
| 能力检查及 UI | 模型拒绝对非视频帧混合、对非矢量连续栅格化、对非像素来源运动模糊 | 最终 host 能力声明和实际支持开关待接入；未加入空开关 |
| 双语、主/浮窗、焦点失效 | 本轮没有修改 UI | 待真实开关接入后验收与截图 |

## 已实现的连续栅格化语义

默认保持现有源纹理栅格化行为。启用后，使用已经包含父级与图层变换的 `view_projection * model`，在中心及四角计算局部像素到输出像素的投影导数，提高实际矢量源纹理分辨率。尺寸、位置、锚点、效果参数与源坐标不变。组透明度仍由现有隔离组渲染器执行，未用额外整图放大冒充矢量栅格化。

完全位于相机后方的源属于正常裁切，不提高密度。穿过相机平面而无法给出有限密度时显式返回含 layer ID 的错误；非有限坐标/导数同样返回错误。超过设备尺寸或现有源纹理预算时沿用真实资源拒绝，未偷偷截到源分辨率。维度拒绝在新增 tessellation 之前执行。

该实现覆盖已存在的 authored vector 几何。Text 当前引用已栅格化的资产，Composition 当前有自己的子合成纹理；二者不能仅靠设置布尔量就宣称连续栅格化。后续需要明确各自可支持的行为与限制。

预览继续服从现有实际输出尺寸；在同一输出尺寸下连续矢量采用同一投影密度。预览缓存显式比较 LayerSampling，不能只依赖未变化的 vector/mask Arc 或工程 revision。底层 mesh 的分辨率与顶点指纹继续参与上传缓存。

## 存储与时钟契约

LayerSampling 默认字段不序列化，旧工程默认重开保持字节等价。非默认设置要求格式 21；Set 和 Add 路径均提升格式。格式 22 仍拒绝。三个旧测试在保留未知格式失败断言的同时增加格式 21 成功断言，将未知格式用例改为 22；未删除或弱化旧格式、锁定、取消及源时间断言。

快门单位为合成帧，曝光居中，范围 (0, 2]，2–16 次采样，默认 0.5 帧/8 次。预览与冻结时钟函数保持同一曝光范围和均匀权重，预览仅减少样本数。图层或合成范围外应贡献透明度，禁止用重归一化使入出点变亮；当前仅验证了时钟函数，未声称 GPU 已实现该语义。

帧混合依据导入的严格递增 PTS，而非 nominal fps；索引已有验证保证首项等于 video_start_us、末项小于 video_end_us。源有效区间外透明，尾部不循环。新的真实 worker 用例覆盖非零首 PTS 100ms、后续 170ms/400ms 的 VFR，135ms 的样本权重为 0.5，117.5ms 为 0.25；精确 PTS 只需单帧，最后一帧保持到 500ms 有效区间结束。还覆盖 clip 的 offset、in/out、源偏移和冻结保存重开。

## 已接入的双帧链路

Stream 的 Request 现在显式区分必需的右帧与推测预取。即使关闭预取的冻结输出也会解码必要右帧；两帧全部就绪前 prepare 返回正常 pending，不将左帧冒充完成结果。右帧失败保留原始 Err，并按必要帧记录错误；先前的推测请求成为当前必要帧时也使用当前要求分类。新请求取消旧 decode 后，旧结果不能覆写新目标。未增加自动重试、线程重启或 worker 上限。

每个实例保留两个源纹理和一个混合输出。RGBA 与现有 GPU YUV 转换均可提供输入；输入为 sRGB view、颜色为线性预乘，shader 对解码后的线性 RGB 和 alpha 同时加权，再写回 sRGB attachment。单独提交源缓存拷贝和混合，保证 uniform 写入与相应命令顺序，不复用尚未提交的转换 token。输出不冒充任意一个 PTS 的原始帧，切回 Hold 会正确重新上传。相邻前进或后退会交换缓存槽，复用已有邻帧；仅权重变化不会再次解码、上传或分配源纹理。

PreviewVideoCache 对左右输入应用同一现有 video_edge 策略，分辨率参与缓存命中，降低代理分辨率后能恢复完整输入。桌面 capture 使用完整双帧 GPU 混合。冻结组合读帧协议保留不可变双帧 snapshot，在显式 CPU 字节读回时才进行同色彩空间的 CPU 混合；在耗时像素处理前释放全局冻结读者锁。它与 raw source-frame 协议区分：后者仍返回实际源 PTS 的解码帧，不伪造混合 PTS。协议测试验证冻结后改变当前工程不会改变冻结输出、旧序号拒绝、关闭读者拒绝，以及已读取 Arc 的正常存活。

两幅缓存源纳入已有 source texture budget、VideoRgba 分项、texture resource ledger、清理/预算回收/项目替换与回滚。CPU FrameCache 的 Hold 上限仍为每流 36MiB；需要双帧时最多提升为两个最大支持 RGBA 帧，即每流 70,778,880 字节，四 worker 上限不变。两幅必需源同时固定，不能被预取逐出；切回单帧会释放额外预算与非必需帧。metrics 分别报告可用最高双帧预算及原 Hold 上限，不把逻辑容量称为驱动驻留显存。

双帧额度不允许单帧超过已有最大像素数对应的 RGBA 容量。FrameCache 的 bytes/budget 仅统计自身持有的帧；冻结快照、显式读回返回给调用者的 Arc/字节、解码器内部及在途缓冲不在该指标内，因此不能把该预算称为全进程 RAM 上限。

## 可观测性审计

| ID | 分类 | 证据与剩余边界 |
| --- | --- | --- |
| SAM-OBS-01 | 既有覆盖 | 新命令复用核心原子 apply、历史和原有串行引擎信道；未新增后台线程、超时、重试、重启、panic hook 或 stdout 日志。非法参数、锁定目标返回原始 Err。 |
| SAM-OBS-02 | 功能边界/既有覆盖 | 投影与预算失败返回 Err，保留 layer ID 或已有 ResourceRejection；实际 GPU 测试验证低预算不会无声降级，恢复预算后继续成功。尚未对新增相机平面错误单独验证默认文件日志/Error stderr/信道链路，不能将函数的 Err 测试等同于可观测性验收。 |
| SAM-NORMAL-01 | 正常路径 | 默认采样、缓存命中、透明边界、完全背向相机裁切不记故障；健康渲染无新增文件 IO、计数器或警告。 |
| SAM-OBS-03 | 既有覆盖扩展/实际故障 | 必要右帧复用 worker 的 in_flight 发布和原始失败日志；新分类防止右帧失败被降格为预取问题。桌面真实 FileLogger 子进程在默认 MOTION_LOG 下验证错误文件及 stderr、stdout 隔离、右帧阻塞前状态可读、失败原始 cause 和当前 object/source/sequence/PTS/phase。 |
| SAM-OBS-04 | 既有计数范围扩展 | pending_polls 仍是每 Stream 连续未就绪 request 轮询次数，现在包含必要右帧；成功及失败终态归零。默认日志测试阻塞两轮，各连续 120 次仅第 60 次产生一次 Error，切换 Hold 成功后重置，第二轮恢复成功后不再刷日志。60 次不是一秒。健康帧与普通缓存 miss 不增加文件 IO。 |
| SAM-FUNC-03 | 功能/协议边界 | 双帧请求、缓存槽复用、GPU 混合、预览代理、冻结 CPU 字节读回是功能实现；释放读者锁和保留 immutable Arc 是锁范围变更。未把这些算作仅加日志。 |
| SAM-OBS-05 | 新错误上下文/既有信道 | 新增双帧纹理预算拒绝保留 typed ResourceRejection、实际 required/budget、目标实例/槽、source ID 和 blend_allocate phase；实际 GPU 拒绝用例断言这些字段，随后恢复预算成功。它沿用 host 的资源拒绝与错误信道，没有新增 stdout 日志。 |
| SAM-NORMAL-02 | 正常路径 | 相邻源 cache miss、等待双帧、被取代请求、释放的冻结读者和 GPU 预算拒绝沿用各自协议处理；GPU 新缓存 API 的 false 表示未命中，不隐藏解码失败。原始帧读取与组合采样不是同一协议。 |
| SAM-PENDING-01 | 待完成 | 运动模糊尚需逐项审计多时钟 pending、取消、被取代请求、采样失败与冻结读者释放。新增连续栅格化相机平面错误仍缺少默认文件/Error stderr 的端到端验收；不能将现有函数 Err 用例等同于完成。 |

## 本地验证

日志目录：`E:/Dev/aem/artifacts/property-selection-context/`；图像与成本目录：`E:/Dev/aem/artifacts/layer-render-sampling/`。

- `sampling-render-check.log`：motion-core、motion-render 检查通过。
- `sampling-desktop-check.log`：带 FFmpeg 的整个桌面程序编译检查通过；没有新增 renderer/host 字段构造或命令穷举遗漏。当前仅是编译检查，未把该分支标作已交付的可运行 UI 版本。
- `sampling-render-tests.log`：首轮测试夹具误用了不存在的 PlanBuilder::default，编译失败，保留原日志。改用实际构造器，未改变生产返回值。
- `sampling-render-tests-correct-builder.log`：格式测试发现 document 入口仍有旧的最高版本校验，保留失败。同步两个验证入口后，`sampling-render-tests-version-gate.log` 完成：核心 layer_clips 11、render_sampling 2、subframe_clips 6、video_layers 3，以及渲染 continuous_rasterization 2、subframe_clips 1 全部通过。
- `sampling-clocks-tests.log`：模型两项快门/PTS 函数测试通过。
- `sampling-vector-preview-tests.log`：命令误指定 motion-render 中不存在的 vector_layers 测试目标；该文件属于 motion-core。修正为实际目标，未删用例或放宽断言。
- `sampling-vector-preview-correct-targets.log`：连续栅格化含真实 build_preview 与仅切换 sampling 的缓存失效验证；同时运行既有 image_preview_cache、vector_adjustment、vector_budget_alignment、vector_groups 回归。五组分别为 2、2、9、5、5 项，23 项全部通过。

原生 GPU 图片为 `native-vector.png` 和 `continuous-vector.png`，`continuous-raster-comparison.png` 仅将这两张截图放在棋盘格上并添加文字；未使用图片生成模型。圆形与矢量几何均由仓库原有 ShapeKind::Ellipse 独立生成，没有外部素材。

当前成本记录 `continuous-vector-cost.json`：源纹理等已跟踪资源从 16,852 字节增至 1,043,332 字节，半透明边缘像素从 9,348 降至 814，面积差小于 5%。记录同时保留首次捕获和三次资源驻留后的捕获耗时，包含 GPU 回读；不将这些数字称为播放 FPS、全进程 VRAM 或稳定跨设备性能。

### 帧混合追加验证

- `sampling-blend-render-check.log` 和 `sampling-blend-host-check.log`：实际 renderer、host 编译检查成功。早期 host 的单帧测试 wrapper 出现 dead_code 警告，限定为 cfg(test) 后没有保留新增警告。
- `sampling-blend-gpu-tests.log`：最初两项 GPU 用例通过；黑白 50% 输出约 188 而非 128、透明预乘、预算拒绝/恢复、缓存/清理、Hold 切换均有实际 capture 断言。
- `sampling-blend-host-tests.log`：首轮 fixture 使用了不存在的 start_frame 和浮点 LayerTimeline 字段，编译失败，原始日志保留；改用实际整数 offset_frame。
- `sampling-blend-host-correct-fixture.log`：fixture 没有将可见 clip 限定在非零 PTS 源区间，触发原有 recoverable interval 拒绝；修正夹具为 10–50 帧，没有更改生产校验或已有断言。随后 `sampling-blend-host-valid-clips.log` 三项通过。
- `sampling-blend-host-regressions.log`：39 项通过、一项新增 YUV fixture 失败。range=1 表示 full，测试错误地期望 Y=16 映射为黑色；改为已有定义的 limited range=2，保留黑色像素断言，没有改动转换公式、返回值或既有测试。
- `sampling-blend-host-final.log`：host 全部 41 项通过。加入前进/后退纹理槽复用后，`sampling-blend-host-reuse.log` 再次完成 host 41、preview_memory 3、video_retirement 1，45 项全部通过。新用例覆盖必要右帧失败、阻塞状态、取消、非零 VFR、代理恢复、YUV、冻结协议与 source-frame 区分；实际相邻移动每次只新增一次上传。
- `sampling-blend-desktop-logs.log`：带 FFmpeg 的桌面测试程序编译并通过新默认日志用例；使用真实 logging::init，子进程验证 Error 文件+stderr、stdout 隔离、阈值抑制和恢复。原有编译警告仍保留，未将它们称为新缺陷或本次修复。
- `sampling-blend-desktop-final.log`：在槽复用、冻结读者锁范围和提交/取消保护完成后，再次编译带 FFmpeg 的桌面测试程序并通过同一真实日志验收。该结果是测试二进制编译，不冒充已发布的桌面安装包。
- `sampling-blend-render-final.log`：frame_blend 3、video_instances 3、video_lifetime 8、video_yuv 7 共 21 项通过，涵盖既有提交/取消、plane 预算和多实例回归。
- `sampling-blend-bgra-effects.log`：桌面 BGRA sRGB target 经 brightness_contrast 效果链读取同一视频 view，在不重新上传的情况下正确响应三种权重；独立 GPU 捕获用例通过。
- `sampling-blend-cancelled-source.log`：未提交以及取消的 YUV token 都不能变成完成的混合输入；相同 PTS 显式重新上传成功后才可缓存，实际 GPU 用例通过。这里是提交状态的功能保护，不是通过重试掩盖取消。
- `sampling-blend-cache-limits.log`：五项 CPU 缓存用例通过，包括双帧额度下仍拒绝超过最大单帧容量、同时固定两帧、关闭时收回额外预算，以及既有 VFR/预取上限。
- `sampling-blend-budget-context.log`：新纹理预算错误保留实例/槽、source、phase 和原有结构化拒绝，实际 GPU 拒绝/恢复用例再次通过。

`hold-frame.png` 与 `linear-frame-blend.png` 是独立构造的透明矩形源经过真实 GPU capture 的结果，比较图 `frame-blend-comparison.png` 仅加棋盘格、文字并放大显示。它验证的是两帧加权的半透明叠影，不是光流补帧或物体运动插值。原始图像 alpha 面积保持在量化误差内。

`frame-blend-cost.json` 记录 192×96 源：逻辑跟踪纹理从 73,732 增至 221,188 字节，新增两幅 RGBA 源共 147,456 字节。两幅输入共上传两次；驻留后的权重变化不再上传输入。保留冷启动和三次包含 GPU 回读的捕获耗时（包括 155,335us 的首次混合与 81,239us 的首轮驻留捕获异常高值），未删除慢样本或宣称稳定性能提升；这些测量不是正式 Present FPS，也不是全进程内存。

PR #336：同一 head 的 PR CI 全通过；push 的 Intel Mac 重复构建已完成编译，在 upload-artifact 阶段因 GitHub DNS ENOTFOUND 失败，原始记录保留为 `pr336-intel-push-failure.log`。只重跑失败 job，未修改实现、跳过检查或直接合并；当前仍需确认重跑最终状态。

仍待 #255 最终验收：嵌套组合内双帧混合的专门像素/成本用例、真实桌面播放与冻结导出的整段采样一致性、完整性能记录、最终能力边界、UI 开关、双语主/浮窗和焦点取消。上述小范围 GPU 与协议测试不能代替这些验收，也未用于提前关闭 issue。

未新增依赖、第三方资产、UI 栈或主题 token；未读取 Adobe 二进制，也未把参考产品截图放入产品。本 issue 的最终 UI 仍需 DESIGN §0.7 逐项处置及 §12 验收。
