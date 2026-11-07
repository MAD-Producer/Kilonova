# 多视图共用后台管线准备

关联 #64、草稿 #627。本轮把双／四视图接入 Session 原有的后台准备和缓存收尾，
不改变面板布局、视觉样式、渲染像素语义或生产预算。

## 问题和实现

普通多视图此前绕过后台准备，直接编码各相机。旧普通程序红对照中，画面已呈现
两次，但 `pipelinePreparation.preparing=true`、`task=null`、缓存 `load=null`；
沿用的 30 秒完成期限实际失败。这不能称为完全没有画面，而是准备状态和缓存
接线缺失，首次效果也仍可能在 owner 上创建原生管线。

现在普通组先复用已有源解析，再把 root scene、1／2／4 个拥有所有权的相机选择
和各自目标尺寸交给同一个任务。worker 在每个近期采样时钟上选择相机并编译，
同组只产生一份待安装复用库，owner 一次安装。不能逐格安装独立复用库，否则
后安装的格子会替换前一格的变体。

原有三个时间样本、128 条管线、8 MiB WGSL、16 MiB alpha 快照、256 个场景节点、
32 层深度及单 worker／队列／待入队／完成槽上限保持不变。每次只保留一个当前
相机 fork，没有新增每格 worker，也没有把输入快照放大为十二个场景。

准备键加入 native owner、相机、各格尺寸与曝光格索引；其余 generation、revision、
surface、preview 和曝光上下文沿用既有校验。曝光组按已有顺序准备其真实快门
采样，不替换成中心帧计划。完成整组后才做缓存 checkpoint；宿主准备错误保留
原类型和 cause，通过既有 worker 信道返回，未改成成功、重试或重启。

## 验收范围

最终命令及源码摘要见证据包中的 `final-summary.json`。每次命令都保留 stdout、
stderr 和执行前后摘要；普通 exe 另有构建来源和文件身份收据。

最终 2,486 份源码摘要在下列命令前后及当前工作树一致，46 项测试通过、0 忽略：

| 最终轮次 | 结果 | 总耗时 |
|---|---|---|
| `multiview-pipeline-prepare-v3` | 宿主 18、渲染 8 项通过 | 108 秒 |
| `multiview-native-host-v2` | 宿主多视图 4 项通过 | 48 秒 |
| `multiview-desktop-viewer_frame-v2` | Present／Capture 2 项通过 | 5.812 秒 |
| `multiview-desktop-multiview_tests-v2` | worker／默认信道 1 项通过 | 2.375 秒 |
| `multiview-desktop-frame_completion-v2` | 帧继续队列及空闲领取 6 项通过 | 51.016 秒 |
| `multiview-desktop-preparation_progress-v2` | owner 真故障 2 项通过 | 2.750 秒 |
| `multiview-dx12-surface-v1`／`group-v1` | DX12 单视图 1、多视图 4 项通过 | 10.328／44.468 秒 |
| `multiview-normal-build-v2` | 普通 FFmpeg exe 构建通过 | 2.453 秒，增量构建 |
| `multiview-normal-app-v3` | 4 个普通程序独立进程通过 | 13.672 秒 |

该普通 exe SHA-256：
`bb7ab64f3fc9b0fa81c5c02a1a2432bcb2e83233a5e734102ecb840fda1d63f5`。
双视图冷／热首次内容完成 4,875／2,484 毫秒，四视图 2,515／2,469 毫秒；
这里只报告本机各一次测量，冷指应用私有缓存缺失，不能隔离所有驱动和系统因素。

- 渲染层对四种不同格尺寸逐一比较完整 RGBA；独立冷 renderer 是负对照，确实
  发生前台创建，而预备完成的 renderer 各格都不增加前台原生创建。
- 0／3／5 格和零尺寸被拒绝，原票据取消，不安装复用库。
- 真实宿主覆盖四种相机、独立单视图像素 oracle、普通／四样本快门、owner 切换
  和后续图层的新 Gaussian 变体；保持前台创建数不变。既有视频等待和整组呈现
  断言原样运行。
- 桌面原 Present／Capture、多窗口与逐格检查、帧继续队列、空闲准备领取及 owner
  真故障回归保留原断言，验证默认 Info 启动、Error 文件／stderr 和 stdout 隔离。
- 普通 FFmpeg exe 用四个独立进程分别启动双／四视图的冷／热应用私有缓存。
  核对真实内容帧、installed ticket、工作区布局、完整捕获像素、工程字节不变、
  原生缓存缺失／实际恢复、保存和正常退出。未清空 OS 或全局驱动缓存，也不把
  这些 debug 构建计时当作通用启动提速保证。
- 显式 `WGPU_BACKEND=dx12` 的原生 surface 报告实际 `Dx12` 适配器，并通过原有
  单视图准备和全部四项多视图用例。这只证明其渲染／准备路径；该 surface 夹具
  没有配置应用缓存目录，不能冒称普通程序 DX12 持久化端到端验收。

## 首次结果及夹具边界

原始失败全部保留，没有修改既有断言、期限、预算或 ignore：

1. 第一轮编译在生产模块误用 dev dependency 的 `motion_core::Observer`，改为
   已有生产依赖的 `motion_model::Observer`。保留诊断与源码摘要；不声称归档了
   此次全部源码字节。
2. 原桌面 TestStudy 夹具假设首个调用立即完成。普通批 API 允许返回 pending，
   新接线实际进入该路径。夹具在原保留呈现断言前显式等待，仍用原 30 秒期限；
   每次等待都检查 overlay 和完整工程、修订、时钟、撤销状态，期间另发真实 state
   命令。原完成、错误信道和内容重用断言保持不变。
3. 第一轮普通探针冷进程通过，热进程在 cache restore 后、真正内容完成前取到
   `task=null`。初始 revision=0 与 chrome 绘制不能证明内容就绪。保存原探针和
   原失败；最终探针另外要求 `lastPresentedFrame == frame`，仍保留 installed、
   缓存、像素和退出断言，没有延长期限。
4. 最初普通 exe 红对照用的是此前封存的旧二进制。其身份取自对应 runtime
   收据，外层命令对当前源码的摘要不冒充旧 exe 的编译来源。

## 错误处理处置

| ID | 分类 | 处置与边界 |
|---|---|---|
| 64-M1 | 功能修正 | 普通多视图和真实快门接入原有有界准备；完整像素与前台创建对照 |
| 64-M2 | 功能修正 | 整组完成后 checkpoint；普通冷／热独立进程实际恢复 |
| 64-M3 | 既有观察 | 原始宿主错误和渲染错误分别保留；默认日志信道回归，不新增热路径 start 日志 |
| 64-M4 | 正常分支 | pending、取消、替换和无新缓存创建沿用原协议，不告警、不重启 |
| 64-M5 | 验收夹具 | 加强真实就绪判定和待准备阶段不变性，不放宽既有断言 |
| 64-M6 | 仍有边界 | 本机 Vulkan／DX12 不能代替 Metal 或所有格式、包和驱动组合；普通程序 native GPU 创建故障的端到端注入仍未由本轮补齐 |

开始前已核对五个开放 PR 与本地工作树。另一个任务的 viewer.rs 未提交 hunk 在
原第 175 行附近，本轮只改导入、poll 和 render_views 接线，保留其工作树和改动。
不新增依赖或第三方资产。#64 与草稿 #627 暂保持打开；最终提交 CI 单独记录。

证据包：[multiview-pipeline-preparation-evidence.zip](multiview-pipeline-preparation-evidence.zip)，
412 个成员，18,522,684 字节，SHA-256：
`8be5df5ccfcd9339e388edeba739da44a2f94038a7a32af470acdbe60248553d`。
逐成员字节、SHA-256 清单和 zip CRC 已核对；保留所有首次失败、源码摘要、可得的
阶段源码、最终源码、控制器、默认日志、截图与普通程序身份收据，不含 exe、DLL、
插件二进制、驱动缓存数据或外部资产。
