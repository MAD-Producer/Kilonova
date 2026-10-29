# 曲线读数与防御边界验证（#259，仍未完成）

接续[十二处故障阶段](curve-range-faults-progress.md)。编译源码 `8ec085709d674973ee02c97e507ca212f630097d`，合并基线 `72d23d740f7885d7a3c1d118fd5e547564e3ccc1`，二进制 SHA-256 `50c41a4d234f2373d7e6d36da7c04229e5c3228db3eef8db00342d909834dca6`。Guard25实际编译并通过29项定向用例；App26逐文件/文件集合/二进制哈希核对后复用这份完全相同的编译产物，运行九组完整filter，136项通过、1项原生明确ignored。没有为同一源码无故重复编译，也没有把ignored或旧原生截图重标成当前验收。

## 功能修正与日志补充分别说明

效果图的悬停入口此前只处理Err；真实速度采样可以返回Ok(∞)。短composition视口的曲线采样被末帧截断，而鼠标悬停可落在其后尚可见的 authored interval；一个fps1e308的退化provider快照即可使实际导数转f32时溢出。旧入口没有报告失败，仍把非有限数值交给读数绘制。新增检查只在悬停读数入口拒绝非有限结果并进入既有observe，不改Core sampler、参数值、曲线采样语义或历史。原始Err通过and_then原样保留，原16项效果回归（包括原NonFinite及原始错误断言）全部通过。

Guard24先运行原入口，新回归在首次故障期望的1条记录处得到0条而失败；Mini12项和效果16项仍通过。Guard25补上述有限值检查后同一回归通过。保留真实失败日志，不修改旧断言或把失败改为ignore。该案例明确是非法fps/缩短composition的退化provider边界，不声称合法保存工程会生成此快照。

独立的日志改动为普通/效果/对比图全部14个新增站点补document generation和revision。完整App26验证原12站点、新2读数站点均实际输出这些上下文，同时保留phase、对象/效果/参数或单位组、原始原因及已有耗时。其他Mini站点原本已有这两个字段。健康Present绘制仍不新增日志、文件IO或start/done记录。

## 最后四个观察站点的精确处置

| 站点 | 证据与结论 |
|---|---|
| effect.graph_readout | 真实效果图悬停、Core物理导数的f32溢出，19个实际故障站点之一；不改变原采样失败返回 |
| curve.comparison.readout | 同一真实效果轨道加入对比图；可用曲线和悬停失败同时存在，恢复fps后恢复健康读数 |
| curve.mini.adaptive | View::draw接受的正有限冻结Y范围0..1e-310导致pixels/unit溢出，实际adaptive拒绝；重复几何负缓存，恢复普通范围后重新采样；是注入的表示层快照，不声称普通滚轮能产生 |
| curve.mini.normalized_readout | **防御边界处置**，不是新复现/修复缺陷。依据以下有限域约束、22个边界合法Easing/22550次实际Core采样、实际健康悬停及畸形数据在fit阶段被先行拒绝；没有伪造一个不可能的错误回调来冒算覆盖 |

读数2站点、Mini adaptive1站点均在两个独立子进程中验证首次Warn、第59次仍仅Warn、第60次Error、第120次抑制，Error只再复制到stderr，Warn仅文件，stdout无诊断。穿插健康调用不重置既有process-lifetime Counter；新进程重新计数。Mini adaptive同一失败范围额外重复120帧，仍只有第一次观察；变化/恢复时按实际几何重新采样。所有步骤不改作者数据或undo/redo。

## normalized_readout有限域依据与限制

核对生产`mini.rs`的View::draw：读数仅在time属于闭区间0..1、automatic成功、无冻结/菜单时运行。核对`crates/motion-model/src/curve.rs`：无Curve的五种Ease将t夹在0..1；Bezier控制X在0..1、Y及端点绝对值≤8，progress端点固定0/1，逆解最多24步并把非有限Newton候选退回有界二分，u保持0..1，因此多项式系数及求值有限。Velocity Bezier area绝对值≥0.05，definition_scale与归一化定义的除法不除零；系数和积分均有界。Progress速度另有现存分母下界/±1e6上限，但该progress模式读数使用progress。

Elastic的w=2π·oscillations，oscillations属于0.5..8、damping属于0.5..20。归一化分母是`1-exp(-d)*(cos(w)+(d/w)*sin(w))`；绝对振幅不超过`exp(-d)*sqrt(1+(d/π)^2)`，在d≥0.5单调递减，最大值<0.615，故分母>0.385。所有三角、指数、系数及归一化结果有限。Velocity Elastic的definition_scale保留1，结论同样成立。

这些是当前有效模型契约的源码依据。22例/22550样本不是对全部浮点输入的穷举；畸形Easing用NaN控制点实际先在fit被拒绝，不声称覆盖所有未来契约变化、任意坏provider或新CurveShape。保留normalized_readout的观察作为防御，不计为实际错误注入通过，不删除检查、不吞错误。

合计：新增20站点中19站点取得真实默认故障信道证据，1站点有上述防御边界处置；不称20个缺陷被修复。既有站点只算回归证据。精确ID/位置/处置在ZIP内inventory；每个实际站点均核对generation/revision、Error stderr/file及stdout隔离。原Counter有界进程生命周期范围保持不变，60是次数不是时长。

## DESIGN §0.7逐项责任边界

没有增加依赖、资产、token、属性行或UI颜色/尺寸；仅一个非法读数的显示边界功能修正，另有故障上下文和测试改动。以下既有12处逐项保留，不冒称整改完成。

| 基线 | 处置与原因 |
|---|---|
| Path六处纵向步进 | retained with reason：未修改Path行，本轮不重新验收该处。 |
| Path两列分割 | retained with reason：未改Path列宽。 |
| Path按钮高度 | retained with reason：未改Path按钮和原命中范围。 |
| Path自绘字段背景 | retained with reason：未改数值字段。 |
| Path布尔宽按钮 | retained with reason：未改Path布尔，新锁轴使用原生Checkbox。 |
| Effect Controls行高/步进 | retained with reason：未改效果属性行。 |
| Effect Controls fx布尔 | retained with reason：未改效果开关，不冒认主分支整改。 |
| Effect Controls枚举 | retained with reason：未改枚举语义或控件。 |
| Effect Controls手工hover | retained with reason：未改该处，没有新增颜色。 |
| Effect Controls层级 | retained with reason：未改属性树层级。 |
| Timeline标签栏列坐标 | retained with reason：未改标签栏坐标，继承主分支独立开关列。 |
| Timeline吸附线颜色 | retained with reason：未改吸附反馈或既有颜色。 |

性能、最终格式/all-targets/最新main组合、最终原生复验、完整DESIGN12处置及自身PR/CI仍待完成；本阶段不关闭#259。当前编译仍是72d基线，不能代替已更新main的组合验证。

[结构化结果](curve-guard-progress-results.json) · [原始证据包](data/curve-guard-stage-26.zip)：100项/297024字节，SHA-256 `6f7837434c7874c5b27b88193ce87827eb61e26369fb4c70dd601f1162d09f85`。
