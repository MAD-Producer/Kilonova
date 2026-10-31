# #221：独立视图检查与快照隔离

本阶段在阶段六的生产 Present/CaptureUi 通路上接通逐格通道、ROI 和快照比较，
替代此前明确拒绝这些设置的边界。产品的布局入口仍未开启，#221 不在本阶段关闭。
源码、原始执行输出、图片和程序身份见
[证据清单](viewer-multiview-inspection-evidence/manifest.json)。

## 行为和资源边界

- `Cell` 携带自己的检查设置，`Plan.active` 指定活动格。顶层共享检查设置继续拒绝，
  原因改为要求设置到每格，避免共享设置覆盖所有格；原有拒绝断言仍执行。
- 颜色呈现使用显示图像，R/G/B/A 使用工作图像；每格有独立 Presenter、uniform、
  ROI 和比较源。编码前验证所有格的检查几何和比较源 renderer 所有权，
  错误包含格索引和阶段，并保留原始类型化 cause。
- 快照按 `(window, cell)` 保存，原单视图使用独立的 `None` 槽位。
  每窗最多五个槽位，不按帧或请求无限增长。关闭窗口删除该窗所有槽位；
  文档、合成或 GPU 上下文变化清空快照和活动格。替换快照成功后才发布新 generation。
- 活动格仅在完整 Present 成功后更新。CaptureViewerSnapshot 检查已完成组的
  文档、revision、时钟、窗口、surface epoch、视图版本和质量；旧组、其他窗口、
  未完成或错误状态均不能成为新快照。浮窗保留图像的捕获不要求临时换回其 surface。
- 比较源计入保留组和截图提交的资源账本；关闭比较清掉绑定。没有修改分配预算、
  生产质量策略、线程、重试、超时或返回值。现有 encoder 分配作用域提供在途保留，
  本阶段没有把早期关于临时 Presenter 生命周期的怀疑当作已确认缺陷。

## 验证

本轮 26 项不同测试通过，0 失败、0 忽略：桌面相关回归 14 项、显式执行的原单视图
快照验收 1 项、渲染器 8 项、宿主 3 项。先前阶段、首次失败、负对照和恢复重跑不重复
计数。样式消费者检查通过。Windows FFmpeg 测试版和开发版编译成功，分别保留
26 和 38 条告警；没有用 allow 压制尚未启用的产品入口告警。

新增渲染用例在 Unorm8/Unorm16/Float16/Float32 四种工作精度下，比较四格与四个
独立工作图像 Presenter 的实际输出，覆盖通道、不同 ROI、自己的历史快照、清除和恢复。
最后一格非法 ROI 必须在编码任何格前拒绝，已呈现像素保持不变。

新增桌面用例使用真实隐藏的主窗和浮窗，经生产 Present、CaptureUi、
CaptureViewerSnapshot、SaveViewerSnapshot 运行：

1. 两台相反方向相机分别抓图、保存；两个活动格的实际图片不同。
2. 时钟从 0 变为 10 后，格 0/1 显示各自的旧快照，格 2/3 保持当前画面。
   所有格逐一比较实际裁剪像素。修改检查设置不增加内容 renderAttempts。
3. R/G/B/A 和半格 ROI 独立生效；关闭检查恢复当前图像，重新比较恢复原比较图像。
4. 未捕获的格、未捕获的浮窗、其他窗口已接管的组和已关闭浮窗产生真实错误。
   默认 Info 文件和 Error stderr 保留原 cause，stdout 隔离；健康 Present 无 start 刷屏。
5. 主窗和不同尺寸浮窗均以 High 捕获完整 32×32 工作图像，不把物理格子尺寸当作
   工作图像尺寸。工程、revision 和撤销状态严格不变，只有显式 seek 改变时钟。

回归另覆盖原单视图保存/重开、四精度显示编码、surface 拒绝、UI 上传错误、
分配观察计数阈值与恢复、阻塞 worker 状态读取、视频等待、曝光跨窗口和导出取消。
原单视图快照测试原本 ignored，本轮用 `--ignored --exact` 显式执行；未执行项不计通过。

## 首次失败和负对照

首轮渲染测试误把 CaptureTarget 当作接口要求的 RenderTarget，编译失败后修正新夹具，
使用固定显示编码的输出目标；原输出保留在 `build-renderer-first.log`。

首轮新增原生测试把快照宽度误写成屏幕格子宽度 48，而 Balanced 的 32 像素合成实际
工作图像宽度为 21。修正的是新测试的契约：明确选择 High，严格验证合成尺寸 32，
并在不同格子大小的浮窗执行同样断言。生产质量计算没有改，原有测试仍使用 Balanced。
首次失败及修正后输出分别保留在 `native-first.log`、`native-resolution-fixed.log`，
不把夹具修正描述为修好了生产尺寸问题。

运行时负对照只把逐格比较源查找临时改为统一取格 0，不改任何测试断言。
编译成功后，新用例因格 1 的真实 PNG 像素错误退出 101；源码在 finally 中逐字节恢复。
恢复前后 SHA-256 和正向程序身份见 `negative-receipt.json`；恢复后原正向程序再次通过。
负对照未被当成通过项，也没有通过改变预算、阈值或等待期限规避失败。

## 观察审计和未完成范围

| ID | 分类 | 处置 |
|---|---|---|
| MV-INSPECT-01 | 功能接通 | 每格检查设置和快照路由，经实际 GPU 输出与窗口路由验证。 |
| MV-INSPECT-02 | 既有观察复用 | 使用原 worker start/in-flight/完成和错误信道；没有新增热路径日志或锁内日志。 |
| MV-INSPECT-03 | 真实故障 | 缺失快照、过期组、关闭窗口、非法几何和 renderer 所有权拒绝保留 cause。 |
| MV-INSPECT-04 | 正常分支 | 无检查保持原画面；关闭比较清绑定；等待媒体仍为 false，不升级为故障。 |
| MV-INSPECT-05 | 资源边界 | 快照槽位按窗口和有限格索引持有；比较源入账，不声称证明物理 VRAM 总量。 |
| MV-INSPECT-06 | 待实现 | 产品布局选择、每格导航/拾取/编辑、共享选择、浮窗焦点与工作区持久化接入。 |
| MV-INSPECT-07 | 待验证/完善 | 跨格曝光子采样求值共享、自适应预算/profiling、calibrated-depth 曝光专门验收。 |

本轮没有编辑产品 panel、样式、命中范围或资产。DESIGN §0.7 逐项处置如下，
retained 只表示此阶段保留原状，不宣称该历史问题已修复：

| 基线项 | 状态及理由 |
|---|---|
| D01 path_controls 六处纵向步进 | retained：本轮未改该 panel。 |
| D02 path_controls 双列尺寸 | retained：本轮未改该 panel。 |
| D03 path_controls 按钮/字段高度 | retained：本轮未改该 panel。 |
| D04 path_controls 自绘数值底色 | retained：本轮未改该 panel。 |
| D05 path_controls 布尔按钮 | retained：本轮未改该 panel。 |
| D06 effect_controls 行高 | retained：本轮未改该 panel。 |
| D07 effect_controls 布尔显示 | retained：本轮未改该 panel。 |
| D08 effect_controls 枚举控件 | retained：本轮未改该 panel。 |
| D09 effect_controls 悬停颜色 | retained：本轮未改该 panel。 |
| D10 effect_controls 层级缩进 | retained：本轮未改该 panel。 |
| D11 timeline 列坐标 | retained：本轮未改该 panel。 |
| D12 timeline 吸附线颜色 | retained：本轮未改该 panel。 |

DESIGN §12 产品界面、DPI、菜单和输入验收待入口接通。隐藏窗口的小图是像素/协议
证据，不是产品设计截图。没有新增依赖、第三方资产、字体或图标。
