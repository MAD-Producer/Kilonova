# 受管工程的 SDR MP4 输出端点

本批继续 #83，将已获准的受管二维工程接入实际 H.264/AAC 导出。
不是 HDR 视频支持，也不改变编码器、既有 PNG 或显示变换的像素语义。

| 编号 | 分类与处置 | 实现、证据及边界 |
| --- | --- | --- |
| MMO-01 | 功能 | 冻结工程及配置、原图后，按每帧的动画采样在 Float32 线性工作空间合成。先以工作黑色消除 Alpha，再运行工程 WorkingToOutput，最后 clip/round 到不透明 RGBA8；不借用显示变换 |
| MMO-02 | 输出合同 | 接入现有编码器要求 full-range sRGB RGB、BT.709 原色与 D65。编码器保留既有 IEC61966-2-1 传递、BT.709 矩阵/原色、limited YUV 元数据。线性、配置未声明或其他输出显式拒绝，不悄悄改为 sRGB |
| MMO-03 | 生命周期与预算 | 原生输出程序、原图、工作/输出纹理和转换绑定只在导出准备时建立，逐帧复用。额外程序计入既有 256 程序和 64 MiB 编译载荷上限；GPU 分配、提交和回读使用既有预算与资源账本 |
| MMO-04 | 独立端点 | opaque output 有独立程序键，普通保留 Alpha 的 output 和 opaque display 不变。输入阶段及 straight 工作输入不能调用 opaque output 编译入口 |
| MMO-05 | 新观察 | 新冷操作 prepare_opaque_color_output 复用既有 Observation：Info start 后才加载/编译，成功 Debug done，错误保留阶段、请求、原始 cause、配置/选择项与 shader 指纹。没有新 sink、正文输出或逐帧 Info |
| MMO-06 | 既有错误边界 | 导出 worker 的开始/进行状态、最终失败及取消信道沿用原实现；新路径给准备、采样、合成、提交、health、回读和量化错误补充阶段/帧。取消仍为正常取消，无重启、自动重试或 unmanaged 回退 |
| MMO-07 | 实际编码 | 三组既有 ACES 配置 × straight/premult 输出，六帧动画、72 个原始 CPU/GPU 色卡对照；真实 MP4 解码首尾帧、六帧时间范围及 AAC 样本数。冻结后替换实时配置和图片、关闭 Session，仍使用冻结资源；完整 Session JSON 不变 |
| MMO-08 | 负值与 HDR | 12 组独立原生 CPU/GPU Float32 输出对照含负值、超一值和零 Alpha 的未定义 RGB，验证先在工作空间垫黑。HDR/负值保留到明确的 SDR 量化边界；不将 RGBA8 产物称为 HDR |
| MMO-09 | 默认信道 | 实际缺失 runtime manifest 经新 opaque 准备入口失败：Error 同时进入文件/stderr，Info start 不进入 stderr，stdout 隔离。检查完整原始原因、请求、阶段、计时及准备后状态恢复。负对照移除 stderr、降级 Error、隐藏路径或污染 stdout 必须失败 |
| MMO-10 | 正常分支 | 输出合同拒绝和逐帧取消均保留已有目标文件，不遗留临时 MP4。不把正常取消视为 Error；后续显式新请求可正常准备，不改写前次失败 |
| MMO-11 | 不变与资格 | 未增加依赖/第三方资产、未改工程/效果包格式、未修改旧断言/golden、未改 UI/token。DESIGN §0.7 的 UI 项全部保留，本批无面板修改 |
| MMO-12 | 待完成 | 既有受管 admission 对视频、效果、嵌套、三维和非默认采样的限制继续生效；校准深度和时间曝光在新路径明确拒绝。其他输出格式、完整 ACES 2、OS/ICC/HDR 及其他平台窗口资格不由本批证明。此前原生/编译器异常根因仍未定位，#83 与 PR 继续保持未完成/草稿 |

本地原始证据位于 `artifacts/issue-goal-current/color-managed-mp4/`。
首轮测试构建有四处新测试夹具 API 使用错误，尚未运行测试；已修正为现有
冻结入口及 opacity 动画，不涉及放宽既有断言，失败日志保留。
随后 Vulkan 与 DX12 各通过同一组三项新增导出测试（不能计作六项不同测试）；
原有普通 MP4 十一项回归和新增默认信道一项通过。最终留存输出、PNG 解码帧与
独立参考数值的复验日志单独命名，不拿早期结果代替最终代码。
最终 Vulkan 26.07 秒、DX12 37.75 秒，分别留存六份完整 MP4 与首尾解码 PNG。
原有受管 PNG 三项回归通过（19.95 秒），不带 FFmpeg 的新 opaque 输出 GPU
测试通过（7.43 秒）。曾出现的新测试非 FFmpeg 构建 unused import 警告已通过
feature gate 消除；原有 render 两条与桌面二十六条警告保留。
原有 opaque display 的一项真实 GPU 回归亦通过（7.77 秒），含 12 个错误 RGB
像素的负对照；这没有修改此前显示端点断言或像素容差。

CI 的普通原生资格步骤增加不依赖 FFmpeg 的 opaque CPU/GPU 对照及默认信道
测试；只有现有非 PR 的 FFmpeg 发布步骤才运行完整 MP4 资格，不扩大 PR 的
下载范围。Linux/macOS 实际编码结果仍需该平台自己的 CI 证明。

本批仅使用源码阅读、常规构建/测试和现有 CI；不使用内存检查、反汇编或
特殊故障工具。默认信道测试只调用缺失文件这一正常错误路径。阻塞、GPU
设备丢失及原生崩溃的完整新路径端到端覆盖仍属待验证，不冒充已修复。
