# 原生宿主的整组多视图呈现（#221，第五阶段）

基线 `fbbe6a48` 已同步 main 的 #576、#577；继续前核对开放 PR 和工作树。
#575 预渲染仍由另一分支处理，本轮没有修改其工作树、预渲染调度或缓存协议。
`session.rs`、snapshot 和 preview 仅接入新状态、资源统计及工程替换清理。

## 实际接入

新增 `Session::render_views_with_overlay`，在 Session 的实际 owning thread 上运行，
接受 1／2／4 个显式相机及各自 viewport／canvas image：

- 普通场景一次取样后派生 root 视角，准备一次共享图像／视频来源，分别编码。
  不修改全局 observer、不替换无效相机。相机身份、文档 generation、合成、revision、
  frame、owner、surface epoch／尺寸、预览质量和输出尺寸构成内容身份。
- 曝光使用原有 `SamplingPreview` 按 revision 保留的不可变 `ExposureDocument`，
  各格拥有显式冻结的 `ExposureScenes`／`ExposureFrame`。请求构造共享工程快照；
  曝光子采样的求值仍按格独立，尚待跨格共享。
  接收器、相邻 PTS 混合及 GPU 曝光都继续使用原来的渲染实现。
- 同一视频实例只有一个期望的解码时点，四格共用 VideoFrames，故先完成当前格的曝光，再推进下一格。
  普通 pending 保留任务与整组旧输出，不交替推进不同格的快门时钟。
- 每格 resolve／encode 按现有 GPU uniform 规则提交，全部成功才发布完整 front 组。
  之后只获取一次 native surface，编码全部格子及 overlay，再提交并 present 一次。
  Surface 暂不可用沿用既有 acquisition 观察和协议，不在新组件中增加重试。
- 平移／缩放 placement 的变化只重新呈现；内容 key 不变时不重做取样或曝光。
  内容尝试纳入既有 `renderAttempts`，复用不增加；该饱和计数沿用 Session 存活期
  累计、创建新 Session 归零的范围，不是连续失败计数。
  图像和曝光 poll、GPU 编码或任务构造的错误保留 typed cause，并在该请求内终止，
  后续同 key 调用返回原 cause。显式更换 key 可创建新请求；参数／模型校验仍逐次拒绝。
  没有 catch 生产 panic、清零诊断、改写错误为成功或自动重启。
- 等待时保留旧组仅限同一文档、合成、原生 owner、surface epoch／尺寸和格数。
  其他情况只画 pending chrome；不显示另一个窗口／文档的已完成组。
- 新旧单视图输出共用 preview target 额度，保留的单视图 scratch 作为额外容量入账。
  整组预算加法饱和，巨大额外容量不能溢出后误获准。没有临时放宽 renderer policy。
  snapshot、显式 ledger 和资源汇总都计入多视图；曝光目标沿原类别计数，避免重复相加。
  工程替换清除旧组／请求；detach 随 Graphics 释放所有权，在途 GPU lease 仍按原规则退休。

## 验证边界

新增原生宿主用例运行真实窗口 surface 和 GPU，窗口隐藏；它们不模拟产品 UI 已完成：

1. 不同相机实际像素、1→2→1→4 格切换、canvas 重呈现保持输出 texture 身份，
   overlay／presented 各增加一次。非法格数、重叠和缺失相机在绘制前失败；工程、
   revision 和 observer 不变。验证多视图资源统计、detach 归零、重绑及工程替换 generation。
2. 四格曝光逐格等于分别运行的单格相机 oracle；实际资源拒绝保持旧四格像素。
   相同 key 保留同一失败 Arc、原错误及预算类型；更换 frame 后显式成功。
3. 自制 RGBA decoder 通过真实 VideoFrames／ExposureMedia／GPU 路径运行 120fps
   视频在 30fps 合成中的四相位曝光和相邻 PTS 混合。门闩暂停第二个采样时间点，
   连续八次 Present 仍保留整个旧组；其他格不推进接收器，也不取代在途解码。
   释放后完整更新、四格像素一致、视频 cancellation 为零，工程不变。
4. 桌面实际 serial worker 用既有 test-only `TestStudy` 入口调用上述 API。
   缺失相机的原始 cause 同时出现在默认 Info 配置的文件及 Error stderr，stdout 隔离；
   受控阻塞发生时，in-flight 和 Info start 已发布。正常调用及后续恢复不改变工程。
   这是新 API 在原 worker 信道的证据，不代表生产 `UiFrame` 已携带多视图。
5. 新 GPU 边界用例验证额外 retained 容量的精确预算、少一字节拒绝、饱和溢出拒绝，
   被拒绝时不进入 callback，已完成像素不变。

测试输出、源／程序身份、最终通过数及负向对照见
[本轮证据清单](viewer-multiview-host-evidence/manifest.json)。不重复累计中间执行。
最终复验共 68 项通过、0 失败、0 忽略：宿主 10 项、渲染器 34 项、桌面 24 项。
带 FFmpeg 的桌面测试程序和开发版应用均编译成功，分别仍有 26、37 条编译警告；
本轮没有将这些既有／尚未接入的 dead-code 等警告描述为已清理。
默认日志另保留本机 Vulkan loader 的 Epic Overlay 清单缺失报错，未过滤或吞掉。
第四阶段的 56 项及程序 hash 是同步 main 前的历史记录，以本轮冻结源码复验为准。
所有中间失败保留：测试选择参数错误、默认 Temp 目录 ACL 拒绝，以及自制视频夹具
超过来源时长／遗漏显式相机。分别修正调用环境或夹具，没有放宽断言或生产校验。

调度负向对照实际把逐格推进临时换成交替推进：编译成功，视频用例在暖机的
真实媒体／GPU 路径因完整组一直未就绪而退出 101（35.82 秒）。不是编译失败或
零选中，也不能据此断言所有真实 codec 都会出现同一时序。`finally` 逐字节恢复
`viewer.rs`，前后 SHA-256 均为 `602deaee536ccfd44b2ba9dafc2d617522369617b3debb3797bb673e28dc4323`。
未改生产表达式预算、poll slice、decoder 期限或验收断言；恢复后使用相同测试复验。
随后仅补上上述 renderAttempts 记账及其复用断言，最终源码身份另列于证据清单。

## 观察审计

| ID | 分类 | 证据与处置 |
|---|---|---|
| MV-HOST-01 | 新功能／错误边界 | request key、整组输出和 terminal GPU/media cause；真实预算拒绝及原 Arc 身份检查。没有新增日志站点。 |
| MV-HOST-02 | 正常分支 | 无 graphics、pending media／exposure 和 surface 暂不可用沿用 false；受控视频门闩证明旧组与任务均保留。 |
| MV-HOST-03 | 既有观察复用 | 真 worker 的开始、in-flight、默认文件／stderr／stdout；新 API 不在持有 Session 时另写文件日志。既有原生单视图 Present 回归另验静默热路径。 |
| MV-HOST-04 | 协议／panic 边界 | 不新增线程、期限、循环重启或生产 panic 捕获。有限曝光 poll 不能抢占同步 evaluator／driver；worker 生命周期和 hook 仍由既有实现负责。 |
| MV-HOST-05 | 资源 | 额外单视图容量纳入同一额度；ledger 包含曝光任务；原生 detach 后 admitted bytes 为零，工程替换旧状态清空。非实际 VRAM 统计。 |
| MV-HOST-06 | 待完善 | 生产多视图 UiFrame、每格工具／inspection、预算降级和 profiling 的端到端接入、曝光子采样的跨格求值共享，以及 calibrated-depth 曝光专门验收。不能将新 API 的成功用例冒充这些功能已交付。 |

#221 仍开放。还需真实的布局／视角菜单、各格 zoom/pan 和拾取／编辑路由、共享选择、
主窗／浮窗持久化与重开、取消和失效、中文／英文及 DPI 的原生交互截图。
本轮没有新增用户可见 UI；隐藏窗口/GPU 输出不作为样式截图。

DESIGN §0.7 D01–D12 retained，原因沿用[第一阶段](viewer-multiview-state.md)逐项记录；
§12 的新布局验收仍待 UI 接入。没有新增依赖、第三方代码或外部资产。
