# 原生显示码值的 GPU 输出传输

本批属于 #83，接在普通 Session 安装之后。它提供受类型约束的显示传输，
尚未把普通窗口、多视图和导出接入管理颜色；#83 保持开放。

`ManagedDisplayTarget` 只接受已核验的 WorkingToDisplay 程序，私有 Float32
目标保留空间、范围和 Alpha 契约，不能从旧 sRGB 目标重新标记得到。
`ManagedDisplayPresenter` 的内部 Presenter 不对外暴露旧的重绑和通道 API。
程序、目标及呈现管线在准备阶段创建，编码阶段不编译程序、不访问文件或回读像素。

OCIO 输出已经是显示码值。普通 UNORM 附件直接写入；sRGB 附件先补偿其强制
写入传递函数，避免再次编码。Float32 来源通过四点 textureLoad 双线性采样，
不依赖可选的 FLOAT32_FILTERABLE，也不在缩放时退化成最近邻。
现有线性 Presenter 的模式、采样和编码分支保留。

接口只接收 RGBA/BGRA 的 UNORM/sRGB 四种 SDR8 格式及完整归一化码值范围。
编码前核验 renderer/encoder 所有者、实际输出纹理格式、维度、用途和有限视口。
背景参数沿用旧 Presenter 的 sRGB 画布背景语义，不能当作工作空间颜色。
来源和程序资源沿既有资源账本保留，两个描述 uniform 沿用分配预算与上传生命周期。

这不是 OS 显示资格：调用宿主仍须证明所选显示与实际 surface 一致，并处理
有效帧提交、Alpha 合成和资源替换。原生监视器色域、ICC、HDR、浮点窗口、
limited-range 输出及外部目标纹理的 device 身份均不由这层建立。

## 验证与错误审计

原始证据目录：`artifacts/issue-goal-current/color-display-presentation/`。

| ID | 类型 | 证据与边界 |
| --- | --- | --- |
| MDT-01 | 新功能 | 固定原生运行时现场生成的 8 个显示程序，经四种真实 GPU 附件比较 384 个原 CPU 参考像素；canvas 与整面路径逐字节一致 |
| MDT-02 | 采样资格 | 原 CPU 参考插值后比较 1536 个放大像素，四种格式的 canvas 与整面路径一致；SDR8 最终量化允许 1 字节，原浮点参考与容差不变 |
| MDT-03 | 实际负对照 | 8 条实际漏显示变换/重复编码 GPU 路径必须违反同一 SDR8 像素对照 |
| MDT-04 | 拒绝与恢复 | 错误格式、NaN/越界视口、非法图像、非有限背景、外来 renderer/encoder、错误阶段及 limited-range/浮点附件返回 Err；拒绝提交后完整输出字节不变，显式恢复与原输出一致 |
| MDT-05 | 预算 | 仅余 63 字节时第二 uniform 拒绝且总量和存活数不变；64 字节完整准入，销毁后返回原量；未改变预算或重试策略 |
| MDT-06 | 观察边界 | 新层只返回原始 RenderError，未新增日志或吞错；尚未接入普通 surface 命令，因此本批不宣称新的默认信道闭环，Session 既有信道资格见 managed-color-session.md |
| MDT-07 | 回归 | 旧呈现实际 GPU 测试 8 项通过，包含通道、区域、比较、快照、缩放和平移；描述符、未提交写入、实际帧回收与重复来源绑定 4 项通过 |
| MDT-08 | 正常分支 | 健康编码无新增日志、观察计数、文件 IO 或像素回读；测试专用 readback 不进入产品路径 |
| MDT-09 | 待完成 | 普通 surface、有效帧事务、多视图、OS/监视器与 HDR 资格、导出及其他受管消费者仍需独立接入和验证 |

首轮新 readback 夹具忘记解包当前 wgpu 的映射 Result，编译失败保留在
`display-first.txt`；只修正新夹具，后续实际 GPU 和容量对照见
`display-second.txt`、`display-controls.txt`、`display-scaled.txt` 与 `display-final.txt`。
没有改写旧像素参考、旧断言或已有生产时间限制。

远端旧头 `49bf45f6` 的 run 42466294523 三平台构建、桌面测试及 Rust/GPU
均已成功。依赖审计首次固定资产下载 HTTP 504，保留原失败后仅重跑该 job，
128708944555 成功；整轮已为 success。该远端证据不覆盖本批新增传输代码，
新测试已加入同一原生构建资格步骤，推送后仍须验证。

工具门禁发现 144 项，143 项通过、一项原有可选跳过；工具库存仍为 70 项，
来源门禁通过 6397 个已跟踪路径。
