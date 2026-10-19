# 图层渲染采样：契约与实施证据

Issue：#255。本工作最初基于 `f32c64de`，已同步到包含 PR #336 的 `80da4087`，保留在 `feat/layer-render-sampling`。本文是进行中的交付记录，不能据此关闭 issue，也不表示三个开关都已支持。

## 当前要求与状态

| 要求 | 当前证据 | 状态 |
| --- | --- | --- |
| 模型存储、旧工程迁移、撤销 | LayerSampling、SetLayerSampling、格式 21；真实核心命令、保存/重开、undo/redo、手势取消测试 | 已实现并测试 |
| 连续栅格化实际像素 | 原创圆形在 256×256 输出中放大八倍；真实 GPU capture、透明边界、覆盖面积、反复切换和冻结模型重开 | 矢量已实现；其他来源仍需逐项能力界定 |
| 连续栅格化预算和预览 | 投影密度影响实际源纹理尺寸及 mesh/cache 身份；默认与预览计划一致；低预算拒绝且恢复预算可继续渲染 | 已实现并测试 |
| 帧混合 | 解码器要求真实相邻 PTS 双帧；GPU 在线性预乘空间混合；预览代理、桌面捕获与冻结组合读帧接入；实际 VFR、YUV、取消、冻结协议和默认日志用例 | 已实现核心链路；最终能力声明、UI 及原生界面验收待完成 |
| 运动模糊 | 中心快门和逐层有理数阶段调度；实际线性 FP16 GPU 累积；不同逐层相机投影的交叉遮挡、透视 UV、透明边界、取消、预算、BGRA 与嵌套效果/遮罩测试及独立图像 | 调度、累积和逐层投影合成模块已实现；生产逐层 Scene/父级/相机/嵌套时间评估、媒体 pending、预览任务及冻结协议仍待接入 |
| 能力检查及 UI | 模型拒绝对非视频帧混合、对非矢量连续栅格化、对非像素来源运动模糊 | 最终 host 能力声明和实际支持开关待接入；未加入空开关 |
| 双语、主/浮窗、焦点失效 | 本轮没有修改 UI | 待真实开关接入后验收与截图 |

## 已实现的连续栅格化语义

默认保持现有源纹理栅格化行为。启用后，使用已经包含父级与图层变换的 `view_projection * model`，在中心及四角计算局部像素到输出像素的投影导数，提高实际矢量源纹理分辨率。尺寸、位置、锚点、效果参数与源坐标不变。组透明度仍由现有隔离组渲染器执行，未用额外整图放大冒充矢量栅格化。

完全位于相机后方的源属于正常裁切，不提高密度。穿过相机平面而无法给出有限密度时显式返回含 layer ID 的错误；非有限坐标/导数同样返回错误。超过设备尺寸或现有源纹理预算时沿用真实资源拒绝，未偷偷截到源分辨率。维度拒绝在新增 tessellation 之前执行。

该实现覆盖已存在的 authored vector 几何。Text 当前引用已栅格化的资产，Composition 当前有自己的子合成纹理；二者不能仅靠设置布尔量就宣称连续栅格化。后续需要明确各自可支持的行为与限制。

预览继续服从现有实际输出尺寸；在同一输出尺寸下连续矢量采用同一投影密度。预览缓存显式比较 LayerSampling，不能只依赖未变化的 vector/mask Arc 或工程 revision。底层 mesh 的分辨率与顶点指纹继续参与上传缓存。

## 存储与时钟契约

LayerSampling 默认字段不序列化，旧工程默认重开保持字节等价。非默认设置要求格式 21；Set 和 Add 路径均提升格式。格式 22 仍拒绝。三个旧测试在保留未知格式失败断言的同时增加格式 21 成功断言，将未知格式用例改为 22；未删除或弱化旧格式、锁定、取消及源时间断言。

快门单位为合成帧，曝光居中，范围 (0, 2]，2–16 次采样，默认 0.5 帧/8 次。预览与冻结时钟函数保持同一曝光范围和均匀权重，预览仅减少样本数。图层或合成范围外应贡献透明度，禁止用重归一化使入出点变亮。独立 GPU 累积现在验证透明阶段不重归一化；生产 Scene 范围判断、预览和冻结多时钟仍未接入，不能将模块测试称为完整运动模糊。

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
| SAM-FUNC-04 | 功能边界 | 新增 ExposureClock 与独立 ExposureTarget。前者有界调度不同图层快门，后者保存进度、GPU 累积、预算及取消；没有新增 UI、worker、超时、重启、重试或日志基础设施。生产预览/冻结尚未调用这些接口。 |
| SAM-OBS-06 | 新错误上下文/模块验证 | 权重、阶段、所有者、尺寸、当前累计预算和未完成媒体显式 Err；视频错误包含 sample/object/source/time_us/phase，渲染错误追加 sample 和原始原因且保留 typed ResourceRejection。真实 GPU 用例验证拒绝不会推进阶段；提交失败后目标标为不可继续，禁止在同一累积结果上重复提交。默认日志信道、提交失败实际注入及 Error 文件/stderr 仍待 host 接入后验证，不以函数 Err 代替。 |
| SAM-NORMAL-03 | 正常路径 | 显式透明阶段 None、等待媒体前不调用累积、取消时丢弃目标均为正常操作；不写文件或日志。未完成曝光不能 resolve，资源计数在目标析构归还；提交后的资源退休由原有 opt-in submission ledger 记录。当前模块没有提供媒体任务自动重试或隐式继续。 |
| SAM-FUNC-05 | 功能边界 | 新增逐层相机投影的有界几何合成与透视纹理过滤。它们是排序、裁切和采样行为变更，没有包装成仅加日志；现有世界空间快速路径、公开顶点和计划 ABI 保留。生产多时钟 Scene 仍未接入。 |
| SAM-OBS-07 | 新错误上下文/模块验证 | 非有限/奇异投影、几何、W 除法及表示范围失败带 layer ID 和 exposure_projection phase 返回 Err，且只在失败分支构造上下文；裁切顶点和既有 BSP 几何容量有界。真实 GPU 用例覆盖非法投影拒绝及同 renderer 恢复；仍缺少默认文件/Error stderr/前端信道的端到端故障注入，不能把模块恢复称为日志验收。 |
| SAM-NORMAL-04 | 正常路径 | 视锥外、相机后方或退化不可见面正常裁切；未切分的可见面保留原始三角形交给 GPU 裁切。没有新增文件 IO、重试、后台任务、计数器或每帧日志。 |
| SAM-FUNC-06 | 功能修复 | 嵌套实例别名改为完整工程引用路径的确定性布局；可见范围和参数变化不改变实例身份。原始对象、父级世界矩阵、matte、效果图像输入和场景源引用同步映射；新建/复用 Scene 与模型序列化重开生成相同便携计划。不是视频解码器预热或逐层时钟已接入的声明。 |
| SAM-OBS-08 | 新结构化错误/模块验证 | 别名布局拒绝重复或缺失对象、引用身份不符、循环和资源超限，分别保留 sample_failed / composition_missing / cycle / render_resource_limit，包含 phase、合成以及可用对象/引用上下文。未知映射不再触发 HashMap 索引 panic。引用路径上下文有界；构造失败保留上一份完整布局。单元测试验证真实失败与恢复，不新增日志计数、重试或文件 IO；默认文件、Error stderr 和前端信道的端到端注入尚待最终 host 验收。 |
| SAM-NORMAL-05 | 正常路径 | 未启用/范围外引用仍预留确定性整数地址，但不创建纹理或解码器；完整引用布局最多 1,024 个实例（含根）、深度 16（含根）、256 个合成（含根）、每合成 128 个对象。此布局上限不放宽单帧 64 个活跃实例限制。拓扑相同仅比较有界源结构，不在每个样本重新展开路径；拓扑改变时重新构建，未保留无界的历史地址表。 |
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

PR #336：同一 head 的 PR CI 全通过；push 的 Intel Mac 重复构建已完成编译，在 upload-artifact 阶段因 GitHub DNS ENOTFOUND 失败，原始记录保留为 `pr336-intel-push-failure.log`。只重跑失败 job，未修改实现或跳过检查。2026-10-19 04:07:55 UTC 确认 head 仍为 d8415faf、全部检查成功、无 pending 后，按既有授权 squash 合并为 80da40874eea58813923376a23a834b0fdce8b82。

### 曝光调度与 GPU 累积模块

ExposureClock 对各图层的均匀中点采样建立共同阶段。使用所有 i/n 有理数边界的并集，不计算采样数的最小公倍数；支持 2–16 次采样时最多 80 个阶段，预览将每层限制为最多四次时阶段并集最多六段。每层保留自己的快门范围和边际权重，未启用的正 ID 返回当前合成帧。ID 唯一、数量有界，排序后二分查找，避免对每层每阶段线性扫描所有图层。CPU 阶段权重使用 f64，只校正完整曝光的加法舍入误差；不重归一化可见样本。

ExposureTarget 保存一个原有 sRGB 样本纹理和一个 RGBA16Float 累积纹理。样本经纹理读取解码，在线性预乘 RGB 与 alpha 上按权重相加，完整曝光才 resolve 到原有 RGBA/BGRA sRGB 输出。每阶段权重写入与自己的提交保持顺序；预览没有 CPU 像素循环或 GPU 回读。独立累积接口拥有自己的 encoder，拒绝来自其他 encoder 的未提交 YUV，以及取消后尚无完成图像的源，防止把未写入的纹理当成完整采样。嵌套检查保留实例和真实源上下文。

每个目标占 12 字节/像素。每 renderer 的全部存活曝光目标共享一个有界原子容量计数，使用当前 scratch_budget_bytes 的数值作为 ExposureTargets 独立预算；它不与原有效果 scratch、源纹理或预览目标容量合并，不是整个进程或驱动驻留显存上限。构造前拒绝超预算，降低策略后已有目标也重新检查；显式丢弃目标释放额度，未完成输出不能发布。调用者拥有目标，renderer.texture_bytes 不包含它；exposure_texture_bytes 和目标的 texture_resource_ledger 单独报告真实两幅纹理，需由未来 host 的所有权快照合并。既有 submission ledger 在实际提交中合并目标和输出元数据；取消后 GPU 在途资源与当前所有权有意区分。

实际验证：

- `sampling-exposure-clock.log` 首轮保留 f32 阶段求和得到 1.0000000214204192 的失败。改进 CPU 时钟为 f64 并校正完整曝光最后一段的求和误差，保留全部断言；`sampling-exposure-clock-exact.log` 四项通过。随后增加无序 ID 输入并使用二分查找，需以最终回归记录为准。
- `sampling-exposure-render-check.log` 模块编译通过。
- `sampling-exposure-gpu-tests.log` 三项通过、两项失败：真实缺陷是独立 encoder 错误接受未提交 YUV；另一个夹具把画布 x 当成世界 x，使移动矩形超出画面。增加完成源保护，修正夹具为原始世界中心加位移，保留未提交拒绝及透明面积断言。
- `sampling-exposure-gpu-ready-sources.log` 保留新增检查错误移动借用的 VideoSample 所致编译失败；改为借用，未改类型定义或生产返回协议。
- `sampling-exposure-gpu-borrowed-source.log` 五项通过：实际线性颜色、权重提交顺序、透明先/后阶段、80 阶段常量图像误差不超过一档、取消无串帧、全局存活额度、降低预算、外 renderer 拒绝、桌面 BGRA、未提交/取消 YUV 拒绝及显式完成转换恢复。
- `sampling-exposure-nested-mask.log` 夹具将 3D 源提交到只支持平面源的 precompose 命令，被现有协议拒绝。未改变该协议；改为平面源预合成后的空间参考平面。`sampling-exposure-nested-supported-plane.log` 实际冷启动嵌套、遮罩、Gaussian Blur 与空间参考用例通过，输出与正常 capture 每通道误差不超过一档。
- `sampling-exposure-clock-final.log`：无序 ID / 二分查找之后，模型四项采样测试全部通过。`sampling-exposure-regressions.log`：曝光六项、既有帧混合五项、内存策略三项、提交账本两项、纹理所有权六项，共 22 项实际 GPU/资源回归全部通过。两轮合计 26 项，不将前面的重复执行加到总数中。
- 同步 `80da4087` 后，`sampling-exposure-desktop-merged-check.log` 完成 `cargo check -p motion-desktop --features ffmpeg`，保留桌面既有的 21 条未使用/可见性等编译警告，采样模块没有新增警告；这不是新的 release 可运行包。`sampling-exposure-merged-gpu.log` 对最终合并基线再次执行曝光六项，全部通过；未将这次重复运行算成新增测试。

`exposure-sharp.png` / `exposure-accumulation.png` 来自实际 GPU，`exposure-comparison.png` 仅并排加棋盘格和文字。测试显式构造矩形的八个采样场景，并验证下方未模糊参考条逐字节一致、透明边缘、alpha 面积与取消。它证明调度和 GPU 累积有效，不能证明尚未完成的生产多时钟场景评估。`exposure-cost.json` 保留分配、渲染和回读总耗时：192×96、八阶段额外目标容量 221,184 字节；首次 45,307us，后续 49,768/9,682/23,638us，没有删慢值或称为播放 FPS。

### 逐层相机投影与纹理精度

单一 Scene.camera 的世界空间 BSP 不能表达不同时间的逐层相机。现在当空间图层的投影不同于全局相机时，将可见面先做齐次视锥裁切，再送入共同的规范深度空间排序和切分；UV/W 与 1/W 一起插值，最终从源平面恢复世界顶点及 UV。观察方向为规范深度的近端。普通单相机场景继续使用既有世界空间排序，2D 合成边界与共面层顺序保留。

裁切的多边形最多 16 个顶点，源信息最多 MAX_LAYERS=128 项；BSP 沿用 8,192 节点、131,072 arena 顶点、65,536 输出顶点限制。内部 Vertex 增加一个 f64 的倒数 W，公开 PlaneVertex 仍为 20 字节，便携计划版本仍为 9。这里的几何上限不等于全进程内存上限；本轮没有增加 GPU 纹理预算或目标纹理。

CPU 齐次裁切只服务于排序。未被 BSP 交叉平面切开的图层输出原始两个三角形，由 GPU 做实际视锥裁切，避免重复裁切及重新三角化改变已有边缘。实际切分后，高反差纹理暴露出硬件线性过滤权重精度差异：独立 UV/源平面坐标断言已经通过，但硬件 sampler 捕获仍相差最多 9 档。透视平面现使用四次 textureLoad 和 f32 权重计算同一钳位边缘、线性预乘双线性过滤；1×1 常量纹理只加载一次，平面 2D/正交投影仍使用既有硬件过滤。此修正同时用于普通合成、遮罩、遮罩提取及其便携导出着色器，未增加 uniform 或改动公开 ABI。

验证与保留的失败记录：

- `sampling-projected-planes-tests.log` 保留最初三项失败；原因之一是齐次裁切后的多边形仍错误地固定读取四个顶点，已改为实际范围。`sampling-projected-clipped-ranges.log` 随后证明透视/正交、半透明及效果路径的独立 CPU 射线深度参照通过，两个纹理参照仍失败。
- `sampling-projected-uv-diagnostics.log` / `sampling-projected-original-plane.log` 保留纹理差异诊断。用 UV 回到源平面不能消除重复裁切的像素变化；保留原始未切分三角形后，`sampling-projected-original-raster.log` 三项通过。没有放宽原有逐通道一档像素断言。
- `sampling-projected-3d-regressions.log` 增加真正被切开的纹理平面后仍失败，最大差异 9 档；`sampling-projected-gpu-coordinates.log` 记录只改初始投影算术仍无法解决。最终保留 f64 的齐次几何计算，避免将 f32 角点误差带入平面共面性；过滤修正后 `sampling-projected-filter-precision.log` 四项通过，没有删除或放宽新增的棋盘、UV 渐变和覆盖断言。
- `sampling-projected-final-regressions.log` 保留共享函数参数误用了 WGSL 保留字 filter 的编译失败；重命名后 `sampling-projected-valid-shared-shader.log` 的八组 32 项渲染回归（包含实际 GPU 和便携计划）全部通过。最终常量纹理快速路径、f64 裁切及几何代码整理后，`sampling-projected-final.log` 同样 32 项全部通过。
- `sampling-projected-filter-final.log` 两项通过：同一 renderer 内的新旧硬件采样器真实捕获对照、纹理容量不变，以及普通/遮罩/三种 matte 入口的便携 GLSL 编译。与上述 32 项合计 34 项，不将重复执行相加。
- `sampling-projected-recovery-final.log` 在增加同一 renderer 非法投影失败后的图像恢复、固定 20 字节顶点和计划版本 9 断言之后，四项再次通过；不是四项新测试。`sampling-projected-desktop-check.log` 带 FFmpeg 的桌面编译检查成功，保留既有 21 条桌面警告，没有新增渲染模块警告。这仍是 cargo check，不是新的 release 可运行包。

实际图片为 `projected-checker.png` 和 `projected-checker-hardware.png`，源棋盘/渐变纹理由测试独立构造。最终 `projected-filter-cost.json` 记录 RTX 4070 Laptop/Vulkan：同一切分纹理与未切分参照的最大差异从硬件路径 9 档降为 1 档，跟踪源纹理保持 16,388 字节，新增纹理容量为 0。1024×1024 的十次硬件捕获保留全部 17,614–20,616us，新过滤的十次捕获保留全部 16,972–20,623us。两者交替先后顺序，均包含 CPU 准备、GPU 和图片回读；这些数值没有分离 GPU 采样成本，不能据此声称播放帧率、无性能开销或稳定提速。显式过滤的指令成本从一个硬件采样操作变为最多四个 textureLoad，真实主程序播放成本仍需后续测量。

下一步仍须解决逐层父级/相机/效果源时钟、嵌套曝光、视频阶段 pending 的持久任务与被取代请求取消；新的投影合成仍须由真实多时钟 Scene 驱动后做整段 3D 验收。禁止用整幅普通动画截图相加来连带模糊未启用图层，也不能在每次 Present pending 时从第一阶段重启而抖动解码请求。最终冻结协议、host 错误信道、真实性能和 UI 接入完成前不关闭 #255。

### 嵌套实例身份与视频缓存

旧代码按当前可见实例的 DFS 顺序重新分配别名。前方引用进入或离开 clip 时，后方持续可见的视频会换 ID，导致同一 PTS 的已驻留图像失去身份。现按完整工程引用路径和原始对象槽建立确定性别名，包含当前不可见的引用。缓存只保存有界源拓扑；透明、禁用和动画参数变化不重新展开引用。布局在临时对象中完整构建后替换，非法结构不能留下半张表。

别名区间避开所有源合成与根合成的原始对象 ID，包括通常的高位 ID 和 u64::MAX。重排/增删源层或更改引用目标属于结构变化，允许重建；同一结构的新建、复用及模型序列化重开结果一致。隐式相机 0 保留，其他源对象通过有检查的映射访问，不使用会 panic 的 HashMap 下标。

- `sampling-instance-layout-tests.log`：既有 composition_frame_boundaries 两项、compositions 五项通过；最初新夹具的六秒视频 clip 超过其五秒源区间，验证器按原定义拒绝。将夹具限定为 150 帧，未修改生产校验、既有断言或返回协议；`sampling-instance-valid-clips.log` 三项通过。
- `sampling-instance-boundaries.log`：四项新单元测试通过，验证别名避开原始高位 ID、新建/复用一致、结构变更、无效编辑后恢复、未知/重复对象和合成，以及 16 层、1,024 实例、256 合成、128 层的边界与超限拒绝。范围外/禁用引用也计入静态容量，未借可见性绕过上限。
- `sampling-instance-final.log` 保留误写测试目标 exposure 的命令失败；实际目标为 exposure_accumulation。修正后 `sampling-instance-correct-targets.log` 六组 24 项全部通过：边界 2、嵌套合成 5、曝光 6、双帧混合 5、新别名 3、视频实例 3；与单元用例合计 28 项，不将重复执行计为新增。
- 新 GPU 用例将三个不同源偏移的双层嵌套引用跨入出点往返三轮。每帧独立按原始引用 ID 与 PTS 计算必要上传数，对实际 renderer 上传计数和输出像素做断言；持续可见、PTS 未变的实例不重复上传。另一个用例比较复用 Scene 与模型序列化重开的新 Scene 的便携计划完整字节；这不是工程 ZIP 文件保存重开的验收。matte、效果图像输入、Null 父级/场景源的测试验证映射和世界矩阵存在，不将其称为所有生成器的像素验收。
- `sampling-instance-desktop-check.log`：带 FFmpeg 的桌面 cargo check 成功，保留既有 21 条桌面警告，无新增渲染模块警告；不是新的 release 可运行包。

`instance-alias-cost.json` 保留真实 GPU 捕获用例中的 30 次 CPU Scene 采样耗时（53–237us）、16 次必要上传/16,384 字节及跟踪纹理峰值 39,940 字节。输入是测试明确提供的已解码 1fps 原创颜色帧，没有读取媒体文件；不将这些数值称为解码时延、真实播放 FPS 或完整进程内存，也不宣称用户首次遇到视频时的全部卡顿已经解决。

仍待 #255 最终验收：嵌套组合内双帧混合的专门像素/成本用例、真实桌面播放与冻结导出的整段采样一致性、完整性能记录、最终能力边界、UI 开关、双语主/浮窗和焦点取消。上述小范围 GPU 与协议测试不能代替这些验收，也未用于提前关闭 issue。

未新增依赖、第三方资产、UI 栈或主题 token；未读取 Adobe 二进制，也未把参考产品截图放入产品。本 issue 的最终 UI 仍需 DESIGN §0.7 逐项处置及 §12 验收。
