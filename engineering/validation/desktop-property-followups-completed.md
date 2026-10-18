# 桌面属性与菜单五项问题完成记录

日期：2026-10-18（Asia/Tokyo）

基线：`origin/main` / `fbde6a2`；分支：`fix/desktop-property-followups`；工作目录：`E:/Dev/aem`

范围：[#262](https://github.com/MAD-Producer/Kilonova/issues/262)、[#209](https://github.com/MAD-Producer/Kilonova/issues/209)、[#219](https://github.com/MAD-Producer/Kilonova/issues/219)、[#235](https://github.com/MAD-Producer/Kilonova/issues/235)、[#245](https://github.com/MAD-Producer/Kilonova/issues/245)。本文区分已有能力、本批新增实现和验证边界，不表示远端 issue 已关闭或主分支已合并。

## 实现与证据

| Issue | 基线已有能力 | 本批实现与真实回归 |
| --- | --- | --- |
| #262 合并 XYZ | 已有保守无损 `MergeDimensions` 命令、项目格式和冻结渲染支持；历史后端交付见 `merge-dimensions.md` | 属性与时间轴共用右键入口，按真实轨道状态显示分离或合并；锁定/只读禁用。`dimension_context_merges_and_rejects_without_losing_axes_in_both_windows` 通过真实桌面 Engine 验证双语、主窗/浮窗、保存重开、单步撤销/重做及不匹配键时间拒绝后原轨道/版本/历史不变 |
| #209 图层锁定 | 模型已有锁定标志、时间轴锁图标和编辑入口保护 | Layer 菜单提供锁定/解锁所选、全部解锁；图层右键“开关”提供单层切换与全部解锁；批量变更只包含实际需要修改的层、保留可见性、使用当前合成作用域并作为单事务提交。修正只读模式下根据“可编辑”推断锁状态的标签错误，禁用锁定层的顶层拆分/复制/删除/添加位置键。`layer_menu_lock_and_unlock_all_are_atomic_and_preserve_visibility` 和 `lock_switch_and_unlock_all_work_in_both_window_routes_without_empty_history` 验证混合选择、隐藏层、无效 ID、无操作、只读、取消、保存及撤销 |
| #219 原始曲线与负时间 | 图表读取作者轨道；模型允许保留负时间键；已有负时间键编辑保护 | 时间轴图表页脚、曲线面板明确显示“不含表达式/约束”，存在负时间键时显示零帧前只读；保留原图表几何与输入坐标。禁止从负起点线段启动曲线手柄编辑，避免负帧转无符号整数后误改零帧；观察视图也禁止启动编辑。`negative_time_segment_handles_are_read_only_and_authored_keys_survive_graph_views` 验证实际点击、Delete/Esc、表达式采样与原始值差异，以及负键保存不变 |
| #235 蒙版模式与手柄 | 当前 main 已使用七态原生下拉、当前项标记、可撤销命令与锁定禁用；没有重复实现七态循环替换 | 选择节点后仍显示其他节点的淡色手柄，命中仍只允许当前节点；用实际 node ID 查找绘制索引，避免删除/重排后突出错误节点；切换蒙版清除旧节点选择。实际截图发现模式菜单被后绘制的父级行遮挡，改为属性内容绘制完后追加菜单。`mask_dropdown_commits_only_the_selected_mode_and_cancel_lock_do_not_edit` 验证直接选择、浏览不提交、Esc/锁定、保存/撤销和菜单绘制顺序；`mask_handles_remain_visible_but_only_the_selected_node_can_be_dragged` 使用非连续节点 ID、偏移视图验证六个手柄可见、未选节点不可误抓、指定节点修改与保存/撤销 |
| #245 表达式目标 | 模型/渲染已有分离轴、效果实例和参数目标；上一批已交付属性内联编辑器 | 属性面板复用时间轴真实行计划，分离轴逐行保留 axis，秒表只切换该轴；耦合向量仍一属性一行，3D 耦合旋转的 XYZ 值同行。效果参数仅对已实现且声明可动画的 float/vec2/vec3/color 开启表达式；bool、enum、curve 及未知类型提供明确禁用说明。`separated_axis_inline_editor_preserves_other_axes_and_round_trips` 验证 X 表达式不覆盖 Y；`discrete_and_curve_effect_targets_show_a_disabled_expression_action` 点击实际禁用项不产生编辑或历史。渲染测试 `separated_axis_and_effect_expressions_match_explicit_values_in_pixels_and_frozen_plans` 将多个小数帧的实时像素和冻结计划与独立显式值工程比较，并验证冻结后不随实时工程改写 |

表达式编辑区仍默认收起，通过属性右键或 Alt+秒表打开；没有新增独立面板，也没有处理 Ctrl+空格等此前明确延后的快捷键冲突。

本批截图复核后，用户明确调整层级：属性面板和时间线均把蒙版放在变换前面，取消“几何设置”外壳，矢量内容继续使用已有分组；3D 直接放到时间线空间图层行，用 Checkbox 与表头表达当前状态。窄时间线优先隐藏混合模式列以给名称和开关留出空间。对应规范已同步到 `DESIGN.md` §7.1/§7.2，尺寸先定义于 `dark.json`。`masks_precede_transforms_and_layer_row_3d_is_undoable_in_both_windows` 验证双语、两种行高、主窗/浮窗、真实命令、保存、锁定/只读禁用与撤销。

用户继续要求时间线显示蒙版标题行的完整操作。本批追加启用 Checkbox、路径编辑、定位并展开蒙版时间线、删除，以及组标题的“＋”菜单（钢笔蒙版、矩形蒙版）。两处标题复用操作实现；菱形保留现有定位语义，不添加关键帧。时间线菜单只有一个有界状态，验证工程/合成/对象归属，并在对象消失、锁定、只读、滚出视区、失焦或取消时关闭。名称和展开点击区预留操作槽，窄视图先缩进让位，不缩小点击容差。新增真实回归 `timeline_mask_header_actions_and_add_menu_preserve_targets_locking_and_undo` 覆盖三种宽度（含 360 浮窗最小宽度）、双语、两种行高、主窗/浮窗、保存撤销、菜单取消与关闭帧输入隔离；已纳入最终 420 项完整测试。实际回归捕获的跨面板菜单关闭问题通过修正事件路由处理，保留原有断言。

## 原生样式验收

截图来自 `PaintList`、真实面板控件、系统字体和已附着的 GPU 渲染器，没有另画菜单示意图。16 个新增场景放在独立的 `--property-smoke`，由 `tools/ci_desktop.py` 分别运行默认样式双语与样式覆盖双语；原有普通 smoke 和 19 面板 smoke 的检查及超时预算保持不变。

`artifacts/property-followups/property-panels-zh.json` / `property-panels-en.json` 引用这些新增截图：

| 名称 | 场景 |
| --- | --- |
| `property-merge-menu` / `property-merge-menu-narrow` | 360 / 220 宽属性右键菜单，显示合并与表达式入口 |
| `property-axis-expression` | 分离 X 轴的内联编辑，其他轴保持原行 |
| `property-mask-group` / `timeline-mask-group` / `timeline-mask-group-narrow` | 蒙版位于变换前，取消几何外壳；时间线图层行直接显示 3D 开关，720 宽下为名称让出混合模式列 |
| `timeline-mask-add-menu` / `timeline-mask-add-menu-narrow` | 900 / 720 宽时间线中的同一行四个蒙版操作和“＋”添加菜单；菜单覆盖后绘制内容，关闭帧不穿透 |
| `layer-lock-menu` / `layer-unlock-menu` | 图层右键“开关”子菜单，两种真实锁状态 |
| `layer-lock-main-menu` | 顶部 Layer 菜单，所选层锁定及全部解锁 |
| `mask-mode-menu` / `mask-mode-menu-narrow` | 七态蒙版下拉、当前项标记和同行反转复选框 |
| `mask-handles-selected` | 合成视图中选中单个节点后，其他节点手柄保持可见 |
| `graph-raw-readonly` | 原始曲线与零帧前只读页脚 |
| `effect-expression-unavailable-menu` | 离散效果参数右键表达式项明确禁用 |

夹具一次性原子添加临时图层与表达式，截图后单次撤销；失败时仍执行清理，并保留最初失败与清理失败原因。每份报告检查 `designCaptureProjectUntouched=true`，PNG 必须存在且比例与目标面板一致。

## 验证记录

统一使用 `.tools/observability-validation-build`、离线锁文件、现有依赖；没有增加第三方资源、依赖或工程格式。

最终验证结果不能以中间编译成功替代：

| 检查 | 记录 |
| --- | --- |
| `cargo check -p motion-desktop --offline --locked -j1` | 通过；包含追加的时间线蒙版控件；`.tools/property-followup-mask-header-check.log` |
| 桌面完整测试，包含 ignored | 最终实现完整通过：420 项通过、0 失败、0 忽略；`.tools/property-followup-mask-header-full-tests.log`，149.62 秒；不修改原有断言 |
| 合并 XYZ 核心、宿主、渲染测试 | 8 + 1 + 1 项通过；`.tools/property-followup-merge.log` |
| UI 完整测试，包含 ignored | 82 项通过；`.tools/property-followup-ui.log` |
| 工具脚本完整测试 | 最终脚本版本完整 54 项通过；`.tools/property-followup-mask-header-python-full.log`，44.373 秒；脚本守卫单独 5 项也通过，见 `.tools/property-followup-mask-header-python.log`；使用干净源码快照，避免把用户未跟踪的 `tools/package_macos.py` 纳入此次变更；复用本地已有 PyYAML 与实际 `effect_tool.exe` |
| 渲染表达式像素/冻结计划 | `.tools/property-followup-render.log`；2 项通过 |
| 蒙版菜单遮挡回归 | 通过；`.tools/property-followup-mask-overlay.log`；绘制顺序检查排除透明焦点边框，防止将边框误判为遮盖菜单的填充 |
| 最终桌面构建 | 通过；`.tools/property-followup-mask-header-build.log` |
| 原生总验收 | 最终完整通过；`.tools/property-followup-native-complete.log`；`tools/ci_desktop.py` 验证双语普通窗口、双语节点、共享 MCP、双语新增属性场景及双语默认/覆盖主题；产物 `artifacts/property-followups/complete/`；保留原有超时预算 |
| 独立双语属性场景与样式覆盖 | 最终默认与覆盖样式、中英文各 16 个新场景全部通过，共 64 张场景截图；检查行高/拖拽阈值/缓动覆盖、工程还原和同姿态主题像素变化；`.tools/property-followup-mask-header-native.log`；产物 `artifacts/property-followups/property-{panels,styled}-{en,zh}.json` |
| 原有 19 面板与 MCP | 最终完整通过；`complete/styled-panels-{en,zh}.json` 验证 19 面板及原有截图/交互检查，`complete/mcp-ui-smoke.json` 验证共享工程、时钟、保存和 GPU 设备；新属性截图没有替代原有验收 |

中间截图夹具曾错误要求显式样式，而默认样式由绘制层提供；修正为复用绘制上下文。夹具撤销后需要同步 Shell 快照版本，现保留工程不变与菜单打开不改版本的双重检查。新增截图最初与普通 smoke 共用 30 秒预算，随后与 19 面板共享预算，均在本机遇到超时；现独立为属性截图阶段，原有检查保留，未增加其超时或跳过检查。

中间完整验收也曾遇到中文普通窗口 40 秒超时及资源紧张时的引擎启动 60 秒超时，保留原始失败日志 `.tools/property-followup-native-rerun.log` / `.tools/property-followup-panels-final.log`。最终使用同一构建、原有时限完成全部阶段；没有终止其他任务的编译进程，也没有将中间报告当作最终通过证据。

用户确认的层级变更需要同步蒙版删键/拖拽、矢量拖拽、属性拖拽、快捷键展开和原生秒表夹具的点击位置，以及旧几何行的滚动补偿；保留原有选择、拖拽、关键帧生成、历史和工程数据断言。属性拖拽夹具现在明确点击 Position 行。新属性场景的主题像素对比使用同一 `--property-smoke` 夹具的默认/覆盖样式；原有 19 面板主题阶段继续使用原有基线，避免把不同工作区姿态造成的差异当作主题变化。

## DESIGN §0.7 基线处置

下表按规范中的历史位置逐条核对。已有整改不算本批新修复；保留表示没有借本任务扩大为全文件视觉重排。

| 历史基线项 | 当前代码 / 本批处置 | 状态与证据 |
| --- | --- | --- |
| path_controls 六处字面量 y 步进 | 蒙版紧凑属性已迁到 `mask_inspector.rs`，使用 row/gap；本批保留行坐标 | 保留现有实现：双语标准/窄蒙版截图、模式提交与菜单层级测试 |
| path_controls 硬编码两列分割 | `mode_row` / `property_row` 已按可用宽度分配模式、反转与数值区；本批没有另设列系统 | 保留现有实现：原有窄面板不重叠测试和 220 宽截图 |
| path_controls 混合按钮高度 | 本批蒙版模式继续使用原生 Dropdown，反转继续 Checkbox，不改变既有点击容差 | 保留：没有全面规范其他路径按钮 |
| path_controls SUNKEN/ACCENT 数值绘制 | 蒙版数字编辑已有 field.number 样式；本批未重写路径数值控件 | 保留：没有声称全文件硬编码已消失 |
| path_controls 全宽布尔按钮 | 当前蒙版已使用 Checkbox；本批没有添加布尔按钮 | 保留现有实现：模式/反转同行截图 |
| effect_controls 30/26/34 步进 | 当前效果已有分组、参数行与动作尺寸 token；本批仅扩展表达式目标判断 | 保留：标准/窄效果控件及样式覆盖截图 |
| effect_controls fx/— 布尔按钮 | 当前实际参数使用 Checkbox；本批未替换为按钮 | 保留现有实现：真实 bool 参数禁用表达式菜单截图 |
| effect_controls 全宽枚举按钮 | 当前实际参数使用 Dropdown；本批不改变枚举值或选择行为 | 保留现有实现：枚举与曲线禁用表达式回归 |
| effect_controls 手动 hover alpha | 旧 `.with_alpha(0.2)` 路径已不存在；本批继续既有样式 | 保留：样式消费者检查与双语原生截图 |
| effect_controls 无层级 | 当前分组标题与参数有 tree.indent；本批不重排树 | 保留现有实现：新增表达式说明位于原参数右键菜单 |
| timeline 11 列硬坐标 | 用户要求新增图层行 3D；名称/3D 分配使用 `timeline.layer.*` 与 icon.slot，窄视图隐藏混合模式列；其他旧列和曲线 plot 几何保留 | 部分调整：真实 3D 点击/保存/撤销回归与标准/窄截图；既有选择、图表框变换测试保留断言 |
| timeline snap guide 硬编码颜色 | 本批没有修改吸附线或 token | 保留：兼容要求，主题整改不属本批范围 |

## 边界

- XYZ 合并仍执行现有保守精确条件：键时间、插值、表达式作用域不能等价表达时拒绝并保留轨道；不做近似烘焙。既有后端契约没有扩大。
- SDK 当前没有资源引用参数类型可供构造真实资源表达式案例；连续数值白名单会禁用新增未知类型，不宣称已实现资源表达式。
- 曲线继续展示作者数据；本批没有新增求值后表达式/约束曲线，也不转换、删除已有负时间键。
- Windows 当前显示环境、主窗/浮窗、两种语言与标准/窄面板可复验；没有声称全部硬件、DPI 档位、macOS/Linux 或持续性能对照已验收。
- 360 宽的新增回归限定于蒙版标题操作、展开命中和菜单输入；时间线历史固定列在极窄布局下的整体重排仍属于 §0.7 保留基线，不据此宣称最小浮窗全部列已经适配。
- 字体缺字及本机失效 Vulkan overlay 注册项仍由现有日志报告；没有把外部系统配置修改算入这五项修复。
- GitHub issue 勾选、PR 发布与合并不属于本文本地结果的证明范围。
