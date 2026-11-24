# 受管显示快照、比较与保存

本批继续 #83，将现有查看器快照、分屏比较和保存功能接入类型化的受管显示。
一次显式捕获同时复制工作图像与已经执行原生 OCIO 的显示图像，不重新采样、
不重新准备配置、不读取素材或回读 CPU 像素。实时前台随后复用或改写自己的纹理，
也不会改变快照。保存的是显示快照，元数据明确返回 `kind: display`；它不是
工程的 WorkingToOutput 导出，不宣称具有 OS ICC/HDR 或任意 PNG 颜色配置资格。

冻结图像及其原生程序通过资源账本保留。每份快照预备四个独立可写单元，
在 Present 中直接选择彩色或原始预乘工作分量，不创建绑定、管线或像素目标。
显示截图和 PNG 保存可在显式冷命令中为目标格式准备独立绑定，像素只共享冻结副本。
完整设备、工作/显示契约、程序和附件格式不匹配时，在写入绘制状态前拒绝。

| 编号 | 类别及处置 | 证据与限制 |
| --- | --- | --- |
| MDS-01 | 新增功能消费者 | `ManagedDisplaySnapshot` 同时持有两个冻结端点；桌面快照状态区分 Legacy/Managed，受管数据不会冒充旧 SDR 工作目标。只有复制与提交成功后才替换状态和递增 generation |
| MDS-02 | 新增核心 GPU 资格 | 四组原生显示程序、RGBA/BGRA 的 UNORM/sRGB 四种附件；原 Working 与显示纹理真实改写后，四单元同时比较五种通道、三种 split 及 ROI。每后端 23040 个实际像素，与独立 CPU 数值和像素中心几何对照，原一个码值容差不变 |
| MDS-03 | 错误 GPU 负对照 | 同一编码器让四单元共用冻结 uniform 的错误实现必须与参考不同，每后端十六例；后续正确绘制恢复。不是仅比较元数据或两个相同 GPU 路径 |
| MDS-04 | 资源失败与身份拒绝 | 每后端三十二次真实预算拒绝，分别发生于首个纹理分配前和只够一个 Float32 工作纹理时；候选释放后 admitted bytes 恢复，旧 Alpha 快照仍吻合。另验异设备、不同显示程序、附件格式、非法单元和 split 拒绝 |
| MDS-05 | 实际宿主窗口 | 六组原输入、单/双/四单元、平移缩放、ROI 和通道轮换，整 surface 与显示截图共 5898240 个像素；一个快照可用于所有单元，不增加采样，不改工程/历史/时钟。配置准备后删除，捕获不回读；错误 owner、cell、split 和陈旧组均拒绝 |
| MDS-06 | 实际桌面 worker | 单/双/四单元分别捕获，再真实编辑不透明度，经 Present/CaptureUi 比较全部 64×64 像素；十五张比较 PNG、十二张彩色/数值冻结 PNG，并通过 undo 恢复原工程。保存像素使用原工作和原生显示参考，不使用实时前台作为真值 |
| MDS-07 | 默认错误信道 | 预算不足的捕获和保存保留原生 ResourceRejection；IO 保存失败、陈旧组和已关闭浮动窗口保留原原因。恢复预算后保存 generation 与冻结像素不变，失败不创建 PNG。沿已有 worker 的 Info start/in-flight、Error 文件/stderr、stdout 隔离；不新增生产日志站点或修改重试/超时策略 |
| MDS-08 | 正常分支与所有者 | surface 等待、保留重绘、通道/ROI/split 切换是正常请求。主窗口及浮动窗口快照独立；关闭 owner 后不能保存其快照。健康 Present 保持安静，快照与比较不修改模型或编辑历史 |
| MDS-09 | 断言的功能迁移 | 原“受管比较未接入”的单视图拒绝，迁移为尚未捕获快照时的明确拒绝；保留无输出、完整状态、默认信道及上下文断言，并新增实际比较像素。旧非受管 comparison 目标仍被受管消费者拒绝，不以隐式颜色重解释接入 |
| MDS-10 | 共用路径与 CI | 旧呈现器、四单元 inspection 和桌面窗口/冻结/截图路由继续回归。新增快照 GPU 命令在各桌面原生资格之后、打包之前强制运行；现有 authored 和 Windows worker 门禁包含新增实际路径，工具测试验证顺序与不可跳过 |
| MDS-11 | 来源与样式 | 无依赖、第三方资产、面板、布局或样式令牌改动，DESIGN §0.7 整改项保持；四份原数值参考与固定运行时身份逐字节核对。新增几何与数值断言独立编写，不更改原 CPU golden 和容差 |
| MDS-12 | 待完成 | #83 仍有取色/录制、其他工程输出、附着精度/显示事务、组合成切换及其他场景消费者、完整 ACES 2 和跨平台显示资格缺口。本批不能证明这些路径可用，不能关闭 #83；仅继续该 issue，完成后按用户要求停止 |

证据目录：`artifacts/issue-goal-current/color-managed-snapshots/`。
首轮核心测试编译错误与宿主夹具类型错误保留在各 first 输出；它们不计作通过。
桌面首轮失败发生于旧缺失素材测试删除了源图，随后真实编辑需要重新准备资源；
修复夹具边界，在缺失素材的完整拒绝断言之后恢复图片，再执行成功编辑及 undo。
首轮父测试又在复制未生成的 PNG 时覆盖了子进程主失败；本批改为先保留 stdout、
stderr 和日志并检查子进程状态，随后复制必须生成的成功证据。首轮原始父日志保留，
但当时 child 输出未另存为独立历史副本；不能声称该轮具有完整独立子日志归档。

新增非法 split 测试的首版期望文本与真实拒绝阶段不符，保留
`snapshots-gpu-third.txt`，修正为既有明确 comparison split 原因；新增关闭窗口
测试先由原 worker 拒绝，未进入快照查找，保留 `snapshots-worker-third.txt`
及 `worker-third/`，按真实边界严格检查 closed-window 原因。以上修正不改生产
返回值、旧断言、容差或失败处理。第二轮实际窗口通过输出亦保留于 `worker-second/`。

新增 generation 断言首版误取最后捕获单元，但后续普通 frame 已明确选中单元零；
第四轮正确返回单元零的旧快照而触发该夹具错误。保留
`default-channels-fourth.txt` 与 `worker-fourth/`，将严格比较的期望绑定到活动单元零，
不改变生产选择逻辑，也不删除 generation 的不变性检查。

第五轮子进程全部实际像素通过，但新关闭窗口输出名以 `window.png` 结尾，
误进入旧缺失素材 `assets/window.png` 的四条原因计数。保留 `worker-fifth/`
与 `default-channels-fifth.txt`，将新输出改名为 `closed-owner.png`，保持旧四条
来源计数及每条原原因、stderr、stdout 检查完全不变，不改断言数量或放宽筛选。

本机 Vulkan/DX12 通过不能替代 Metal、其他平台真实窗口或 OS 显示链资格；
新提交必须通过自己的 CI，旧提交绿色结果不沿用。既有编译警告及外部 Vulkan
overlay 注册问题未在本批处理。

最终核心图片处理二十一项、原生 authored 二十二项、旧呈现器八项、四单元
inspection 一项、桌面默认信道十四项、旧桌面窗口两项及捕获/保存原生错误一项，
合计六十九项不同 Rust 测试通过。新增核心快照另在 DX12 通过，不重复累计；
工具二十九项、来源门禁 6447 个路径及工具清单七十项通过。最终输出分别为
`snapshots-gpu-final.txt`、`snapshots-authored-final.txt`、
`legacy-presentation-final.txt`、`legacy-multiview-final.txt`、
`default-channels-final.txt`、`legacy-desktop-final.txt`、
`legacy-capture-errors-final.txt` 与 `snapshots-core-dx12-final.txt`。
`default-log-channels/managed-window-worker/` 保留十五张比较图、十二张冻结 PNG、
浮动窗口快照及真实错误文件/子进程信道；宿主另保留两张平移比较截图。
