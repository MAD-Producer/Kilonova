# 图层锚点动画（#202）

## 功能契约

- ANCHOR-01：图层 `Transform.anchor` 是独立二维 Track，单位仍为源矩形归一化坐标；矢量组锚点及其单位不变。静态字段继续序列化为旧二维数组，打开旧工程仅构造常量 Track，不覆盖原文件；首个锚点关键帧要求格式 20。没有添加第三个锚点轴或分离锚点轴。
- ANCHOR-02：Property::Anchor 复用 Animate、SetVector、MoveKey、CopyKey、DeleteKey、Ease、Curve 和历史。SetVector 保持既有三元素请求形状，锚点使用前两项且要求 Z=0；不支持的轴/摄影机/表达式请求保留明确错误，UI 不提供锚点表达式入口。
- ANCHOR-03：保留全局 `anchor` 命令的画面补偿语义；新 `anchor_at` 在合成帧对应的本地时间设置锚点并补偿当帧位置，保留分离位置和其他锚点关键帧。静态锚点数值/手柄继续使用全局命令，已动画锚点使用当帧命令。关键帧 SetVector 编辑锚点轨道本身，不隐式改变位置轨道。
- ANCHOR-04：Scene 的几何偏移、宿主投影和 sampledLayers 使用同一个本地时间采样。父子绑定仍使用既有 pivot 坐标契约。冻结导出保留真实 Track；不是 UI 显示值或矢量组数据。
- ANCHOR-05：属性面板和时间轴将原静态锚点行改为秒表属性行，沿用原布局、数值字段、命中容差；不增加占位行。A/Shift+A 与 P/S/R/T 使用相同的属性筛选路由，保留 Ctrl+A、文本编辑和焦点优先级。曲线以源像素显示和编辑二维锚点，统一换算归一化数据。没有新增右键菜单项。

## 错误与观察审计

| ID | 类别 | 证据 / 边界 |
|---|---|---|
| OBS-01 | 既有错误信道 | 非零 Z、越界值、帧越界和不支持的锚点轴拒绝；共享引擎回滚模型/历史。实际桌面 worker 的非法 SetVector 回复、默认 Info 文件和 Error stderr、stdout 隔离有子进程验证。正常取消和关闭动画不是故障。 |
| OBS-02 | 新增退化观测 | 已动画图层缺少 sampled anchor 时，经既有 observe 基础设施报告 `properties.anchor_sample`，保留基础值显示；曲线源尺寸不可用时报告 `animation.anchor_display_scale`。健康路径不添加文件读取；键和上下文只含固定 site、ID/轴。沿用面板固定调用点、进程生命周期累计计数：首次 Warn 只写文件，第 60 次 Error 写文件和 stderr，其后抑制，计数饱和；健康行不重置，以免同一列表里的健康行阻止坏行达到阈值。仅退出进程重置，不宣称恢复后重新报警。默认 Info 子进程验证两次坏数据间穿插健康行仍只升级一次、stdout 隔离。 |
| OBS-03 | 既有覆盖 | 请求进入 serial worker 前的 in-flight/start、完成清理、panic 和阻塞观察沿用原架构；本次不新增线程、超时、重启或重试，不吞掉错误。 |
| OBS-04 | 协议/功能边界 | Track 及格式 20、当前帧补偿命令和 UI 属性路由为功能改动；日志规则不替代这些契约的独立验收。 |

## 验证

- 原静态锚点、固定轴旋转补偿回归通过；原断言语义保留，测试读取静态 Track 的采样值。
- 新共享用例 4 项通过：旧序列化/旧姿态、局部时间/保存重开、父子绑定与分离位置下的补偿/取消/撤销、关键帧编辑及非法操作原子拒绝。
- 实际 FrozenMovie 测试通过：冻结锚点 Track 后编辑当前工程，输出 MP4 并再解码；首末帧两处像素确认动画位移与冻结隔离。
- 完整共享层回归通过：`cargo test -p motion-core -p motion-model -p motion-host --locked -- --test-threads=1`，66 个测试程序共 319 passed / 1 ignored，`anchor-shared-tests-final.log`。
- 渲染回归通过：`spatial_effects` 4 项、`vector_adjustment` 9 项，共 13 项，`anchor-render-tests.log`。
- 最终桌面完整回归通过：`cargo test -p motion-desktop --features ffmpeg --locked -- --test-threads=1`，423 passed / 1 ignored，`anchor-desktop-verified-tests.log`；包含两个新观察 site 的真实默认级别、阈值/抑制与进程调用点累计范围验证。
- FFmpeg 优化构建通过，13m51s，22 条既有警告；`anchor-release-build.log`。二进制 SHA-256：`8abda07595518ed6e26e95db3c37ac8943e40afec3d68f3d00e2201299d98912`。
- `tools/ci_desktop.py` 对该二进制完整通过：中英文主窗、实时节点、共享 MCP，以及两种语言的样式覆盖原生浮窗。新增主窗三个截图为 `smoke-{en,zh}.layer-anchor-animation.png`、`.timeline-anchor-animation.png`、`.timeline-anchor-graph.png`；报告验证截图尺寸、存在性和工程恢复。
- 额外默认中文原生浮窗通过，19 个面板、`overrideActive=false`。实际检查 `default-panels-zh.panel-11-anchor-keys.png`、`.panel-4-anchor-keys.png`、`.panel-4-graph-anchor-graph.png`；锚点与位置关键帧是两条独立属性轨道，截图中的曲线只选锚点 X。保持秒表、属性名和数值列原有对齐，不添加行或缩小命中容差。没有新增右键菜单项。
- 上述 JSON / PNG / stderr 位于 `E:/Dev/aem/artifacts/layer-anchor-animation/final/screens`。stdout 没有新增失败日志。节点及浮窗 stderr 仍有既有 U+2194 缺字升级 Error（`timeline.rs` 的缩放提示），与 #203 时一致；本次未修改字体覆盖，不将此项报告为已修复或全局无 Error。
- 样式消费检查、`git diff --check` 通过。可运行包及其机器可读 SHA-256 / runtime 哈希清单位于 `artifacts/layer-anchor-animation/final/package`；打包验证在代码提交后执行，保证元数据指向已提交源码。
- 日志根目录：`E:/Dev/aem/artifacts/multiple-camera-layers/anchor-*.log`。首轮新测试的 f32 JSON 期望误用了 f64 字面量，保留失败日志线索，现用同类型 f32 精确比较；没有弱化既有断言。
- 首轮完整回归的处置：格式 20 已新增支持，三个原未来版本拒绝用例增加接受 20 / 拒绝 21，仍验证未知格式必须拒绝；属性行用例改为显式选取 Position，保留原三个轴和关键帧断言，并新增 Anchor 静态行检查。默认键位目录通过真实解析器的生成模式更新，文档一致性断言不变。新增退化阈值用例原先复用了实际宿主的完整采样，未触发预期缺失；现在明确移除采样。第二轮暴露新增用例对既有面板计数策略的错误假设：该策略是进程调用点累计，首次 Warn 加第 60 次 Error，并非每次恢复后重新计数；按 OBS-02 明确其范围及限制，分别验证文件的两条记录、stderr 的单次 Error、stdout 隔离和穿插健康行不重新输出。未改动既有计数器或生产返回值。失败记录保留于 `anchor-shared-tests.log`、`anchor-desktop-full-tests.log`、`anchor-desktop-full-tests-final.log`、`anchor-observation-isolated.log`。
- 3D 图层的曲线入口强制限制二维锚点组件，避免沿用上个属性的 Z 轴；新增非正方形源尺寸和二维曲线拖动回归，确认两轴换算及未编辑分量保持不变。
- 来源核验：沿用 PROVENANCE.md、ATTRIBUTION.md、NOTICE；无新增依赖或第三方资产，测试色块及文字独立编写。

## DESIGN §0.7 逐项处置

| ID | 位置 | 状态 / 原因 |
|---|---|---|
| D01 | path_controls 纵向步进 | retained：本次只接入图层锚点，不重排路径属性。 |
| D02 | path_controls 双列宽 | retained：未改路径双列。 |
| D03 | path_controls 按钮尺寸 | retained：未改路径按钮或命中容差。 |
| D04 | path_controls 自绘字段 | retained：锚点继续使用既有原生 NumberEditor，未替换路径字段。 |
| D05 | path_controls 布尔按钮 | retained：未改路径布尔值。 |
| D06 | effect_controls 行高 | retained：未改效果属性行。 |
| D07 | effect_controls 启用控件 | retained：未改效果启用。 |
| D08 | effect_controls 枚举 | retained：未改效果枚举。 |
| D09 | effect_controls hover | retained：未改效果悬停。 |
| D10 | effect_controls 层级 | retained：未改效果树。 |
| D11 | 时间轴 11 列 | retained：锚点仍占原单行，沿用共享字段列；保留其他列和关键帧时钟。 |
| D12 | 吸附颜色 | retained：未改吸附反馈。 |

§12 本次只声明锚点相关属性、坐标、输入与输出的验证；其他面板、字体覆盖、OS Reduced Motion 和完整 DPI 矩阵仍在原范围内，不声明全仓合格。
