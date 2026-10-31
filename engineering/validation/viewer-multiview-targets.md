# 多视图输出事务与同表面合成（#221，第四阶段）

接续 `2e625374` 的[共享帧取样](viewer-multiview-frame.md)。开始前检查开放 PR、
head 和工作树，#575 的预渲染仍由另一分支处理。本轮没有改 session、桌面 engine、
预渲染调度或另一工作树，只新增 renderer 输出组件及最小的 Presenter/SDR 内部复用。

## 新增行为

`multiview::ViewFrameTargets` 接受 1／2／4 格输出，复用现有 SDR 双缓冲：

- 准备前统一检查整组的旧 front、新 candidate、布局切换时保留的槽位，以及
  新槽位初始化 front。四格不能各自套用一份完整 preview target 预算。
- 同时变大、变小或改变格式时，先释放所有尺寸／格式过期的 candidate，再分配
  任意替代目标。精度估算复用 SDR 的实际 native＋display 容量。
- 各格按现有 renderer 规则独立编码和提交，全部成功后才一起切换 front。
  不能把多次 queued uniform 写入误认为可在单个延迟提交中安全复用。
- callback 的 `Ok(false)` 表示正常等待素材／曝光，保留完整旧组；Err 原样返回，
  不加重试、不清理主错误、不改为成功。后续格子不再执行。
- 回调 panic 原样向外展开，不增加生产 catch 或恢复策略；已发布组不变，
  未发布所有权有界，下一次显式 update 可替换候选。
- 成功路径保留两组缓冲；显式 clear 释放组件所有权，在途 GPU 命令仍按原生
  allocation lease 保留。资源统计沿用原有 ledger，不宣称是实际 VRAM。

`ViewFramePresenter` 为每格保留独立的来源绑定与 uniform，向调用者提供的同一张
surface texture 编码，只有首格清除目标。后续格使用 Load，不覆盖之前的格子。
各格有独立 viewport 和虚拟 canvas image，支持之后接入不同缩放、平移。
检查格数、目标尺寸／格式、有限数值、非重叠和设备归属；设备重建需显式 clear。
旧 `Presenter::encode_view` 继续使用 clear 语义，不改变旧单视图的呈现约定。

这个组件不持有窗口、不自行获取 surface、不读取工程来源、不轮询或等待。
宿主将来应先准备整组媒体和曝光，再获取一次 surface，编码多格和 chrome 后呈现。
callback 的存在不代表已经实现多视图曝光、视频或深度调度。

## 最终源码验证

新增 6 项真实 GPU 测试通过：

1. 1→2→4→2→1 格数切换，逐格检查实际颜色和间隔。旧 clear 语义的负对照
   必须丢掉前几格像素，证明测试能识别“后格清掉前格”。
2. 第二格正常 pending、受控错误及真实 panic 均不发布前面已提交的新 candidate；
   原 front 身份、字节和完整格数保留。检查原错误类型与 cause、原 panic payload，
   panic 后保留资源不超过四格两组 native/display，再显式成功更新并验证全部颜色。
3. 四种工作精度分别验证整组精确预算边界；少一字节在 callback 前拒绝，
   原四格像素不变，恢复预算后成功。保留 typed PreviewTarget 拒绝及准确容量。
4. 同一次场景取样派生四格相机，各格不同 canvas image；同目标输出的每行
   像素等于独立单格 Presenter 输出。相机和 canvas 替换的负对照必须不同。
5. 空／三／五格、零尺寸、设备尺寸越界、非法 viewport、重叠格、退化 image、
   外 renderer 的 target／presenter 均被拒绝；clear 后可绑定新设备。
6. 在同一组件上往返切换四种工作精度，display 字节相同、native 格式准确。
   双缓冲预热后三次正常更新不增长 admitted bytes 或 live allocations，不执行
   renderer readback；ledger 的总已知纹理容量与组件统计相等。

既有呈现 8、SDR 6、preview target 预算 5、共享帧 8 项也通过：核心共 33 项。
带 FFmpeg/GPU 的桌面测试程序及实际 dev 应用均编译成功，app 各保留 26／36 条
告警，没有通过 allow 屏蔽。桌面视图状态 7、Workspace 7、导航 3、合成面板 5、
原生失败重绑定 1，共 23 项通过。与核心合计 56 项不同测试，不累计中间重复执行。

原生重绑定测试使用隐藏窗口、真实 surface 和实际截图：受控 attach 失败后保留
原设备工厂和画面；默认 Info 下的同一 Error 出现在文件与 stderr，stdout 隔离，
健康 Present 没有 start 刷屏。Workspace 两轮 61 次非法加载另验证第 60 次阈值、
抑制和成功重置。这些是旧路径回归，不能当作多视图 worker 已接入的证据。

首次新测试编译把 AdmittedTextureView 当作可比较对象，修正为比较底层 texture
身份，并继续严格比较字节。随后四项通过、一项失败：测试遗漏 RenderError 的
既有 `GPU validation:` Display 前缀；改为直接严格匹配原 variant 和原 cause。
扩展资源用例时误用了 ledger 方法名，编译拒绝，改为实际的 total_known_bytes。
所有原始失败日志保留，没有修改生产错误返回、旧测试断言或阈值来通过验收。

原始日志、截图、源码／程序身份见[证据清单](viewer-multiview-targets-evidence/manifest.json)。
四格图是 renderer 取样结果；原生 before/after 是旧单视图用例，均不代表新的 UI 设计。

## 审计和仍待完成

| ID | 分类 | 处置和边界 |
|---|---|---|
| MV-TARGET-01 | 新错误边界 | 不合法格数、几何、设备、总预算均返回错误。callback 的 typed 错误和原 cause 不改写，受控错误／恢复已有 GPU 证据。 |
| MV-TARGET-02 | 正常等待 | callback false 不计为故障，旧组保留；不增加日志键、轮询、超时、重试或重启。 |
| MV-TARGET-03 | panic 边界 | 不吞 panic，不自行恢复 worker；测试仅 catch 以核对 payload、旧组和有界所有权。生产仍由现有宿主 hook／worker 契约负责。 |
| MV-TARGET-04 | 既有观察 | 原生 attach 故障和 Workspace 默认信道均实际验证。没有新增日志站点，不能将旧路径覆盖冒充新增调用链的端到端观察。 |
| MV-TARGET-05 | 待接入 | 新组件尚未由桌面 worker 调用；未来命令须沿现有 Info 开始、in-flight、耗时、Error 文件＋stderr 和 stdout 隔离执行。原生单表面获取次数、阻塞观察仍需宿主验证。 |

#221 仍开放：尚缺 Session 的多视图媒体／曝光／深度和缓存调度、桌面 worker 的
真实批量呈现、布局与视角入口、每格工具／拾取及焦点路由、主窗／浮窗生产保存
恢复、取消／失效和中英文原生截图。不能把本阶段的离屏同纹理输出说成窗口已完成。

无面板、token、图标或命中容差修改；DESIGN §0.7 的 D01–D12 全部 retained，
原因沿用[状态阶段逐项记录](viewer-multiview-state.md)。§12 的新布局验收仍待接入。
没有新增依赖、第三方资产或复制的代码；测试图形独立编写。

同一轮期间，自有 #577 的提交 `203d3e4f` 五项必需 CI 全部成功后按准确 head
合并为 `f2e9f0e8`，#236 与 #159 自动关闭。未审查或合并 #575。
