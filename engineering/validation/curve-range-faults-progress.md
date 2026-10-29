# 曲线范围默认故障信道阶段（#259，仍未完成）

接续[原生范围阶段](curve-native-ranges-progress.md)。编译源码 `819c12239ec33da91d80b7936122bdc4af54ff65`，二进制 SHA-256 `dea91f27a50cec74ff82fa16fd38c7b669a62814f3973c63fb6f3344b0c3eb64`，冻结的 Rust/WGSL/Cargo/样式来源前后哈希一致。134 项普通用例通过，1 项原生用例在本轮默认 filter 中明确 ignored；本轮没有重新运行原生窗口，先前 32 场景见上阶段，不能冒称当前源码的全量最终验收。

## 新增的实际故障证据

普通属性图、效果图、对比图分别实际触发 view_bounds、adaptive、fit_all、fit_selection，共12站点。调用生产渲染入口、实际 Core sampler、真实菜单开启、真实选区，不替换 sampler 回调、计数器或返回值。原4个 Mini 默认信道用例也在本轮通过；共16个新增站点有实际默认信道证据，尚余4站点，见结构化结果的精确ID。

| 类别 | 实际触发 | 责任边界 |
|---|---|---|
| view_bounds（三类图） | 旧Y偏移 f32::MAX；对比图外部 offset=1e300，范围端点不可表示 | 表示层退化状态，不声称正常导航或合法保存工程会产生该状态 |
| adaptive（三类图） | 实际样式加载器接受 graph.sample.error=0；生产采样拒绝无效几何 | 表示层输入边界，不修改生产token，也不降低生产采样预算 |
| fit_all/fit_selection（三类图） | Engine验证通过的36000帧工程，各有33000个交替InOut关键帧；窄视口显示正常，整段/两端选区适配达到65536输入预算 | 实际合法创作轨道；失败必须禁用适配，不能返回部分范围 |

每站点在每个子进程实际触发120次；分别核对第1、59、60、120次计数。首次 Warn 仅进文件，第60次 Error 同时进文件/stderr，之后抑制；每次穿插健康绘制不重置既有 process-lifetime Counter，第二个新进程重新验证计数范围。每个进程恰有12条 Error stderr，stdout没有诊断。保存原始原因、phase和对象/组上下文。对比图重复相同失败状态保持负缓存，样式恢复后重新采样；改为单关键帧选区后恢复可用的选区适配。工程作者数据、revision及undo/redo状态不变。

被动绘制的静默断言保留为只有日志初始化一条Info；收集曲线是既有显式命令，准备步骤在日志初始化前完成。没有清除失败、隐藏panic或放松旧断言。现有Counter的首次Warn/累计到60的Error是既有有界面板观察例外；60是进程生命周期内次数，不是一秒，也不在健康帧重置。

## 保留失败及限制

App21 编译失败：新测试直接调用私有收集方法；改为测试模块中调用现有生产收集入口，不扩大产品API。App22 编译成功，原133用例通过，新测试将正常收集命令的Info误计入被动绘制静默检查；移动准备步骤，保留静默断言。App23 全部134项通过，两个新隔离进程均通过。全部失败日志保留在证据包。

本阶段只有测试改动，没有增加依赖或第三方资产，没有改面板布局/颜色/尺寸或采样返回语义。新增站点的generation/revision上下文最终审计仍待完成；本阶段证明的是这些路径和信道，不能据此声称完整可观测性或完整#259已完成。效果/对比读数和Mini adaptive/normalized_readout四站点仍需实际证据或明确的边界处置，不能由其他站点测试代替。

## DESIGN §0.7逐项处置

本轮未改下列12处，继续逐项保留及原因，不声称整改完成。

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

性能、最终格式/all-targets/最新main组合、完整DESIGN12责任边界和自身PR/CI仍待完成。最新main已到a8b5ac19，当前编译基线仍包含72d23d74，不能以旧基线通过替代最终组合检查。

[结构化结果](curve-range-faults-progress-results.json) · [原始证据包](data/curve-range-faults-stage-23.zip)：55项/273645字节，SHA-256 `5b7ee4eab5a9a164821135646a8ba9d6405cb13f88becfe86c49fc7c05d6ef50`。
