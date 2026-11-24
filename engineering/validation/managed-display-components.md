# 受管显示的数值分量检查

本批继续 #83，将现有 RGB/Alpha 通道检查接入受管窗口、保留重绘和显示截图。
彩色画面使用原生 OCIO 显示结果；数值通道读取原工作图像，选中分量限制到
0–1 后显示为不透明灰度。RGB 是实际存储的预乘工作分量，Alpha 是铺黑底前的
覆盖率；不解预乘、不执行 OCIO、不把显示程序生成的不透明 Alpha 当作覆盖率。

原工作纹理通过既有分配租约保留，各单元的数值绑定复用该单元已有 uniform，
没有额外纹理、表、可写 uniform 分配或逐帧文件 IO。每次调用显式选择通道，包括恢复
Color；不同单元和截图继续使用独立可写 uniform。桌面单视图显式转送 channel，
并跳过旧 scratch presenter 的数值源切换，避免读到另一个合成路径的旧图像。

| 编号 | 类别及处置 | 证据与限制 |
| --- | --- | --- |
| MDC-01 | 新增消费者 | 类型化 `ManagedDisplayPresenter::encode_inspected_region_view_load` 同时服务整组呈现、保留重绘及显示截图；工作与显示身份不混用，租约沿用原资源账本 |
| MDC-02 | 新增 GPU 资格 | 四组原生显示程序，RGBA/BGRA 的 UNORM/sRGB 四种附件，单/双/四单元轮流检查 R/G/B/A，再恢复 Color；每个后端 6720 个像素，不改变原 SDR8 一个码值容差，工作纹理逐字节不变，中间不读回处理图像 |
| MDC-03 | 实际错误 GPU 负对照 | 将显示码值冒充工作数值的错误生产者必须失败；同一编码器中让四单元共用一个可写 uniform 的错误绘制也必须失败。每类每后端各十六例，随后恢复正确独立绘制 |
| MDC-04 | 实际窗口与截图 | 三配置、两输入 Alpha、六组输入，单/双/四单元，平移缩放及局部区域，共 5898240 个实际 surface/截图像素；独立像素中心几何与原 CPU 数值参考比较。配置准备后删除，检查操作不增加采样，不改变工程、历史或时钟 |
| MDC-05 | 桌面协议资格 | 实际 worker 经 Present 和 CaptureUi 验证单/双/四单元的独立通道与恢复彩色；保留十五张实际 PNG，逐格中心十六像素与原生工作/显示 CPU 数值比较，确认不触发重新采样或模型变更 |
| MDC-06 | 既有错误观察 | 新入口复用原资源、设备、附件与区域检查；错误沿原 worker 信道传递。真实失效来源、陈旧组、非法区域及未接入比较继续核验默认 Info 文件、Error stderr 与 stdout 隔离。没有新增日志、重试、线程或超时策略；健康 Present 保持安静 |
| MDC-07 | 正常分支 | 通道切换、颜色恢复和 surface 等待不是故障。截图仍要求当前完整显示组，不以旧帧或重新渲染掩盖错误。工作图像保留是该显示消费者的资源所有权，不是新的缓存兼容回退 |
| MDC-08 | 功能边界与断言迁移 | Alpha 的原明确拒绝由本批真实像素功能替代。旧拒绝夹具改为仍未接入的比较请求，保留其无发布、无文件、工程/历史不变及全部日志信道断言；诊断文本相应指向比较。不是修改容差或删除失败路径来通过验收 |
| MDC-09 | 共用呈现器回归 | 旧非受管 presenter 的公开行为保持，继续验证 ROI、比较、通道、平移缩放和桌面窗口/冻结/截图路由；受管 Color 的普通 encode 使用既有不可变颜色缓冲，不继承上次分量状态 |
| MDC-10 | CI | 新核心 GPU 像素门禁在各桌面平台原生准备之后、打包之前运行，禁止条件跳过与 continue-on-error；Windows 宿主 authored 门禁含实际窗口资格，已有真实桌面 worker 门禁包含新增通道请求 |
| MDC-11 | 样式与来源 | 没有面板、布局、样式令牌、依赖或第三方资产改动，DESIGN §0.7 面板整改项保持。原四份固定参考与原生运行时身份逐字节核验；新数值夹具、几何参考和截图独立生成 |
| MDC-12 | 未完成 | 比较快照、取色、录制及其他工程输出，附着精度/显示事务、组合成切换、其他场景消费者和系统显示资格仍需后续完成；本批不关闭 #83。按用户要求，仅完成该 issue 后停止，不继续其他 issue |

原始输出和源码快照位于 `artifacts/issue-goal-current/color-managed-components/`。
首轮宿主命令误用模块筛选名，执行零项，记录在 `components-window-first.txt`，
不计作通过。修正为真实 `authored::window::components::` 后执行实际资格；
新增窗口测试独占子进程并沿用已有 120 秒看门狗，避免 winit 的同进程事件循环
限制。测试进程限制不改变生产线程或恢复行为。没有修改既有参考或容差。

窗口 PNG 和其子进程输出位于 `default-log-channels/managed-components-window/`；
真实桌面 worker 的十五张通道 PNG、原有失败日志及恢复证据位于
`default-log-channels/managed-window-worker/`。Vulkan/DX12 的本机通过不证明
Metal、OS ICC、HDR 或显示器校准；远端新提交仍必须通过自己的矩阵。

最终二十一项原生 authored、二十项核心图片处理、八项旧呈现器、十四项默认
信道及一项旧桌面路由，合计六十四项不同 Rust 测试通过；新增核心分量另在
DX12 通过，不重复累计。工具检查二十八项，来源门禁覆盖 6441 个路径，工具
清单七十项。`components-authored-final.txt`、`components-gpu-final.txt`、
`legacy-presentation-final.txt`、`default-channels-final.txt`、
`legacy-desktop-final.txt` 与 `components-core-dx12-final.txt` 保存最终输出。
