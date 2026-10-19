# 图层渲染采样：契约与实施证据

Issue：#255。本工作基于 `f32c64de`，保留在 `feat/layer-render-sampling`。本文是进行中的交付记录，不能据此关闭 issue，也不表示三个开关都已支持。

## 当前要求与状态

| 要求 | 当前证据 | 状态 |
| --- | --- | --- |
| 模型存储、旧工程迁移、撤销 | LayerSampling、SetLayerSampling、格式 21；真实核心命令、保存/重开、undo/redo、手势取消测试 | 已实现并测试 |
| 连续栅格化实际像素 | 原创圆形在 256×256 输出中放大八倍；真实 GPU capture、透明边界、覆盖面积、反复切换和冻结模型重开 | 矢量已实现；其他来源仍需逐项能力界定 |
| 连续栅格化预算和预览 | 投影密度影响实际源纹理尺寸及 mesh/cache 身份；默认与预览计划一致；低预算拒绝且恢复预算可继续渲染 | 已实现并测试 |
| 帧混合 | 模型有 Hold/Linear、按实际 PTS 加权的时钟函数 | 解码双帧调度、线性预乘 GPU 混合、冻结输出、图像与成本仍待接入 |
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

帧混合时钟依据导入的严格递增 PTS，而非 nominal fps；源有效区间外透明，尾部不循环。接入解码器时还需逐项核对首尾帧、裁剪、非零 PTS 和 VFR 与实际帧持续时间，不能仅用单元函数证明完整的视频混合。

## 可观测性审计

| ID | 分类 | 证据与剩余边界 |
| --- | --- | --- |
| SAM-OBS-01 | 既有覆盖 | 新命令复用核心原子 apply、历史和原有串行引擎信道；未新增后台线程、超时、重试、重启、panic hook 或 stdout 日志。非法参数、锁定目标返回原始 Err。 |
| SAM-OBS-02 | 功能边界/既有覆盖 | 投影与预算失败返回 Err，保留 layer ID 或已有 ResourceRejection；实际 GPU 测试验证低预算不会无声降级，恢复预算后继续成功。尚未对新增相机平面错误单独验证默认文件日志/Error stderr/信道链路，不能将函数的 Err 测试等同于可观测性验收。 |
| SAM-NORMAL-01 | 正常路径 | 默认采样、缓存命中、透明边界、完全背向相机裁切不记故障；健康渲染无新增文件 IO、计数器或警告。 |
| SAM-PENDING-01 | 待完成 | 帧混合和运动模糊接入时需要审计 pending、取消、被取代请求、解码/采样失败及冻结读者释放；不能靠加重试隐藏问题。 |

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

未新增依赖、第三方资产、UI 栈或主题 token；未读取 Adobe 二进制，也未把参考产品截图放入产品。本 issue 的最终 UI 仍需 DESIGN §0.7 逐项处置及 §12 验收。
