# 图层渲染采样：契约与实施证据

Issue：#255。本工作最初基于 `f32c64de`，已同步到包含 PR #336 的 `80da4087`，保留在 `feat/layer-render-sampling`。三个采样开关现已接入时间轴，支持范围见下表及能力声明。本文保留各阶段证据；旧阶段的“待接入”不代表最新状态。最新 main 的渲染集成和剩余性能/失败验收尚未完成，不能据此关闭 issue。

## 当前要求与状态

| 要求 | 当前证据 | 状态 |
| --- | --- | --- |
| 模型存储、旧工程迁移、撤销 | LayerSampling、SetLayerSampling、格式 21；真实核心命令、保存/重开、undo/redo、手势取消测试 | 已实现并测试 |
| 连续栅格化实际像素 | 原创圆形在 256×256 输出中放大八倍；真实 GPU capture、透明边界、覆盖面积、反复切换和冻结模型重开 | 矢量已实现；能力明确拒绝 text 和 composition，未宣称折叠合成变换 |
| 连续栅格化预算和预览 | 投影密度影响实际源纹理尺寸及 mesh/cache 身份；默认与预览计划一致；低预算拒绝且恢复预算可继续渲染 | 已实现并测试 |
| 帧混合 | 解码器要求真实相邻 PTS 双帧；GPU 在线性预乘空间混合；预览代理、桌面捕获与冻结组合读帧接入；实际 VFR、YUV、取消、冻结协议和默认日志用例 | 已接能力声明及时间轴开关；仅 video 支持 |
| 运动模糊 | 中心快门和逐层有理数阶段调度；逐接收层的真实 Scene/父级/相机/表达式/效果输入求值；独立局部效果图像与投影 matte；逐阶段调整图层；线性 FP16 累积；持久 GPU 帧任务、递归嵌套独立曝光、稳定媒体身份和真实冻结 MP4；Session 主窗/浮窗预览保留进度及请求身份；桌面 PNG 使用冻结后台任务 | 核心、冻结 MP4、桌面预览、PNG 和时间轴已接入；单时刻旧协议在修改状态/输出前拒绝曝光。实际长时间播放性能仍待验收 |
| 能力检查及 UI | host 公布来源、时钟、预览采样上限、冻结质量及旧协议边界；时间轴原生 Checkbox 发送真实 SetLayerSampling | 已接入；图层行 3D 旁三列，视图菜单显隐；不适用/锁定/观察状态禁用 |
| 双语、主/浮窗、焦点失效 | 原生 1600×1000 主窗及 900×600 浮窗实际 GPU 截图/输入，真实引擎命令、保存重开和撤销；释放及失焦输入不编辑 | 七项专项通过；完整桌面回归运行中。此轮修改视图菜单，没有修改右键菜单；DPI 扩展验收仍待完成 |

## 已实现的连续栅格化语义

默认保持现有源纹理栅格化行为。启用后，使用已经包含父级与图层变换的 `view_projection * model`，在中心及四角计算局部像素到输出像素的投影导数，提高实际矢量源纹理分辨率。尺寸、位置、锚点、效果参数与源坐标不变。组透明度仍由现有隔离组渲染器执行，未用额外整图放大冒充矢量栅格化。

完全位于相机后方的源属于正常裁切，不提高密度。穿过相机平面而无法给出有限密度时显式返回含 layer ID 的错误；非有限坐标/导数同样返回错误。超过设备尺寸或现有源纹理预算时沿用真实资源拒绝，未偷偷截到源分辨率。维度拒绝在新增 tessellation 之前执行。

该实现覆盖已存在的 authored vector 几何。Text 当前引用已栅格化的资产，Composition 当前有自己的子合成纹理；二者不能仅靠设置布尔量就宣称连续栅格化。后续需要明确各自可支持的行为与限制。

预览继续服从现有实际输出尺寸；在同一输出尺寸下连续矢量采用同一投影密度。预览缓存显式比较 LayerSampling，不能只依赖未变化的 vector/mask Arc 或工程 revision。底层 mesh 的分辨率与顶点指纹继续参与上传缓存。

## 存储与时钟契约

LayerSampling 默认字段不序列化，旧工程默认重开保持字节等价。非默认设置要求格式 21；Set 和 Add 路径均提升格式。格式 22 仍拒绝。三个旧测试在保留未知格式失败断言的同时增加格式 21 成功断言，将未知格式用例改为 22；未删除或弱化旧格式、锁定、取消及源时间断言。

快门单位为合成帧，曝光居中，范围 (0, 2]，2–16 次采样，默认 0.5 帧/8 次。预览与冻结时钟函数保持同一曝光范围和均匀权重，预览仅减少样本数。图层或合成范围外应贡献透明度，禁止用重归一化使入出点变亮。独立 GPU 累积验证透明阶段不重归一化；逐层 Scene 求值模块验证合成两端及精确 clip 的范围判断。真实冻结 MP4 已调用多时钟任务并验证透明边界；桌面预览尚未调用，不能据此声称完整运动模糊已交付。

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
| SAM-FUNC-07 | 新功能模块 | ExposureScenes 持有不可变文档及逐层时钟，驱动真实 Scene 求值；父级、相机、表达式、参数、matte 与图像输入来自接收层时刻，未模糊层来自中心时刻。子合成可作为求值根，资源/引用仍从完整文档解析。当前输出是借用的接收层上下文，不是已物化的通用局部图像；不得直接把完整上下文整图累积。 |
| SAM-OBS-09 | 新结构化错误/模块验证 | 求值失败保留原合成错误 code/message/源 phase，再追加 exposure phase、receiver、sample_frame；其他原因以原始 Display 保留。非对象 details 和内层 exposure 上下文包入 source_details，避免 JSON 字段索引 panic 或覆盖原始上下文。任务失败后不可继续求值，不重试，不推进阶段；非法推进显式 Err 且不改变位置。实际失败测试覆盖这些分支与释放文档所有权；默认日志/Info in-flight/Error 文件及 stderr/前端信道仍待 host 端到端验收，不把返回 Err 称为完成可观测性。 |
| SAM-NORMAL-06 | 正常路径/资源边界 | 范围外、缺少可绘制接收层、空合成和等待媒体是正常状态；60 次重复 current 不重算或推进，不产生日志。每任务只保留中心与一个变化时刻的两个 Scene（各自仍受既有活跃实例限制）、最多 128 个接收层与最多 80 阶段的时钟；不保留全部阶段场景。+0/-0 缓存身份按位区分。ExposureDocument 只在不可变文档 revision 创建时校验一次，各帧/子任务可共享；本模块没有新 GPU 分配、文件 IO、后台线程、超时或重启。 |
| SAM-FUNC-08 | 新功能模块 | 增加 caller-owned LayerRaster、逐层物化、阶段 BSP/混合及 accumulate_layer_exposure。接收时刻的局部效果输出与投影 matte 分开保留；纯色保留浮点颜色/alpha，用 1×1 常量源避免额外 RGBA8 舍入。普通路径、资源信道和 UI 未切换到新模块，不宣称 #255 完成。 |
| SAM-OBS-10 | 新上下文/模块验证 | 物化拒绝失败效果、未提交/已取消的视频转换、尺寸/所有者/策略/重复接收层/预算错误；错误不推进曝光阶段，保留原始 cause 与 typed ResourceRejection。缺少效果定义增加最多 96 字符的效果标识。模块真实 GPU 拒绝与恢复不等于默认日志、Error stderr、Info in-flight 或前端信道验收，这些仍待 host 接入。 |
| SAM-NORMAL-07 | 正常路径/资源边界 | 缺少接收层返回 None；范围外样本由调度方保持透明权重。每个保留局部 RGBA 为 4WH 字节，可选投影 R8 matte 为 viewport WH 字节，纯色源为 4 字节；与所有 ExposureTarget 合计使用同一有界 temporal 计数，独立于 effect scratch/source allowance。Drop 归还逻辑容量，提交 ledger 记录异步退休；这不是总显存上限。无新增线程、重试、重启、日志计数、文件 IO 或每帧日志。 |
| SAM-FUNC-09 | 新功能及独立兼容性修复 | draw_exposure_phase / accumulate_exposure_phase 将中心时刻的调整参数和引用素材作用到每阶段的下方合成，再进入曝光累积。复用现有效果、范围/opacity、混合及背景呈现；保留旧局部 API 的调整图层拒绝断言。真实 BGRA 组合测试发现混合中间纹理固定 RGBA 导致 pipeline 格式不匹配，现随 renderer 输出格式创建，容量仍为 4WH；这是功能修复。 |
| SAM-OBS-11 | 新上下文/模块验证 | 中心依赖先严格预检，包含嵌套效果；不走预览的失败效果绕过路径。返回原始 cause、composition、sample_frame、adjustment_prepare / adjustment_effect 及已有对象信息，保留 typed ResourceRejection。真实失败、预算和策略拒绝后阶段不推进，未完成的累积不能捕获；调用者显式提供有效上下文后可继续。没有新增日志计数、吞错、自动重试或重启，默认文件/Error stderr/Info in-flight/前端信道仍待 host 接入验收。 |
| SAM-NORMAL-08 | 正常路径/资源边界 | 空 backdrop、disabled 效果、零 opacity 和范围外接收层按既有语义处理；背景在最终呈现时合入，不被调整图层改色。中心依赖准备先提交，再上传阶段几何；有调整图层时通常为准备、阶段、累积三次有序提交，opt-in ledger 保留输入/输出容量。调整 accumulator 为独立 allowance 的 8WH，存在混合则为 12WH；没有提高预算、线程或每帧日志。 |
| SAM-FUNC-10 | 新功能及协议边界 | ExposureFrame 持久保存阶段、局部图像及失败状态；ExposureMedia 按当前 Scene 的真实相邻 PTS 准备并提交媒体，等待不改写请求 generation。FrozenMovie 使用不可变 ExposureDocument，并在全部阶段完成后才编码；普通无快门路径保留原实现。8ms poll、每次最多 128 工作单元是操作之间的预算，不能抢占同步 evaluator/driver/callback。冻结输出沿用 30 秒解码期限，按不变的 pending progress 计时，推进后重置；无自动重试或任务重启。 |
| SAM-OBS-12 | 新错误上下文、既有覆盖扩展 | 实际素材上传/解码/求值错误保留原始原因和 typed ResourceRejection，附加 composition、sample_frame、phase 及可用 object/source/PTS。任务失败终态清除保留局部图像，首次 Err 保留原因；每任务最多保留 2,048 字符供后续轮询返回。真实默认 FileLogger 子进程验证新媒体路径复用原 decoder open Info、阻塞前 in-flight、Error 文件/stderr、stdout 隔离及第 60 次一次性升级。没有新增逐帧 Info 或日志基础设施；GPU/求值错误的完整前端信道验收仍待完成。 |
| SAM-NORMAL-09 | 正常路径/资源边界 | 媒体未就绪、有限切片耗尽、透明接收层均为正常 Pending/跳过；不会丢弃已完成接收层或阶段。失败任务后续轮询不再调用媒体，显式丢弃释放 temporal 容量；换新任务/reader 是调用者显式动作。进度和统计限于一帧，工作计数饱和相加、资源/粒子取峰值；cpu_prepare_us 计 poll 内实际墙钟，排除两次 poll 间等待。没有新增后台线程、日志计数或健康热路径文件 IO。 |
| SAM-BOUND-02 | 原临时边界已消除 | 最初拒绝独立子合成快门，现由 SAM-FUNC-11 的递归任务实现；原拒绝用例改为真正完成且透明的输出断言，并新增子层跨入快门、父子混合帧率和独立 readback oracle。不是删除拒绝断言后仅宣称支持。 |
| SAM-FUNC-11 | 新功能/所有权边界 | 每个嵌套引用在其实际源帧上运行独立 ExposureFrame，子级完成后 GPU resolve 为 4WH 图像；父级接收层/中心调整使用 local_context，不再调用普通嵌套绘制覆盖该结果。父快门采样子级源帧，子快门使用自身 fps/范围，形成真实递归组合。每父任务同时只推进一个子任务；完成兄弟保留解出的图像。所有快照/局部图像/未导入解出图像共享 temporal 额度，转入 renderer 后沿用源预算；同一 allocation 身份转移，无 CPU 像素复制。 |
| SAM-FUNC-12 | 媒体生命周期修复 | 曝光子根共享原根的静态引用路径布局，根对象、shutter、父级/matte/效果源与后代别名一致，不为每次进入子根重新编号。新 sampling_exact 路径保留最多四个解码器，按最近使用淘汰非当前作用域实例；live+retiring 沿用四 worker 上限。普通 prepare/exact 仍按旧语义释放 inactive。失败流不重启；容量腾退和媒体等待是正常调度，显式 clear/项目替换仍释放所有流。 |
| SAM-OBS-13 | 新上下文/实际模块拒绝 | 递归错误保留子合成/reference/source frame/nested_exposure 与原始 cause，最终仍由父任务进入失败终态；typed 源预算/共享 temporal 拒绝保留。work_units 是每帧饱和的完成计数，真实子级推进才增加，媒体等待保持不变，冻结期限因此能识别子级进度；不是日志频率计数或时间单位。默认 decoder 日志沿用既有基础设施，嵌套 GPU/求值拒绝的完整前端信道仍待验收。 |
| SAM-NORMAL-10 | 正常分支与范围 | 单个子层中心 clip 外但自身快门内可贡献像素，未模糊同级层保持中心时刻；整个子合成源时间外仍按原 clip 语义透明。factory 共享 immutable 文档、evaluator 和有界引用布局；源图遵守原 16 深度、1,024 静态路径和每 Scene 64 活跃实例限制。递归各作用域仍持有上下文，不能把 64 说成全部存活上下文总数；没有额外线程、自动重试、后台解码预取或每帧日志。 |

| SAM-FUNC-13 | 新功能 | Session 使用按 revision/合成缓存的 immutable 文档，持久请求比较帧、输出尺寸、质量、视图版本、观察矩阵、surface epoch/尺寸、viewport 和 image 区域。Pending 保留阶段/图像，成功 resolve 后才提交候选画面；主窗与浮窗使用同一入口。 |
| SAM-FUNC-14 | 播放及取消 | 自动播放按正在绘制合成的窗口等待完整曝光，其他窗口只画 chrome 的回复不能解除等待；完成后按既有墙钟/音频时钟跳到最新帧。用户 Seek 立即取消旧任务，GPU 所有权释放；只退休未完成解码，完成缓存和失败终态保留，live + retiring 四 worker 上限不变。 |
| SAM-OBS-14 | 新上下文/既有信道 | 首次渲染失败带 composition/revision/frame/size/view/surface、原始 cause 与 typed 预算拒绝；同一请求保留至多 2,048 字符，不因下一次 Present 重启。清理失败追加到主错误；维持原 respond/引擎日志/错误回复。实际主窗/浮窗受控解码失败验证默认文件、Error stderr、stdout 隔离及阻塞前 decode in-flight。 |
| SAM-OBS-15 | 新诊断数据 | preview.exposure 提供状态、阶段、对象、采样时刻、完成单元、跨 poll CPU 及媒体准备/上传合计；Session ledger 合并递归任务所有者。快照与 FrameRecorder 分开记录 exposureTargetBytes，并加入逻辑纹理总容量；曝光没有收集多提交 GPU 时间戳，gpuTimingActive=false，不将最后一次提交时间冒充全部 GPU 时间。 |
| SAM-NORMAL-11 | 正常分支/已有策略 | 切片用完、媒体 Pending、被取代请求和正常取消不新增每帧日志。沿用原四级预览内存降级/冷却探测；该显式策略可建立不同质量候选，普通错误终态不自动重试。没有新增 worker、超时期限或重启；8ms 是批次之间的让出检查，不能抢占同步 evaluator/driver。 |
| SAM-FUNC-15 | 新功能 | 桌面 Capture/导出 PNG 在串行 worker 捕获版本、帧、注册表、预算及源文件句柄，独立任务复制素材并以完整质量渲染；曝光复用冻结 MP4 的任务驱动，保留小数帧。输出验证后发布，不再把视频 Pending 当导出失败，也不复用/覆盖预览 renderer。 |
| SAM-OBS-16 | 新观测及既有信道 | PNG 后台任务继承原 request ID，Info start 在复制/渲染前写入；独立 frame flight 和 desktop.frameExport 可在串行引擎空闲时读取。错误经原 respond 进入文件/Error stderr；新增 revision/frame/path 上下文。受控 native 阻塞、解码失败、排队前取消和解码中取消验证正常结果、失败信道与 stdout 隔离。 |
| SAM-NORMAL-12 | 正常取消/资源边界 | 每引擎至多一个 PNG worker，直到原生 decoder 真正退出才允许下一个任务。取消不重启 worker、不覆盖旧文件；正常取消返回明确结果并记 Info，不记 Error。PNG 不要求 PCM 或音频源文件，仍保护它们不被覆盖；无 FFmpeg 的图像/图形输出可用，视频仍由实际平台能力决定。 |
| SAM-PENDING-01 | 待完成 | 旧冻结组合读帧协议的显式能力边界、全链路默认故障信道及实际长时间播放性能。连续栅格化相机平面错误仍缺少默认文件/Error stderr 的端到端验收；不能将函数 Err 用例等同于完成。UI 开关及其失焦/撤销、双语和主/浮窗截图仍待接入。 |

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

下一步将逐接收层的父级/相机/效果源上下文接到局部纹理物化、嵌套曝光与 GPU 阶段合成，再接视频 pending 的持久任务及被取代请求取消；新的投影合成仍须由通用多时钟渲染链路驱动后做整段 3D 验收。禁止用整幅普通动画截图相加来连带模糊未启用图层，也不能在每次 Present pending 时从第一阶段重启而抖动解码请求。最终冻结协议、host 错误信道、真实性能和 UI 接入完成前不关闭 #255。

### 嵌套实例身份与视频缓存

旧代码按当前可见实例的 DFS 顺序重新分配别名。前方引用进入或离开 clip 时，后方持续可见的视频会换 ID，导致同一 PTS 的已驻留图像失去身份。现按完整工程引用路径和原始对象槽建立确定性别名，包含当前不可见的引用。缓存只保存有界源拓扑；透明、禁用和动画参数变化不重新展开引用。布局在临时对象中完整构建后替换，非法结构不能留下半张表。

别名区间避开所有源合成与根合成的原始对象 ID，包括通常的高位 ID 和 u64::MAX。重排/增删源层或更改引用目标属于结构变化，允许重建；同一结构的新建、复用及模型序列化重开结果一致。隐式相机 0 保留，其他源对象通过有检查的映射访问，不使用会 panic 的 HashMap 下标。

- `sampling-instance-layout-tests.log`：既有 composition_frame_boundaries 两项、compositions 五项通过；最初新夹具的六秒视频 clip 超过其五秒源区间，验证器按原定义拒绝。将夹具限定为 150 帧，未修改生产校验、既有断言或返回协议；`sampling-instance-valid-clips.log` 三项通过。
- `sampling-instance-boundaries.log`：四项新单元测试通过，验证别名避开原始高位 ID、新建/复用一致、结构变更、无效编辑后恢复、未知/重复对象和合成，以及 16 层、1,024 实例、256 合成、128 层的边界与超限拒绝。范围外/禁用引用也计入静态容量，未借可见性绕过上限。
- `sampling-instance-final.log` 保留误写测试目标 exposure 的命令失败；实际目标为 exposure_accumulation。修正后 `sampling-instance-correct-targets.log` 六组 24 项全部通过：边界 2、嵌套合成 5、曝光 6、双帧混合 5、新别名 3、视频实例 3；与单元用例合计 28 项，不将重复执行计为新增。
- 新 GPU 用例将三个不同源偏移的双层嵌套引用跨入出点往返三轮。每帧独立按原始引用 ID 与 PTS 计算必要上传数，对实际 renderer 上传计数和输出像素做断言；持续可见、PTS 未变的实例不重复上传。另一个用例比较复用 Scene 与模型序列化重开的新 Scene 的便携计划完整字节；这不是工程 ZIP 文件保存重开的验收。matte、效果图像输入、Null 父级/场景源的测试验证映射和世界矩阵存在，不将其称为所有生成器的像素验收。
- `sampling-instance-desktop-check.log`：带 FFmpeg 的桌面 cargo check 成功，保留既有 21 条桌面警告，无新增渲染模块警告；不是新的 release 可运行包。

`instance-alias-cost.json` 保留真实 GPU 捕获用例中的全部 30 次 CPU Scene 采样耗时、16 次必要上传/16,384 字节及跟踪纹理峰值 39,940 字节；耗时保留各轮实测值，不作为稳定性能保证。输入是测试明确提供的已解码 1fps 原创颜色帧，没有读取媒体文件；不将这些数值称为解码时延、真实播放 FPS 或完整进程内存，也不宣称用户首次遇到视频时的全部卡顿已经解决。

### 逐接收层的真实场景求值

ExposureDocument 对整份不可变文档校验一次，再由 Arc 在帧任务与子任务间共享；帧内不重复校验文档。ExposureScenes 保留每层自身时钟及显式的 Receiver / PhaseReady / Complete 状态。只有调用者完成局部图像保留后才能 finish_receiver，整阶段合成并成功累积后才能 finish_phase。媒体等待时重复 current 返回同一上下文；成功/透明状态都不会隐式推进，失败任务不能重试求值。取消通过丢弃任务释放快照，不宣称已完成尚未接入的 decoder/GPU 任务取消。

每个接收层使用其真实采样时刻调用现有 FrameEvaluator 与 Scene 路径，因而父级、相机、表达式、效果参数、遮罩、matte 和图像输入共享接收时刻；输入层自己的快门只影响它作为接收层时的采样，不改变被其他层引用时的时间。按时间分组仅优化求值顺序，最终图层顺序须恢复 DrawLayer.order 并交给原有空间合成；不使用求值顺序冒充层级。中心时刻的 Scene 单独保留，另一个 Scene 复用变化时刻，避免每阶段重算未模糊层，也避免保存所有样本场景。缓存按位区分正负零。

可将文档内任意子合成作为求值根。Scene 的内部 sample_root 将局部视图与原始文档分开，嵌套帧率、图像/视频资源和引用仍在原文档解析。实例布局将该根放在首位，并保留其他合成的原始对象 ID，避免把局部视图误当成缺少资源库的新工程。测试证明 60fps 子根到 24fps 叶子的分数源时间和别名一致；这尚不是父级曝光与子级独立曝光的卷积/组合实现。

保留的失败与最终验证：

- `sampling-receiver-check.log` 保留早期私有 prepare wrapper 的未使用警告；该 wrapper 仅旧模块单元测试使用，随后限定为 cfg(test)，没有通过 allow 隐藏新增生产警告。
- `sampling-receiver-tests.log` / `sampling-receiver-correct-trait.log` / `sampling-receiver-valid-borrows.log` 保留新夹具的 trait 导入、父链 move、索引类型和 ClipRange 字段错误。改用 motion_model::FrameEvaluator、克隆 ParentLink 及真实字段后编译通过，没有改变生产协议。
- `sampling-receiver-valid-range.log` / `sampling-receiver-valid-fixtures.log` 保留夹具使用了不合法的合成 ID、效果参数 ID、精确区间整数包络，以及仅动画了未创建的相机所致失败。改为 comp-*、实际 p0001、Layer::set_interval 和显式创建相机，保留原有模型校验和像素断言。
- `sampling-receiver-expressions.log` 保留新增表达式断言的错误预期：源码明确 `time` 是合成秒，原始 `value` 按图层 offset 读取。新夹具改为检验 `(t-1)+t/30`，没有改变现有表达式实现或原有测试断言。`sampling-receiver-final.log` 五组 19 项通过（当时新模块七项）；既有嵌套合成 5、帧率边界 2、FrameEvaluator 2、别名 3 共 12 项仍保留。
- `sampling-receiver-boundaries-final.log` 增加正负零/不可变 revision，以及合成末端采样后八项通过。随后只补结构化异常 details 的保护和对应拒绝/抑制求值用例；`sampling-receiver-errors-final.log` 同样八项全部通过，不将重复执行计作新增。与上述 12 项既有回归及 `sampling-receiver-layout-final.log` 四项有界布局单元用例合计 24 项。
- `sampling-receiver-desktop-check.log`：带 FFmpeg 的桌面 cargo check 成功；保留既有 21 条桌面警告，无新增模块警告。这不是新的优化构建或可运行包。暂存差异空白检查、样式消费者检查及来源检查（1,395 个受版本控制路径）通过；未新增依赖、素材或 UI 控件。

实际 GPU 对照位于 `E:/Dev/aem/artifacts/layer-render-sampling/receiver-context-final/`：`receiver-scene-sharp.png` 与 `receiver-scene-exposure.png` 由真实动画 Project 经新求值器、平面合成和线性累积生成，不再手动改矩阵来代替求值。`receiver-scene-comparison.png` 仅用这两张原始图加棋盘格、三倍最近邻放大和文字，没有生成或修改像素效果。上方半透明矩形展开拖影；下方同样有运动关键帧、但未启用模糊的参考条与普通中心帧逐字节一致。覆盖支持宽度增加且 alpha 面积差小于 2%，透明边界不重归一化。该夹具没有效果纹理或嵌套曝光，组装纯色 DrawLayer 的方式不能用于通用效果输入、扩边、matte 或生成器。

`receiver-scene-cost.json` 保留八阶段、九次真实 Scene 求值、196,608 字节曝光目标与该轮捕获耗时。计时包含求值、GPU 合成、累积和回读，排除 PNG 写盘，不包括先前建立的文档/目标及普通中心帧捕获；不称为播放 FPS 或完整显存成本。父级、表达式、相机、效果输入和 matte 的复杂用例验证实际 Scene/PlanBuilder 输入及 shader clock，未声称通用局部纹理物化已经完成。完整播放器、冻结输出、嵌套独立曝光与新路径的默认日志验收仍在待完成范围内。

仍待 #255 最终验收：嵌套组合内双帧混合的专门像素/成本用例、真实桌面播放与冻结导出的整段采样一致性、完整性能记录、最终能力边界、UI 开关、双语主/浮窗和焦点取消。上述小范围 GPU 与协议测试不能代替这些验收，也未用于提前关闭 issue。

未新增依赖、第三方资产、UI 栈或主题 token；未读取 Adobe 二进制，也未把参考产品截图放入产品。本 issue 的最终 UI 仍需 DESIGN §0.7 逐项处置及 §12 验收。

### 局部纹理保留与 GPU 阶段合成

LayerRaster 独立拥有局部 RGBA8 sRGB 输出与可选的投影 R8 matte；保存接收层自身时间的平面、扩边偏移、投影、透明度、混合和生成器 additive 语义。效果源与 matte 源按既有依赖顺序先保留 GPU 快照；新的目标强制物化不会误用普通预览缓存，也不允许以跳过失败效果的方式产出局部图像。矢量源保持连续栅格化的投影密度，不能通过普通尺寸的 utility pass 再降采样。纯色使用 1×1 白色源加原始浮点 draw uniform，原始尺寸仍用于平面几何；含 matte 的纯色同样保留颜色精度。matte 独立保留在合成视口分辨率，避免烘进局部图像后发生第二次空间重采样；源离开 clip 后仍保留零覆盖及反转语义。

后续采样、普通渲染和素材上传都不能覆盖 caller-owned 图像。draw_layer_rasters 恢复 authored order，再交给现有的平面/逐层相机 BSP；不使用调用数组或按时间求值的顺序代替层级。它沿用原有 normal/additive、matte 与 blend transfer shader，不新增 shader、公开计划字段或第三方依赖。纯色、图像、矢量、效果扩边、生成器和普通嵌套纹理均通过 GPU 重投影。普通嵌套场景仍在接收时刻渲染；这不等于子合成自身快门的独立曝光组合已经实现。

accumulate_layer_exposure 将独立局部图像组成完整阶段，再进入同一 FP16 线性累积器。阶段合成与累积是有序的两个 GPU submission，成功后才推进阶段，不做 CPU 图像回读；用于测试的捕获在完成后单独执行。目标的输入、输出与保留 matte 通过现有 opt-in submission ledger 合并记录，容量随所有者释放而归还，GPU 退休仍异步完成。原始裸 TextureView 绘制 API 沿用既有调用方负责输出资源账本的边界，不能据此宣称所有外部纹理已跟踪。普通效果路径的时间戳起点保持在源快照和 matte pass 之前；没有将这些阶段从既有 GPU 时间测量中漏掉。

这一阶段的边界：局部图像 API 拒绝 adjustment；旧 draw_layer_rasters / accumulate_layer_exposure 遇到可见 adjustment 仍显式拒绝。下一节的新入口将其作为逐阶段 backdrop 操作接入，没有改变这些已有拒绝断言。文档 revision、被取代请求取消、媒体 pending 的持久帧任务、未模糊层跨阶段纹理复用、子合成独立曝光、生产播放与冻结输出都仍由后续 host 任务接入。本轮接收层准备仍使用有界的完整 Scene/Plan 上下文，不宣称仅构建最小依赖集合，也未测量实际长段视频首次出现的延迟。

验证及保留的失败记录：

- `sampling-local-check.log` 记录新 helper 的模型错误到 RenderError 转换未显式处理；现保留原 Display 并追加 phase，不吞错。`sampling-local-tests.log`、`sampling-local-valid-fixtures.log` 记录新夹具误用了 Layer.color、Vector 元组、旧工程版本、阴影 distance 参数和低于允许下限的 scratch 策略。夹具改用真实模型字段、格式 21、offset 和有效的 64 MiB 策略；原有校验没有放宽。
- `sampling-local-valid-contracts.log` 记录连续矢量的实际像素差异及生成器 extent 夹具没有产生可见样本。前者通过保留投影栅格密度修复，后者将原创星场范围设到画面中。`sampling-local-density.log` 六项通过，没有放宽逐通道一档的对照断言。
- `sampling-local-constant-precision.log` 记录尝试从无 COPY_SRC 用途的普通缓存白纹理复制导致的真实 GPU Validation Error。改为独立常量目标的白色 clear，不改变所有静态素材的 usage。`sampling-local-constant-correct-usage.log` 九项通过，包括半透明纯色和原始 opacity 的逐字节对照、后续普通渲染后仍保留旧图像、所有 matte 模式及离开 clip 的零覆盖/反转。
- `sampling-local-final.log` 记录新增拒绝用例发现缺少效果定义时上下文缺少效果标识。生产错误保留原始 cause 并附加最多 96 字符的标识，未删去该测试断言。随后 `sampling-local-complete.log` 十一组 67 项通过（当时新模块十项），包含嵌套、曝光、接收场景、matte、矢量/adjustment、3D 投影、生成器、双帧、资源拒绝及提交账本。
- 恢复普通路径的时间戳原始范围、显式跟踪阶段绘制的曝光输出后，`sampling-local-ledger-final.log` 五组 34 项通过。新增 GPU 账本用例在累积前后检查恰好两次提交、全部完成、无丢弃快照及同次提交的局部输入/曝光输出容量峰值；不能只在后一次累积提交记录输出。新增可见 adjustment 明确拒绝，未修改旧渲染器的 adjustment 断言或行为。
- 最后在 GPU 分配前检查混合缓冲 12WH 的既有独立 LayerCompositing allowance，并复用既有驻留源预算检查；`sampling-local-budget-final.log` 十二项新用例和三项资源回归通过，断言拒绝前后工作纹理账本容量不变、旧局部图像仍可在恢复策略后使用。上述联合为 12 项新用例及 57 项既有回归，共 69 项；重复执行不另计。
- `sampling-local-desktop-check.log` 带 FFmpeg 的桌面 cargo check 成功，仍有既有 21 条桌面警告，无新增渲染模块警告。这不是新的 release 可运行包。暂存差异空白检查、样式消费者检查及来源检查（1,397 个受版本控制路径）通过；没有新增依赖、资产或面板样式。

GPU 原图位于 `E:/Dev/aem/artifacts/layer-render-sampling/local-raster-budget-final/`。`local-effects-sharp.png` 与 `local-effects-exposure.png` 来自同一真实动画 Project，经 ExposureScenes、局部效果物化、阶段 BSP/混合和 FP16 累积；包含真实 gaussian_blur，未用手动改矩阵替代取样。上方红色运动层扩大覆盖，alpha 面积误差小于 3%；下方同样有运动关键帧但未启用模糊的绿色参考保持中心帧。对比图只增加棋盘格、四倍最近邻放大和标签，未修改原始 GPU 像素。

`local-effects-cost.json` 记录 RTX 4070 Laptop/Vulkan、64×64、八阶段、56 次效果及平面 draw、49,732 字节 caller-owned temporal 峰值与该轮 14,832us 捕获耗时。56 不包含累积和 resolve 的 fullscreen draw。计时包括 Scene 求值、局部物化、GPU 合成/累积和回读，排除前面建立的文档/目标、普通参考捕获及 PNG 写盘；不等于纯 GPU 时间、实际播放 FPS 或总显存。其他阶段目录保留各轮原始成本值，没有用较快一轮覆盖慢值。

新模块暂未由桌面播放器或冻结协议调用，不能将返回 Err 的测试称为新路径的默认日志端到端验收。最终 UI、双语主/浮窗、焦点与右键菜单验收仍未开始；本轮没有面板/token 修改，不改变 DESIGN 的处置基线。

### 曝光阶段中的调整图层

新增 draw_exposure_phase / accumulate_exposure_phase，在每阶段的下方局部图像合成后运行调整效果。调整层本身没有独立可绘制源，不调用 materialize_layer；ExposureScenes 的调用者完成这一 receiver 后，使用中心 Scene 把调整操作插回作者顺序。调整参数、父级变换、范围、opacity 和引用素材保持请求的中心时刻；运动图层的已保留纹理与平面使用各自接收时刻。未启用模糊的动画层保持中心时刻。此设计不会把调整层以上的图像或项目背景一起模糊。

中心上下文先严格预检，递归检查普通嵌套依赖，不允许预览绕过未知或编译失败的效果。源效果快照、矢量、遮罩和投影 matte 使用现有准备过程，准备 encoder 先提交；之后才能把独立阶段几何上传到共享 vertex/uniform buffer，避免已编码的源操作读到后写的顶点。阶段执行调整效果时，将当前下方 accumulator 作为输入，并复用现有调整范围/opacity shader。每个阶段完成绘制后再累积，通常三次有序 GPU 提交；不回读阶段像素，不隐式重试。嵌套子场景的实际准备还可能有既有提交，不能把三次称为全部工程的固定提交数。

几何按作者顺序和既有 BSP 排序；调整层形成原有的 2D 边界。源图层在中心 clip 之外、但在某个曝光样本出现时，阶段仍加入该图层；若它新增混合 accumulator/source，则重新准备效果绑定，中心计划索引和阶段 uniform 索引分开处理。未出现的样本保持透明权重。空 backdrop、disabled 效果和零 opacity 保持原语义，背景在最后呈现时合入。旧 draw_layer_rasters / accumulate_layer_exposure 和 adjustment 不能物化为局部图像的拒绝断言均保留，只有新入口接受完整阶段。

本轮 GPU 用例还发现 BGRA renderer 的混合 source 固定创建为 RGBA，导致普通渲染参照自身报 pipeline/attachment 格式不匹配。CompositingGpu 现在保存创建时的输出格式，用于创建 source；仍为同样的 4WH 字节，没有增加容量或修改 shader/计划 ABI。新的 BGRA/交叉 3D/matte/混合/叠加调整测试同时与 RGBA renderer 比较最终通道，既有 RGBA 混合用例保留原断言。

资源仍受旧 allowance 限制：调整 accumulator 为 8WH，存在混合时连同 source 为 12WH；在新分配前检查，typed LayerCompositing 拒绝保留 required/budget。保留的局部图像、投影 matte 与 ExposureTarget 使用已有 temporal 计数；effect scratch 和素材预算单独计算，不将 temporal 峰值宣称为总显存。提交 ledger 测试检查中心准备、阶段平面与累积三次提交、回调完成、零遗漏快照及局部输入/曝光输出容量，不能只在累积提交记录目标。

验证与保留的失败记录：

- `sampling-adjustment-preparation-check.log` 保留尚未接调用入口时的两个未使用方法警告；完整接入后 `sampling-adjustment-phase-check.log` 编译通过且没有新增警告，未添加 allow。`sampling-adjustment-existing.log` 的既有局部图像 12 项及矢量/调整 9 项通过，旧拒绝断言未改。
- `sampling-adjustment-tests.log` 保留新测试遗漏 TrackMatte.hide_source 字段的编译失败；夹具补齐真实字段后运行。`sampling-adjustment-valid-fixture.log` 为新增七项中的六项成功，BGRA 普通参照触发真实 GPU 格式校验错误；没有把用例改为只测 RGBA 或放宽断言。修正混合 source 格式后 `sampling-adjustment-format-fix.log` 的新增七项和既有混合七项全部通过。
- `sampling-adjustment-boundaries.log` 新增九项全部通过，补充中心范围外接收层改变混合资源、嵌套失败效果拒绝/恢复，以及显式预览策略不符不推进阶段。
- 最终 `sampling-adjustment-final.log` 十二组共 78 项通过：新阶段调整 9、局部图像 12、矢量/调整 9、混合/matte 7、场景效果 8、嵌套 5、曝光累积 6、接收场景 8、双帧 5、投影 4、资源拒绝 3、提交账本 2。包含新增 BGRA/RGBA 通道对照；重复执行不计为新增测试。
- `sampling-adjustment-desktop-check.log` 带 FFmpeg 的桌面 cargo check 成功，保留既有 21 条桌面警告，无新增渲染模块警告。这是编译检查，尚未生成此阶段的 release 可运行包。
- 暂存差异空白检查、样式消费者检查及来源检查（1,399 个受版本控制路径）通过；DESIGN §0.7 面板基线本轮无变更，§12 的新 UI 验收仍待实际开关接入。

真实 GPU 原图及成本位于 `E:/Dev/aem/artifacts/layer-render-sampling/adjustment-phase-final/`：`adjustment-sharp.png` / `adjustment-exposure.png` / `adjustment-cost.json`。夹具的红色运动层位于 gaussian_blur + displacement_map 调整层下；调整参数和隐藏输入的 brightness 都有关键帧，隐藏源还独立启用三样本快门。实际 ExposureScenes / 局部物化 / 新阶段入口与独立普通渲染 oracle 对比：oracle 仅换入逐时刻接收平面，保留中心调整与引用素材；另有错误整图时间平均，断言其确实与正确结果不同。上方红色覆盖宽度增加两倍以上，alpha 面积差小于 3%；同样有运动关键帧的上层绿色参考条保持中心帧，最终像素按通道一档对照。`adjustment-comparison.png` 仅给两张原图增加棋盘、四倍最近邻放大及说明，未生成或改写原始效果像素。

RTX 4070 Laptop / Vulkan 的该轮成本为 64×64、10 个共同阶段、190 次准备/效果/平面 draw、65,544 字节 caller-owned temporal 峰值、29,384us 捕获。draw 数不包括曝光累积和 resolve fullscreen draw；计时包含真实 Scene 求值、局部物化、中心依赖准备、GPU 阶段合成/累积及回读，排除此前建立文档、目标、普通参照和 oracle，以及后续 PNG 写盘。这是一轮捕获，不等于实际播放 FPS、纯 GPU 时间或总显存。早期各阶段目录保留各轮成本，没有以较快值覆盖本轮数据。

边界仍然明确：调用者必须在新入口之前准备好精确中心时刻的依赖媒体。现有 ready 检查能拒绝未提交/已取消/缺失的转换，不能证明所有驻留视频纹理的 PTS 恰好等于当前中心请求；后续持久任务必须协调解码 stamp。当前准备仍使用完整有界中心 Scene/Plan，可能重复准备未使用或未变化的源，不宣称最小依赖或跨阶段复用完成。嵌套独立曝光、文档 revision/取消、生产播放/冻结协议、新路径默认日志及 UI 实际开关验收均未完成，#255 保持开放。本轮没有依赖、资产、面板或 token 改动。

### 持久帧任务、精确媒体与冻结 MP4

此节更新上一阶段的接入状态。ExposureFrame 现在把 ExposureScenes、ExposureTarget 和当前阶段 LayerRaster 保留在一个任务内。媒体 Pending 时停在当前接收层或中心准备阶段，不重新求值同一 Scene，不丢弃先前图像，也不重复累积已完成阶段。带调整图层的阶段会在合成前重新准备精确中心媒体，避免保留了别的接收时刻视频而误当作中心素材。时间切片在工作单元之间检查，每次最多 128 单元；它不是同步求值、驱动或外部 callback 的抢占式超时。

ExposureMedia 复用现有素材准备、VideoFrames 与 PreviewVideoCache，不发推测预取；按实际相邻 PTS 和相同质量策略上传、提交转换后才允许局部物化。真实 VFR 阻塞用例证明反复等待不修改 generation，整个四阶段只解码 100ms/170ms 两帧，不误发 400ms；预览代理和完整素材生成同一原创常量夹具像素。这里验证的是 helper，不是尚未接入的 Session 播放器。

FrozenMovie::export 对需要曝光的冻结工程共享一次验证的不可变文档，每输出帧建立持久任务并等待完整累积，再回读给原 H.264/AAC 编码器。未启用快门的工程继续走原普通路径；不为曝光路径多分配普通 capture target。取消仍由既有 ExportControl 检查，已有输出只有最终验证和发布后才替换。新增实际取消测试在首帧已编码后取消，原目的文件保持不变、临时 MP4 清理、没有最终成功进度。等待阶段沿用原有 30 秒解码期限，进度改变才重新计时，未增加任务重试、线程重启或隐式失败恢复。

错误终态保留首要原因及 typed ResourceRejection，并清除部分局部图像，禁止捕获未完成目标。第一次返回保留完整 cause；后续仅保留该任务的至多 2,048 字符诊断，丢弃任务后释放。GPU 所有者不符及零时间切片属于调用参数错误，未调用媒体、不污染有效任务。已消费/失败的 Scene 调度拒绝再次用于构造新任务。资源/粒子统计取跨上下文峰值，工作计数在单帧内饱和相加；cpu_prepare_us 累加 poll 内真实墙钟，包含回调和求值，排除两次 poll 之间等待，不能解释为纯 CPU 准备或纯 GPU 计时。

独立子合成快门仍未实现。构造前检查整个可达工程，包含当前 clip 外的子层，防止它在自己的快门范围内出现而绕过仅检查中心 Scene 的判断。遇到此情形保留子合成/对象和 nested_sampling phase 并明确拒绝，未引用的子合成正常忽略；普通嵌套图像仍可按接收时刻渲染。这是有待消除的实现边界，不能用来提前关闭 #255。

验证及保留的失败记录：

- `sampling-frame-task-check.log` 保留新 sibling 模块直接访问 ExposureTarget 私有尺寸的编译失败；通过只读 size() 使用尺寸，未公开内部纹理。`sampling-frame-task-tests.log` 保留新测试误从 motion_core 导入 FrameEvaluator 的编译失败；改为真实 motion_model trait，未修改生产类型或断言。
- `sampling-frame-task-valid-import.log` 六项新 GPU 测试通过。随后增加部分消费调度拒绝、失败后局部图像释放及有界原因，并将嵌套检查提前到整个可达源图，覆盖中心范围外夹具；`sampling-frame-task-boundaries.log` 三组 25 项全部通过：新持久任务 8、既有接收 Scene 8、阶段调整 9。包含同一任务 60+60 次等待、极小正时间切片、实际低源预算上传拒绝、原始错误/typed 资源拒绝、终态不重试及显式新任务恢复。
- `sampling-frame-task-media-tests.log` 两项新 host 测试通过；`sampling-frame-task-host-final.log` host 全部 43 项通过（含这两项），未把重复运行计为新增。覆盖必要右帧实际故障、generation、完整/代理 GPU 输出、错误后 60 次轮询不再解码、显式清理 reader 并新建任务才恢复。
- `sampling-frame-task-movie-regressions.log` 保留第二个新真实媒体夹具没有先创建索引父目录导致的 os error 3；修正夹具目录，没有改 save_index 的失败协议。`sampling-frame-task-movie-final.log` 11 项通过。增加独立曝光取消用例和可选真实产物保留后，`sampling-frame-task-movie-artifacts.log` 12 项全部通过；原始普通导出、嵌套视频/效果/表达式/PCM、动画锚点、容器回读和取消用例未改断言。
- `sampling-frame-task-default-logs.log` 保留新测试错误地要求不存在的逐次 `video decode start` Info 而失败。现有协议在 worker 打开源前记录一次 open Info，逐次实际 decode 在调用前发布 in-flight，健康 Present 不产生每帧 Info。新测试改为精确验证现存 open 的 object/source/sequence/PTS、恰好一次 open，以及被阻塞必要右帧的 Decode in-flight（PTS 33333、sequence 1、非 speculative）；未为满足断言添加逐帧日志。`sampling-frame-task-default-final.log` 两项真实 FileLogger 子进程通过，包含旧用例的成功/正常 miss 重置和第二轮升级，以及新曝光路径 120 次等待仅第 60 次一次 Error、实际 codec 失败一次 Error、Error 同写文件/stderr、Info 不写 stderr、stdout 隔离、失败终态 60 次不重试。这是既有 decoder 可观测性经新 helper 的验证，不是新增日志机制，也未宣称所有渲染错误已完成前端验收。
- 上述最终用例合计 82 项：新用例 14、既有回归 68，重复执行不累加。`sampling-frame-task-desktop-check.log` 带 FFmpeg 的桌面 cargo check 成功，保留既有 21 条桌面警告；日志子进程测试构建保留其既有 19 条测试配置警告。没有通过 allow 隐藏新增模块警告。这些检查不是新的 release 可运行包。
- `sampling-frame-task-without-ffmpeg.log` 未启用 FFmpeg 的媒体库编译检查通过，无新增警告；普通 export_destination 的原条件编译保留。暂存差异空白检查、来源检查（1,402 个受版本控制路径）和样式消费者检查通过。

真实产物位于 `E:/Dev/aem/artifacts/layer-render-sampling/frame-task-final/`。`receiver-exposure.mp4` 和 `linear-video-exposure.mp4` 都是独立原创 64×64、30fps、六帧工程的实际 H.264/AAC 输出，测试重新用真实解码器读取，不以合成 PNG 代替编码验收。前者冻结后修改实时工程的 LayerSampling，输出仍保留红色运动层的八样本快门，绿色动画层保持中心时刻；首帧透明边界不重归一化。后者先编码红/蓝源视频，真实索引、相邻帧解码、GPU 线性混合与曝光后再编码输出，第二帧索引处独立按线性色彩空间计算 75% 红/25% 蓝；每通道允许 12 档编码误差，足以区分错误的 gamma 像素平均。

目录保存两段原始 MP4、解码出的 RGBA8、对应 PNG 和各自成本 JSON；PNG 仅做 RGBA/BGRA 通道布局转换。`frozen-movie-comparison.png` 仅给原始解码图加四倍最近邻放大、间距和文字，未修改效果像素，也不是产品 UI 截图。该轮六帧输出分别耗时 568,560us（receiver）与 601,826us（video），文件分别为 2,767 / 2,064 字节，编码器 libopenh264。计时包含 FrozenMovie::export 的 renderer 初始化、媒体准备、GPU、音频、编码、容器验证及发布，排除此前 freeze、事后解码验证和产物写盘；不是播放 FPS、纯 GPU 时间或所有工程的性能承诺。每次测量使用独立目录，没有覆盖先前阶段的慢值。

待继续：递归嵌套独立曝光；Session 桌面预览/capture 的持久任务与 revision/质量/视口身份；被取代和失焦取消；冻结组合读帧协议；GPU/求值故障默认日志和前端信道；真实播放成本与 UI 能力开关、双语主/浮窗/右键菜单截图。当前没有依赖、第三方资产、面板、主题 token 或产品文案变更，DESIGN §0.7 的面板处置保持原状态，§12 的新 UI 验收仍待接入。#255 保持开放。

### 递归嵌套曝光与解码器复用

此节更新上一阶段的临时 nested_sampling 拒绝。ExposureFrame 现在依次运行子引用的独立曝光任务，源帧由原始 Scene 的精确 CompositionClip/fps 映射取得；每个子级按自己的快门和帧率采样。父快门改变子级中心源帧，再组合子级自己的曝光，未用普通嵌套图像或整幅中心帧平均替代。子级完成后 GPU resolve 到一幅 4WH 图像，不回读像素。父级 materialize/中心调整使用保留原世界矩阵、效果、LUT、源映射与素材表的 local_context，去掉已经独立处理的嵌套绘制入口，防止普通 prepare_nested 覆盖结果。旧普通 Renderer API 语义没有改变，尚未接入的桌面预览也不能因此声称支持运动模糊。

每个父任务只保留一个正在推进的子任务；完成的兄弟保留解出的 GPU 图像。子目标的 12WH、局部图像和待导入的 4WH 输出共同使用现有 temporal 额度。4WH 在分配前预留；父媒体就绪后，将同一个 OwnedTexture 转移给 renderer 的 NestedCompositions 源缓存，先检查原源预算，再释放 temporal reservation。没有像素复制，也没有预算外幸存的纹理所有者。新任务 ledger 合并全部活跃子级、未导入结果、局部图像与累积目标；同 allocation 的身份在导入前后保持一致，提交时仍由既有 ledger 记录异步退休。源码中的 source 与 temporal 是不同 allowance，不等于总显存限制。

所有递归作用域共用一次顶层 poll 的时间和 128 个工作单元预算。子任务构造、子级 resolve、接收层准备/物化及阶段合成分别使用额度；一个接收层批次内部仍可能编码多次 GPU 操作。遍历等待中的父级不先消耗第一个动作额度，因此极小的正切片也会逐步抵达最深的可操作节点；测试验证 1ns 与 1s 切片的完成图像逐字节一致，单次轮询全部子级合计最多 128 个完成单位/准备回调。它不是 evaluator、driver 或 callback 的抢占式超时。work_units 只随真实构造/完成推进而饱和增加，媒体等待不增加，作用域间的推进会向上传播以供冻结输出期限判定。

子任务共享 immutable 文档、同一个 evaluator 和以原求值根建立的引用路径布局。root 对象保持原 ID；子根的 receiver/shutter、节点、matte/效果源及后代映射到原全路径别名，保持与普通 Scene 的同一实例身份。不能把每个子合成重新当作独立根编号，否则重复引用/不同视频会互相覆盖。Scene 构造继续保留原深度/实例校验；各作用域仍保留有界的完整场景，累积的 CPU 上下文数量大于单个 Scene 的 64 活跃实例限制。本轮没有实现最小依赖求值、跨阶段图像复用或 CPU 内存峰值测量，不能据预算测试声称全部进程内存有统一上限。

仅保留 ID 还不够：旧 VideoFrames.prepare 会在进入没有本地视频的父层时释放子流，后续阶段又打开素材。新增 sampling_exact 准备路径只淘汰需要腾出容量的最久未用非当前实例，最多保留四个流，live + retiring 仍不得超过原四 worker 上限。每流缓存预算、必要双帧与失败终态规则保留，没有后台预取、失败重试或 worker 重启。普通 prepare_scene / prepare_scene_exact 沿用旧 inactive 释放协议。最近使用列表最多四个数字，retire/clear 时移除；项目替换和显式 reader 清理仍释放全部流。这是功能与生命周期修复，不包装成仅加日志。

证据与保留的失败：

- `sampling-nested-task-check.log` 保留抽取饱和统计 helper 时遗漏多行 self.stats 的编译失败；修正参数引用，未放宽统计类型。`sampling-nested-task-existing.log` 的七项旧持久任务通过，剩余一项按原临时合同要求拒绝子级快门而失败。实现此功能后明确替换该临时拒绝用例：现在必须真正完成且范围外图像全透明；还保留未引用源不参与渲染、释放容量等断言，并新增中心之外跨入快门的真实像素验证。没有把旧预算、错误或取消断言删掉以求通过。
- `sampling-nested-task-tests.log` / `sampling-nested-scopes.log` 最初四个新 GPU 用例及既有三组通过；`sampling-nested-effects.log` 六项通过，补充效果、父 matte/调整、预览、BGRA 和 2048² 子结果的共享预算拒绝。实际低源预算错误随后增加 child composition、reference、source frame / nested_install 上下文；`sampling-nested-error-context.log` 通过，typed ResourceRejection 和失败终态不重试断言保留。
- `sampling-nested-final.log` 保留命令误写 video_instance_ids 测试目标的失败，未运行测试；按实际文件 video_instances 修正。`sampling-nested-final-targets.log` 九组 66 项通过：当时嵌套曝光 7、持久任务 8、接收场景 8、阶段调整 9、普通嵌套 5、局部图像 12、矢量/调整 9、双帧 5、视频实例 3。`sampling-nested-layout-final.log` 四项原布局边界单元测试通过。
- 递归时间/工作单元改为顶层共享后，`sampling-nested-shared-budget.log` 的持久任务 8、当时嵌套 7 通过。增加精确单次总额度、1ns 最终完成与 1s 输出一致性，以及后续无效零切片不能覆盖已存首要错误的断言后，`sampling-nested-shared-final.log` 四组 33 项通过：嵌套 8、持久任务 8、接收场景 8、阶段调整 9。新取消用例在最深媒体未就绪时读出三层目标的完整 ledger，连续 60 次没有推进，丢弃父任务释放全部 temporal 所有权；这不是尚未接入的桌面失焦/被取代请求取消验收。
- `sampling-nested-host-final.log` 及共享预算后的 `sampling-nested-host-shared-final.log` host 全部 45 项通过，含两项新实际 worker 用例。三个嵌套视频引用的 Scene 普通别名与曝光任务保留流集合一致，连续三次新建帧任务只发生各自必需的两次解码（合计六次），没有父层访问导致的重复解码；第五/第六实例按最近使用驱逐，live + retiring 每轮都不超过四，清理后最近使用列表为空。旧阻塞、失效、原始原因、默认观测、reader 退休及双帧协议测试仍通过。
- `sampling-nested-movie-final.log`、最终 `sampling-nested-movie-shared-final.log` 的真实 Movie 13 项通过。新用例在原相邻视频夹具上增加 30fps 父级两样本快门、60fps 子级八样本快门；根帧 2 映射到子中心 3.5 / 4.5，独立手算累计蓝色权重 0.15625，实际 H.264 回读通道误差不超过原有 12 档编码容差。无嵌套旧夹具仍要求原 0.25 权重，不改变原像素容差。
- `sampling-nested-default-logs.log`、最终 `sampling-nested-default-shared-final.log` 两项实际默认 FileLogger 子进程通过：原 decoder open Info / 先发布 in-flight、Error 文件和 stderr、stdout 隔离、第 60 次一次升级、恢复重置及失败终态抑制。它验证新 sampling_exact 经既有解码观测，未新增日志机制；不将函数里的嵌套 GPU/求值拒绝称为默认文件和前端信道全部验收。
- 上述独立用例共 131 项：renderer 67（含八项新嵌套）、布局 4、host 45、Movie 13、默认日志 2；十一项新用例，其余包括一项按已实现功能更新合同的旧用例，重复运行不累加。最终共享预算后的 FFmpeg 桌面 cargo check 与不启用 FFmpeg 的媒体库 cargo check 通过，分别见 `sampling-nested-desktop-shared-check.log` / `sampling-nested-without-ffmpeg-shared.log`。仍保留既有 21 条桌面警告及测试配置的 19 条警告，未添加 allow 隐藏新警告；不是新的 release 包。

真实 GPU 原图和成本位于 `nested-shared-final/`。三层/重复引用夹具经真实 model、Scene、GPU 局部物化、递归 resolve、父级阶段合成和 FP16 累积输出；独立 oracle 用公开下层协议递归捕获子图，反预乘后重新上传为普通图像，再合成父图，故测试 oracle 有 CPU 往返，生产路径没有。每通道最大两档容差覆盖 oracle 额外 RGBA8 读写舍入；另一个效果夹具覆盖父级 matte/调整，BGRA/RGBA 原生路径以一档比较。夹具验证半透明图形及父子快门，另有中心 clip 外的子层在自身曝光里贡献像素、未模糊同级绿色条逐字节保持中心帧的测试。

该轮 RTX 4070 Laptop / Vulkan、64×64、278 个递归完成单位、168 次效果/平面 draw、245,764 字节已跟踪纹理峰值、112,349us poll 加最终回读；不含此前顶层初始化、oracle 和写盘。之前 `nested-task-final/` 同图轮次为 319,361us，原图 PNG SHA-256 与最终共享预算轮次一致（930c87a955761394d248cf14c47df98f523b37be1829f18877d944e6f0679a40）。所有阶段目录保留各轮数值，不以快值覆盖慢值，也不据此声称播放 FPS、纯 GPU 时间或总 VRAM。`nested-task-final/nested-comparison.png` 仅用于显示这两幅原始输出，做四倍最近邻放大及线性透明棋盘合成、加标签；原 PNG/成本不被改写。这不是产品 UI 截图。父级也开启快门时整个子组随父级源帧变化，所以该图绿色也有父级拖影；这与只启用子层快门时绿色同级保持清晰的专门用例不同。

`nested-movie-shared-final/nested-video-exposure.mp4` 是六帧、64×64、30fps 的实际 H.264/AAC 输出，另保留解码 RGBA 与成本 JSON。最终该轮 export 耗时 682,547us、2,074 字节、libopenh264；计时包含 renderer 初始化、媒体、GPU、音频、编码、容器检查和发布，排除 freeze、事后解码和产物写盘。前一完整轮 743,418us 仍在独立目录保留。原始 Movie 容器/PCM/动画锚点、冻结后编辑不影响输出及取消保留目的文件的测试也通过。

暂存差异空白检查、来源检查（1,404 个受版本控制路径）及样式消费者检查通过。

仍待 #255：Session 播放/capture 的持久任务和 revision/质量/视口身份，被取代/失焦取消与实际媒体 worker 生命周期，冻结组合读帧协议，完整默认故障信道，真实播放成本与深层上下文/重复准备优化，以及最终能力/UI、双语主/浮窗/右键菜单截图。独立嵌套曝光核心与冻结 MP4 已接入，不能据此将整个 issue 标为完成。没有依赖、第三方素材、面板或 token 变更；DESIGN §0.7 的既有面板基线保留，§12 的 UI 验收仍待接入。

### Session 桌面预览、播放等待与取消

此节更新上一节的桌面预览待办。Session 创建时即按工程 revision/活动合成缓存一份已验证的 ExposureDocument，sample 时按身份同步；未需要曝光的工程只缓存判断，不复制完整工程。同一曝光请求的后续 Present 共用 ExposureFrame。请求同时比较帧、渲染尺寸、preview/full 模式、内存质量、view revision、surface epoch/尺寸、viewport、canvas image 和观察矩阵。工程替换清理文档与请求，detach 仅释放 GPU 请求、保留不可变文档；编辑/切换合成重新建立文档。这样初次打开和重新 attach 返回的 exposureRequired 也准确，无需先 Seek 才知道预览需要等待曝光。未完成结果保留在任务内，完整 GPU resolve 写入 PreviewFrameTargets 的候选，只有实际成功提交才交换 front；等待期间使用现有 present_chrome 绘制原画面和界面，返回 false，不更新 lastPresentedFrame。

自动播放曾在每次 Present 返回 false 后继续提交新 Seek，这会让跨轮询曝光一直从头开始。Shell 现在按有合成内容的窗口记录尚未确认的曝光帧；普通 chrome-only 窗口回复不能清掉另一窗口的等待。重绘/唤醒判断也读取这份窗口等待集合，防止其他窗口把旧的全局 pending 清成 false 后，曝光再也没有机会继续推进。实际完成后继续按既有音频/墙钟计算下一帧，允许丢帧追上时间，不改变播放速度或增加 Seek 队列。手动 Seek 的 action 路由仍正常执行，并明确取消旧宿主请求。关闭/回停靠浮窗和 worker 关闭清理对应等待，隐藏合成后的 chrome-only 提交解除该窗口等待。

取消不清空所有已解码帧：VideoFrames 仅退休 sampling 最近作用域中仍在 native 调用或缺少必要帧、且尚未失败的流。已完成缓存和失败流保留；一个失败流不能因取消而隐式重新打开。退休线程仍占原四 worker 额度，只有实际结束才 reap。原始取消与 decoder 的 superseded 检查保留，native 调用取消后返回的其他实际错误仍由既有 worker 日志记录。取消/清理错误继续返回，失败请求不在下一次 Present 重建；若清理也失败，追加原因并保留首要失败和 typed ResourceRejection。

内存不足继续使用现有四档预览策略和 30 秒冷却探测。每次策略明确放弃的任务先释放 owned temporal 资源，再回收 renderer 工作集；有旧 front 时保持旧画面。首次渲染/媒体/GPU 错误带 composition、revision、frame、size、view、surface 及底层阶段/对象，后续同请求仅保留 2,048 字符诊断；不捕获吞掉 panic，不改 respond，不新增失败重试或线程重启。

诊断现在区分合并的媒体准备/上传耗时与普通视频路径的分别计时；跨 poll CPU 累加包括任务初始化、callback、GPU 编码和 resolve，排除轮询间等待。成功帧 FrameRecorder 与快照加入 task-owned exposure 容量，Session ledger 包括全部递归任务。曝光包含多次独立 GPU 提交，本轮没有收集覆盖全部提交的时间戳，故 gpuTimingActive=false、gpu sample 保持空；不以呈现 pass 或 CPU 墙钟代替纯 GPU 时间。8ms 只在有界批次间检查，首次管线编译、表达式和 native 调用仍不可抢占。

证据与失败记录：

- `sampling-preview-first.log` 保留初版误用跨 crate 私有 with_diagnostic/texture、测试误读 Layer.color 的编译失败；改用公开 typed error/ledger 和 Content::Solid。`sampling-preview-gpu.log` / `sampling-preview-gpu-final.log` 记录测试 ledger ID 的 String 类型与 iterator 尾表达式生命周期错误；修正夹具访问，未公开 GPU 内部字段或删除像素/身份断言。
- `sampling-preview-candidate-tests.log` 三项真实 headless GPU 用例通过：极小切片不会提前提交 front；与独立普通逐时刻 Scene oracle 比较；同 revision 文档 Arc 身份不变；帧/视口/质量等切换释放旧任务；真实尺寸预算拒绝在 60 次相同请求后保持终态和 typed cause，显式内存策略处置后才允许新任务。
- `sampling-preview-host-full.log` 保留新增取消测试对不实现 Debug 的成功返回类型调用 unwrap_err 的编译失败；改为读取同一 Err，未修改生产类型或失败预期。`sampling-preview-host-final.log` 全部 49 项通过，含三项预览 GPU 与一项新的真实解码取消测试；后者验证 pending 退休、完成缓存/generation 保留及失败流在 60 次取消/准备后不重启。
- `sampling-preview-native-first.log` 的首次原生测试误读根级 video，而实际字段是 preview.video，因 in-flight 断言失败；改为实际协议路径，并检查 object/source/PTS/decode 阶段。`sampling-preview-native-correct-snapshot.log` 隐藏原生主窗/浮窗 × 成功/失败四种组合通过。阻塞期真实 Present 继续绘制 chrome，不确认合成，不改阶段；实际 Seek 释放曝光资源；失败回复含原 cause，默认文件与 Error stderr 有证据、stdout 无泄漏，第二次失败请求不增加 CPU 工作。
- `sampling-preview-native-pixels.log` 在上述矩阵加入动画位置与真实截图回读后通过。CaptureUi 从已提交的 g.scratch 经实际 Presenter 输出，32×32 一比一裁切；独立 oracle 把 authored 白色视频替换为常量白色平面，以四个普通 Scene 在 1.25 帧附近采样再累计。主窗/浮窗像素通道误差都不超过一档，证明实际 front/格式/呈现链路，不只检查任务内部结果。截图路径没有调用尚未支持曝光的正式 export::capture。
- `sampling-preview-playback.log` 六项播放/曲线/拖动测试通过，含新窗口等待用例；普通指针移动、已确认输入帧、手动 action 路由、其他窗口回复及错误/隐藏后的释放都保留断言。没有修改既有播放测试来适应新行为。
- `sampling-preview-desktop-check.log` 带 FFmpeg 桌面 cargo check 通过，保留原有 21 条警告；测试构建保留 19 条既有警告。当前不是新的 release 可运行包。

本轮最初真实预览产物在 `preview-native-first/`：`preview-false.png` 为主窗，`preview-true.png` 为原生浮窗，均是实际已提交预览的 32×32 像素裁切。旁边 JSON 记录该轮状态：主窗 CPU poll 31,758us、媒体合计 2,115us；浮窗 20,658us / 1,742us；均四阶段、八完成单元、12,288 字节曝光容量、21,608 字节已跟踪预览纹理容量。这是原生测试中的单次样本，不是播放 FPS、纯 GPU 时间、峰值 VRAM 或跨设备保证。该轮 JSON 的 gpuTimingActive 仍报告 timer 对象存在，随后已修正曝光路径不收集完整 GPU 时序的声明；保留原始轮次，不覆写成本和旧状态。

完整桌面回归 `sampling-preview-desktop-full.log` 为 531 通过、0 失败、1 项原有忽略（232.96 秒）。此后复核补齐多窗口等待的重绘/唤醒条件，`sampling-preview-redraw-tests.log` 六项播放测试通过；再补齐初始 Session/attach 的 exposureRequired。最终实现的 `sampling-preview-desktop-complete.log` 完整带 FFmpeg 串行桌面回归为 531 通过、0 失败、1 项原有忽略（203.26 秒），包含主窗/浮窗实际预览像素、取消和默认失败日志测试。`sampling-preview-measurement.log` 两项通过，旧容量分布断言保持不变，新增独立曝光容量/合计媒体时间与空 GPU 样本校验。

最终真实图像与该轮 JSON 在 `preview-native-complete/`；主窗 CPU poll 17,645us、媒体合计 1,421us，浮窗 21,265us / 1,880us，gpuTimingActive 均为 false。维度、阶段、逻辑容量与上轮一致。中间 `preview-native-final/` 的 21,841us / 1,631us 和 21,589us / 2,217us 也保留。成本含实际请求初始化/poll/提交 resolve，排除轮询间等待及事后截图回读；不把较快轮次替代旧值，不声称稳定播放性能。

`sampling-preview-host-initialization-final.log` 为最后初始化/清理实现的完整宿主测试，49 项全部通过（78.82 秒）；最终 FFmpeg 与无 FFmpeg 桌面 cargo check 都通过，分别见 `sampling-preview-initialization-ffmpeg-check.log` / `sampling-preview-initialization-noffmpeg-check.log`。总计 582 项通过、1 项既有忽略，含七项新增用例；专项和重复全套不重复计数。最终主窗/浮窗 PNG 的 SHA-256 相同：f27c5946b2ea9fe4d5cafe7cb83f6de96e2bf0db29e3acb4237c9424b67f18ab。已目视查看实际 32×32 输出；它是预览像素验证夹具，不是最终开关样式或完整产品界面截图。来源检查（1,406 tracked paths）、样式消费者和暂存差异空白检查通过。

当前不关闭 #255：正式帧 capture、冻结组合读帧、多种 GPU/求值拒绝的默认前端信道、长时间实际播放性能、深层上下文/重复准备优化、最终能力和 UI 开关仍未完成。没有第三方依赖或素材、面板布局、产品文案、右键菜单或主题 token 变更；DESIGN §0.7 十二项面板基线均 retained（本轮不编辑面板），§12 的新开关验收仍待实现。

### 桌面正式 PNG 导出

此节更新前一阶段的正式 capture 待办。桌面 Engine::Capture 不再调用会复用预览 renderer 的 host::ops::export::capture，而是捕获 FrameFreezePlan，在独立 worker 冻结、渲染、校验和发布。普通 PNG 同样等待必要视频帧；运动模糊 PNG 复用 FrozenMovie 的持久 ExposureFrame 驱动，使用 full 模式、作者样本数和完整图像分辨率，保留小数帧。预览仍有最多四个曝光样本的既有质量策略；不声称降级预览与完整八样本输出逐像素相同。

冻结复用原 MovieFreezePlan 的源文件句柄、复制前/后长度与修改时间检查、私有目录和效果注册表快照。新增 visual 模式跳过音频源和 PCM 的读取与验证；仍将音频路径纳入目的文件保护。实际带音频图层、音频源及 PCM 均不存在的图像工程可导出 PNG。视频索引和素材仍在冻结范围内，无 FFmpeg 构建不假称支持视频。`image` 只是复用 workspace 已锁定的 PNG 编解码依赖，未引入新版本、源码补丁、外部图像或其他第三方资产；PROVENANCE/ATTRIBUTION/NOTICE 已核对。

输出先写同目录临时 PNG，同步文件并解码比对尺寸/全部 RGBA 后才发布。发布和取消共享原 ExportControl 锁，成功取消不能再覆盖已有目的文件；发布成功后 cancel 返回 false。失败含原 cause、revision、frame、path；实际 decode 错误沿用 object/source/PTS 的原媒体上下文。新后台 worker 继承 originating request ID，Info async start 和独立 in-flight 在进入复制/渲染前发布；正常 done 为 Debug。`desktop.frameExport` 提供 request、operation、elapsedMs、progress 和真实 worker 终止状态。它与串行 engine in-flight 分开，不将空闲引擎描述成仍在处理 Present。

每引擎仅保留一个 PNG worker。VideoFrames::finish 在后台停止并 join 所有解码器，冻结源目录和 worker 名额保留到 native 调用实际退出。取消不靠 detach 后马上开始新线程来绕过额度；既有 30 秒解码进展期限不等于能抢占 native 调用。若 native 永不返回，该 export 保持 in-flight 且拒绝新的 PNG，界面和串行引擎继续工作；不重启或重试。渲染 panic 经过原 panic hook 后转为带原因的失败，随后执行 decoder cleanup；清理失败追加到主失败。回收失败、GPU 驱动永久阻塞的实际端到端注入仍待补充，不能据现有解码 gate 用例宣称所有外部阻塞都已覆盖。

界面先选择目的路径再启动任务，状态栏使用既有样式显示导出/保存/取消；“取消视频导出”改为通用“取消导出”，PNG 不再经过错误的“正在导出视频”文案。排队前取消与运行中取消均为正常结果，保留返回信道并记录 Info，不走 Error 或“操作失败”。正常文件菜单不是右键菜单；本轮没有新增渲染开关或修改面板布局、行密度、点击容差和主题 token。DESIGN §0.7 十二项均 retained，理由为未编辑其列出的面板；这不表示旧违规已修复。§12 的开关及原生浮窗交互仍待后续实现。

验证记录（日志均在 `artifacts/property-selection-context/`，图像在 `artifacts/layer-render-sampling/`）：

- `png-capture-first.log` 保留测试误用 Content::Asset 的编译失败；改为模型真实的 Content::Image。`png-capture-second.log` 保留夹具遗漏版本 21、资产未放 assets 目录的校验失败；修正夹具，不改模型校验或输出断言。`png-capture-fixtures.log` 无 FFmpeg 的三个用例通过：独立八时刻普通 Scene oracle、冻结版本及 1.25 小数帧、源路径被替换后仍使用原句柄、未启用模糊的同级保持清晰、缺失音频不妨碍 PNG、预算失败/取消不改已有文件、保护工程源。
- `png-native-first.log` / `png-native-build.log` 是误把二进制名当 package、误用不存在 desktop/Cargo.toml 的命令错误；正式命令使用根 workspace 的 `-p motion-desktop`。`png-native-test.log` 的真实原生 decoder gate 子进程通过：普通/曝光成功、曝光失败、取消四种组合，等待时状态查询/Seek 能返回，in-flight 和 Info start 已可读取，不发布部分输出；重复提交拒绝；取消仍等待 native 关闭。实际错误同时进入默认日志和 Error stderr，不泄漏 stdout。
- `png-media-full.log` 带 FFmpeg 的媒体 crate 全部 28 项通过。真实相邻 PTS 的 PNG 独立线性 oracle 为 RGB [225,1,137]，实际完全一致；父子快门在 30/60fps 下的 oracle [237,1,110]，实际 [237,1,111]。原 MP4 断言不变，在其用例中增加 PNG 对照。原始 PNG/JSON 存 `png-real-video/`，两次完整 PNG 任务耗时 692,801us / 740,683us；含素材复制、独立 GPU 初始化、媒体解码、所有阶段、回读、PNG 编码验证及发布，不是纯 GPU 时间、播放 FPS 或 VRAM 峰值。32×32 独立小数帧夹具该轮 927,090us，早期 `png-fixtures/` 成本也保留，不以快轮次覆盖旧值。
- `png-native-ui.log` 两项专项通过，包含两语言实际原生窗口/GPU 截图。第一次截图 `png-ui-first/` 目视检查后发现通用 Exporting 翻译仍含“视频”；保留该轮图像，随后改为通用文案并加入真实排队取消/状态栏断言。
- `png-desktop-host-complete.log` 全量桌面为 532 通过、1 失败、1 原有忽略；失败是新截图测试在既有测试创建原生事件循环后再次创建，触发 winit RecreationAttempt。改为子进程隔离，不改产品窗口逻辑，不忽略测试或弱化截图/交互断言。该失败导致同一 cargo 命令未运行宿主 suite；最终重跑结果另记于后。

旧 host::ops::export::capture 的直接使用方经检索为 Android JNI；本轮只替换桌面入口，不宣称旧 SDK / 冻结组合读帧协议已经获得运动模糊。#255 的下一阶段必须先明确这些接口的支持/拒绝边界，再完成 host 能力及真正支持的 UI 开关、主/浮窗/焦点和保存撤销验收。

最终重跑 `png-desktop-host-final.log` 为桌面 533 通过、0 失败、1 项原有音频设备忽略（350.31 秒），宿主 49 通过（61.04 秒）。截图用例已在子进程实际运行，并通过生产 start/poll/CancelExport 路由验证取消回复、未发布文件以及中英文“导出已取消／Export cancelled”；没有用静态文案替代工作流。实际截图保存在 `png-ui-final/`，每语言包含文件菜单及初始导出状态、保存完成、取消完成三张原生 1280×800 图；另有正式输出的 64×64 PNG。已目视查看中文菜单和英文取消图，原有面板窄列截断属于保留的基线问题，不据此宣称面板样式已全面验收。

此后仅收紧公开 FrameFreezePlan 的非法帧入口，与 Seek 的半开范围一致（负数、NaN、Infinity、等于合成末尾都拒绝）。`png-media-final.log` 在最终守卫上重跑全部 28 项（22.49 秒），新增非法帧断言未削弱原像素或文件保护断言；`png-noffmpeg-final.log` 五项全部通过（10.68 秒），与 FFmpeg suite 重叠，不重复计数。最终媒体产物保留在 `png-media-final/`：相邻视频 PNG 710,962us、嵌套视频 PNG 817,054us、小数帧图形 PNG 621,357us；像素 oracle 仍相同，成本范围同前，旧轮次全部保留。

最终带/不带 FFmpeg 桌面编译检查均通过，见 `png-desktop-ffmpeg-final.log`（7.40 秒）和 `png-desktop-noffmpeg.log`（14.67 秒）。保留既有 21 条普通构建警告、19 条测试构建警告。本阶段没有生成新的 release 分发包。去重计 610 项通过、1 项既有忽略，含五个新增测试及两个既有视频用例的 PNG 扩展；不把多次运行相加。

暂存差异空白检查、来源守卫（1,410 tracked paths）和样式消费者检查通过。#255 仍为 OPEN；本提交完成正式桌面 PNG 这一段，不代表能力/UI、旧协议边界和完整性能验收已完成。

### 时间轴采样开关、能力声明与单时刻协议边界（2026-10-19）

按用户反馈，最终入口为时间轴图层行：在 3D 列旁增加帧混合、运动模糊、连续栅格化三列，默认显示。属性栏中的试验入口已撤回；`controls-first/third/final/complete` 图像和对应测试日志仅保留过程证据，不是最终 UI 验收。列显隐复用“视图”菜单，保存到 workspace，既不写工程历史也不改变采样值。表头短标签配完整名称提示；现有“＋”仍是音频标记入口。没有新增右键菜单、主题 token、依赖或第三方资产。

列布局复用原 ColumnRects 和 `icon.slot`/`value.min_width`，不缩小开关点击范围。窄窗先回收可选混合/父级/遮罩文字列，再按完整槽位显示能容纳的采样列；用户的列偏好保持不变。全部关闭采样列后，位置与原布局一致。控件使用原生 Checkbox，关闭的参数保持模型默认值；不适用来源、锁定和观察状态禁止编辑。命令带 revision，非主合成显式包装 InComposition，走同一引擎错误回复及撤销历史。

| 记录 | 分类 | 证据及限制 |
| --- | --- | --- |
| SAM-FUNC-16 | 功能/UI | 七项专项覆盖实际视频/矢量开关命令、保存重开、undo/redo、两语言、原生主/浮窗输入、视图菜单开关及退出菜单。UI 视频夹具仅是索引和不参与解码的占位文件，截图没有合成预览；实际视频像素证据沿用前述真实 VFR/YUV/PNG 用例，不能用控件截图代替。 |
| SAM-FUNC-17 | 能力声明 | host sources 与模型十种 Content 的 validate_content 逐项一致。帧混合仅视频，连续栅格化仅矢量，运动模糊覆盖六种可绘制来源。desktop_movie 的 exposure_renderer 是编译链路声明；实际 h264/aac 能力在 export_start 检查，不把 FFmpeg feature 等同于编码器可用。 |
| SAM-BOUND-03 | 显式协议边界 | render_plan_info、sample_render_plan、sample_planes、sample_geometry、frame_bundle、legacy_capture 不支持多时刻曝光，在写时间、观察状态、调用者缓冲或文件前拒绝。真实 session 测试验证拒绝不改变状态、a5/5a 缓冲、输出目录和 picking。仅不可达子合成有快门不阻断主工程；可达子合成阻断。单独连续栅格化仍能生成旧计划，未扩大拒绝范围。 |
| SAM-OBS-17 | 新观测/既有错误信道 | malformed sampling 使用现有 observe 生命周期计数，站点/上下文有界；真实 UI 绘制重复 120 次，默认文件首条 Warn、第 60 次一次 Error，健康值不重置该生命周期计数。真实 EditChecked 非法样本数经 engine 回复，保留原因和 revision；Error 文件及 stderr、stdout 隔离均有子进程证据。此处不新增重试、线程或逐帧 Info。 |
| SAM-NORMAL-13 | 正常交互/边界 | 未定义 sampling 是旧模型正常默认，不报错；不支持、锁定、观察、仅释放、失焦后的按住输入不修改项目。列显隐只改 workspace。开关是即时命令，没有持续拖动事务；项目撤销由既有命令历史处理。 |
| SAM-PENDING-02 | 待验收 | 新 main 抗锯齿与采样合并、组合后的 GPU 像素、长时间播放性能、连续栅格化相机平面错误的默认端到端信道、扩展 DPI、正式可运行包。不得以专项或完整桌面测试绿灯直接关闭 #255。 |

`sampling-controls-complete.log` 是属性入口阶段的完整验证：桌面 539 通过、1 项既有忽略，宿主/媒体及集成合计去重 651 通过。改用时间轴后，`sampling-timeline-tests-first.log` 七项全部通过（27.88 秒）。图像 `timeline-switches-first/` 每语言四张：主窗开关、视图菜单、隐藏模糊列、原生浮窗开关；已目视检查中文主窗、菜单、浮窗和英文浮窗。完整新回归另记结果，不把旧属性截图算最终验收。

早期用例的失败分别为存储夹具缺根目录/素材、子合成 ID 不符合格式、测试字体未使用产品中文回退；均修正夹具并保留失败日志，没有改生产校验或削弱像素断言。既有 timeline_3d_tests 仅改为用生产 with_sampling 布局计算点击坐标，其 3D 状态、滚动、浮窗和撤销断言保持不变。

DESIGN §0.7 逐项处置：

| 基线位置 | 状态与理由 |
| --- | --- |
| path_controls 六处纵向步长 | retained：本轮没有修改路径面板 |
| path_controls 双列分配 | retained：不属于图层采样入口 |
| path_controls 混合按钮高度 | retained：不改变路径交互 |
| path_controls 数字字段自绘 | retained：本轮不编辑数值字段 |
| path_controls 全宽布尔按钮 | retained：原缺陷不归本次；新增开关使用 Checkbox |
| effect_controls 行高/步长 | retained：本轮未编辑效果行 |
| effect_controls 布尔文字按钮 | retained：不复用该模式 |
| effect_controls 枚举按钮 | retained：本轮不增加或循环枚举 |
| effect_controls 手工 hover alpha | retained：本轮未编辑效果 hover |
| effect_controls 层级 | retained：属性树布局未改 |
| timeline 既有列坐标 | retained with reason：未整体重构历史坐标，新增独立经测试的采样槽；六种宽度×八种显隐组合验证无相交、完整 hit slot、隐藏后原坐标一致，原生 3D 回归使用生产列布局 |
| timeline 吸附指示颜色 | retained：没有改色、拖动容差或吸附行为 |

DESIGN §12 的开关状态、菜单优先级、中文/英文、主/浮窗、最小/默认窗口、保存/撤销有直接证据；未宣称所有面板、OS Reduced Motion、全部 DPI 或整套八乘七控件矩阵通过。功能模型格式 21 来自 #255 已实现的功能契约，不是因视觉调整引入的新模型。

### 并行分支检查与集成边界（2026-10-19）

接续前已检查 worktree、相关近期分支、未提交变更及开放 PR；没有把只读审查写成对全部历史分支的穷尽证明。

- `node-gestures` 有 node_editor 与 shell 输入/呈现的未提交修改；本分支不复制或重置它们。
- `ui-atlas-lifetime` 有 paint/text/ui 和 atlas 测试修改；本轮沿用生产字体装载，不重复修复图集寿命。
- `codex/node-library-layers` 为节点库简化，涉及 main/shell/style；未发现另一个图层采样开关实现。后续集成保留其功能。
- PR #340（`fix/render-compositor-plane-check`）负责 compositor 缺失平面的受检错误与 #133；本轮不重复实现。
- 最新 origin/main 为 `452f659`，包含已合并的节点工作及平面抗锯齿/放大效果密度。只读 `git merge-tree --write-tree --name-only HEAD origin/main` 发现八个渲染文件冲突：compositing_gpu.rs、compositing_plan.rs、compositor.rs、lib.rs、renderer.rs 和三份 plane shader。该命令未修改工作树；后续必须结合双方语义解决，不能选整份 ours/theirs。合并前先保存并验证本阶段，合并后重验 GPU 覆盖、源密度和曝光。

完整时间轴回归 `sampling-timeline-complete.log` 首轮为 538 通过、2 失败、1 项原有忽略（452.32 秒）。失败均来自旧测试专用 `layer_switches::columns` 仍用未分配采样列的布局计算 3D 点击目标；实际生产 3D 滚动/切窗测试和七项采样用例通过。将该测试辅助函数改为生产 with_sampling 路径（这些真实 host 夹具使用默认可见采样列）；没有改变原保存重开、undo/redo、readonly、菜单互斥或列不相交断言。修正后的整个 property_followup_tests suite 单独重跑，结果另记。

最终控件截图在 `timeline-switches-final/`，另有从真实主窗截取的 `zh-timeline-row-detail.png` 与 `zh-columns-menu-detail.png`，只裁切不重绘。已目视检查两张细节图及完整主窗菜单。来源守卫（1,412 tracked paths）、样式消费者和暂存差异空白检查通过。截图裁切首次用 Python PIL 失败（环境未安装）；改用系统 System.Drawing 完成，没有安装依赖或修改产品。

修正辅助坐标后，`sampling-timeline-existing-targets.log` 的整个 property_followup_tests 为 10 通过、0 失败（18.44 秒），包含前述两项失败。最终桌面用例去重 540 通过、1 项原有忽略；完整轮次有两项失败、后续定向重跑修复，不把它描述为一次完整全绿运行。修正只涉及测试辅助坐标，生产语义与完整轮次一致（其后列布局仅做排版格式化）。
