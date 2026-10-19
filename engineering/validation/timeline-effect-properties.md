# 时间轴效果参数、曲线与缓动验收

分支：`feat/timeline-effect-properties`，基于已合并的 `f32c64de`（PR #335）。#255 的未提交实现保留在独立的 `layer-render-sampling` 工作树，本修复不包含它。

## 问题与功能修复

用户截图中的效果原先只有启用开关、名称、“编辑”按钮和只能寻帧的汇总菱形。`LayerRows` 仅预留每个效果一行，滚动范围缓存还缺少效果分组展开状态；F9 只读取变换属性键。

现在在绘制前冻结效果标题和参数子行计划。高度、裁切、分隔线、命中和后续图层共享该计划；分组和实例展开状态均进入滚动范围签名。效果保留独立启用开关和折叠箭头，参数逐行展示。数值、向量和颜色分量使用现有数字编辑事务；布尔使用 Checkbox，枚举使用透明的原生下拉字段及最后绘制的菜单。动画导轨保留三个完整 `icon.slot` 命中区域，秒表和各层级名称单独对齐。

参数选择复用现有 Effect Controls / Properties 的唯一命令所有者和镜像机制。图表读取真实 `EffectParam.track`，通过 `EffectAction` 编辑实际参数，未伪装成位置/不透明度轨道。支持数值/速度图表、分量选择、键时刻和值、贝塞尔手柄、F9/Shift-F9/Ctrl-Shift-F9 和关键帧右键缓动/删除。普通时间轴只改键时刻；Ctrl 首次复制后移动副本。目标时刻占用时不覆盖。松手使用 release 坐标，一次手势一次提交；锁定、观察、失焦、Esc、工程/合成/实例包身份变化和删除目标取消并回滚。

缓入需要前一段，缓出需要后一段；菜单按实际邻段禁用不可用入口。离散参数保留 Hold，禁止数值缓动。CurveObject 是颜色映射曲线，继续在效果控件中的既有曲线编辑器编辑；时间图表明确显示该边界，不把 LUT 冒充数值轨道。未新增图层汇总的效果键合并拖动、多时刻批量曲线框选或效果空间切线链接。

## 可观测性审计

| ID | 分类 | 证据与边界 |
| --- | --- | --- |
| FXT-OBS-01 | 新增观测 | `timeline.effect_parameter` 对存在却无法解码的参数保留 object/effect/parameter 和原始 serde 原因。复用 `panels::observation`：每进程固定调用点累计失败，首次 Warn、60 次 Error，此后抑制；健康相邻参数不重置，防止混合列表永远无法升级。计数饱和、上下文最多 4096 字符，不新增动态日志键。真实默认日志子进程测试验证文件、Error stderr、stdout 隔离、120 次抑制及正常删除不记故障。 |
| FXT-OBS-02 | 新增观测 | `timeline.effect_curve` 在手柄产生编辑前验证 Easing，失败保留实际目标和原因；使用相同有界调用点规则。默认日志子进程向实际手柄更新函数注入非法纵坐标，验证 120 次失败只生成首次 Warn 和第 60 次 Error，文件保留原始原因、Error 写 stderr、stdout 无日志，未产生非法 Edit；失焦只取消一次，真实引擎工程保持不变。正常有效曲线不写日志或文件。 |
| FXT-OBS-03 | 既有覆盖 | 所有实际编辑沿用 Action/EditBatch → 引擎串行信道，沿用请求 ID、非 Present Info start、in-flight、原始 Err 和完成计时。未新增阻塞任务、线程超时、重试、重启或 panic hook。路由边界观测现包含活动效果时间轴手势。 |
| FXT-NORMAL-01 | 正常分支 | 目标消失、取消、没有键、没有前后邻段、非整数时刻、锁定/观察、目标位置占用都不记故障。正常选择和绘制不建立历史。绘图固定 257 个采样点；手势只保留一个目标、两个手柄控制点或有界键时刻集。下拉缓存每帧仅跨帧保留当前目标。 |
| FXT-BOUND-01 | 功能边界 | 双语、主/浮窗使用同一控制器；窗口/视图切换仍由原路由取消交互。未改变日志等级规则、respond 语义、工程格式、渲染器、插件协议或模型返回值。 |

## 检查与截图

本机材料目录：`E:/Dev/aem/artifacts/property-selection-context/`。

- 初次 `effect-timeline-tests.log` 保留 2 个夹具失败：夹具使用了不存在的 `main` 合成 ID。仅将夹具改为模型的 `MAIN_COMPOSITION`，未改生产断言或编辑返回值。`effect-timeline-tests-correct-composition.log` 三项通过。
- `effect-timeline-graph-tests.log`、`effect-timeline-widget-tests.log` 五项通过，矩阵覆盖两语言、主/浮窗，实际核心编辑、时差、值、手柄及撤销。
- `effect-timeline-full-tests.log`：完整桌面测试 525 通过、0 失败、1 项原有忽略。之后仅补充速度图表钳位修复和相应测试；`effect-timeline-final-targeted.log` 七项全部通过，覆盖钳位平台速度为零、实际手柄非法输入和两处默认日志阈值。未用原始 Track 的超界速度冒充 EffectParam.sample 的导数。
- `effect-timeline-copy-tests.log`：随后增加测试、未再修改生产代码，八项全部通过；实际引擎验证占位键不覆盖、Ctrl 复制后移动副本、合成/局部时差、松手坐标、序列化重新打开、删除后撤销及一次手势一次撤销。
- `effect-timeline-tools-tests.log`：54 项工具测试完成，53 通过、1 项跳过。
- `effect-timeline-debug-native.log` 英/中文原生 GPU 验收通过，每种语言生成主/浮窗 × 参数/图表 × 普通/右键菜单的 8 张截图。截图使用仓库原创 `soft_sphere_motion` 示例，经真实 graph 编译、EffectInstance 和核心引擎编辑；F9 确实产生 Easing，撤销后工程一致。截图不导入用户视频，不声称覆盖视频解码或导出。
- Debug 英文运行的 stderr 保留环境中的 Epic Vulkan layer JSON 缺失记录；它不等于应用回归，也不在本补丁中隐藏。
- 本地运行目录通过既有 `tools.package_desktop.runtime_payload` 验证 FFmpeg 的架构、逐文件 SHA-256、来源与许可证后，放置 DLL、许可证及来源清单。未修改仓库的 FFmpeg 依赖或发行能力声明。
- `effect-timeline-release-build.log`、`effect-timeline-release/build.json`：最终优化 FFmpeg 构建成功，8 分 09 秒。局部验证仅将桌面包 codegen-units 调到 16，保留 release 优化与 thin LTO，未修改仓库构建配置。构建报告明确 video_encoder=false，不声称验证视频导出。
- `effect-timeline-native.log`：对最终运行目录执行完整 `tools/ci_desktop.py` 成功。启动环境 PATH 仅有 Windows 和 System32，未设置 MOTION_LOG；双语、19 面板、五种布局、节点渲染、共享 MCP、默认与覆盖主题、主/浮窗均完成既有验收，没有放宽超时或删除断言。所有该轮 stderr.log 无 Warn/Error。新增效果截图共 32 张，覆盖两语言 × 两主题 × 主/浮窗 × 参数/图表 × 普通/右键菜单；已人工检查中文默认主窗两种菜单、浮窗图表菜单和覆盖主题的浮窗菜单、英文参数行。
- 来源检查通过（1372 个受版本控制路径）；样式消费者检查、暂存差异空白检查通过。程序和 DLL/许可证材料位于 `effect-timeline-release/`，截图与报告位于 `effect-timeline-native/`。

新增效果轨道的原生专用截图覆盖 1200×760 主区域和 900×760 浮窗；现有验收仍包含整个编辑器的最小窗口与默认窗口。本轮没有单独测量新增轨道在各档 Windows DPI 下的耗时，也没有重做全站 §5.3 状态矩阵；这些不是“全 DESIGN 合规”的证据。

## DESIGN §0.7 逐项处置

| 原基线项 | 处置与原因 |
| --- | --- |
| Path 六处纵向步长 | retained：不修改 Path 控件；效果时间轴使用现有 row.height。 |
| Path 两列分割 | retained：不修改 Path 列；新增效果值沿用 timeline::property_layout。 |
| Path 混合按钮高度 | retained：不修改原控件或缩小命中区。 |
| Path 数值背景 | retained：不修改 Path 数值绘制；新增数值复用 numeric/field.number。 |
| Path 全宽布尔按钮 | retained：不修改 Path 布尔；新增效果布尔使用 Checkbox。 |
| Effect 行高/步进 | retained：Effect Controls 的既有步进不变；新增时间轴参数每项一行。 |
| Effect 布尔按钮 | retained：不改既有面板；新增轨道禁止使用按钮模拟布尔。 |
| Effect 枚举按钮 | retained：不改既有面板；新增轨道使用 Dropdown::draw_inline，不循环值。 |
| Effect hover alpha | retained：不改既有 hover；新增行选中使用已有样式，无字面颜色。 |
| Effect 层级 | retained（原面板）；本修复新时间轴标题/实例/参数按 tree.indent 分级，截图验证。 |
| Timeline 固定列坐标 | retained：列和图层行几何不变；参数沿用现有共享值边界，分量同一行。 |
| Timeline 吸附线颜色 | retained：不改既有吸附绘制。 |

新增尺寸仅为图表手柄命中半径、图表顶部/底部保留区及模式按钮宽度，先写入 dark.json，未缩小原图表的 18×18 手柄命中。未新增第三方依赖、素材、字体或图标；未读取 Adobe 二进制或复制参考截图到产品。此文不声称整个 DESIGN 已全部整改。
