# 路径点选择与批量编辑验收

对应 #234。分支 `codex/path-node-selection` 从 main 的 `8bf0874` 开始；先核对开放 PR 和本地 worktree。#349、#362 涉及同一个 path_controls.rs，但修改的是属性筛选/展开；本批不修改那些属性行。路径选择实现位于新增 point_selection 模块，复用现有输入路由、EditBatch、History 与持久数据结构。shell/mod.rs 的测试模块登记与其他 PR 共用文件，集成时保留其他模块。节点编辑器与 UI 编辑器组合生成效果继续延期。

## 实际行为

- 选择键包含蒙版、几何项、路径、节点的稳定 ID；不同蒙版/几何项允许有相同内部节点 ID，不会串改。选择集合是临时 UI 状态，按工程/合成/图层/工具领域隔离，不进入项目文件和历史。
- Shift/Ctrl 点选追加或切换；空白 Ctrl 框选替换、Shift 框选追加、Ctrl+Shift 框选切换。取消框选恢复之前的集合。
- 拖动一个已选锚点移动整组；拖动曲线路径段选择其两端并整体平移。开放路径不虚构闭合边，空白普通点击继续原钢笔绘制。矩形等参数几何仍由原宽高控件处理。
- 批量拖动保留相对手柄、每个几何项的坐标变换和原动画轨道；蒙版按整份姿态合并后提交，避免多个节点互相覆盖。批量删除保留其余路径、组、绘制项、重复器及动画。
- 原 7px 命中半径保持；新增选中方框和框选透明度先写 dark.json，再引用。颜色复用 focus/text，边框复用 border。曲线拾取是固定 32 段的几何近似，不受样式覆盖影响；选择清理使用有界源节点集合查找，避免每帧逐选中点再次扫描整条路径。
- 主窗和原生浮窗沿用同一面板入口；路径焦点接收删除/撤销，文本输入与 IME 不被路径删除抢占。延迟松键使用 release 自带坐标，后续鼠标移动不能继续拖动节点。

## 错误与正常分支审计

| ID | 分类 | 证据与边界 |
|---|---|---|
| POINT-MOVE | 新增功能/原协议 | 使用原 EditBatch 和单次 History(2/3/4)，不增加 worker、重启、重试或项目格式。真实引擎验证成功、一次撤销、保存重开；第二条命令失败时整批不发布。 |
| POINT-PROJECTION | 新增观测/降级 | 投影不可用、反投影失败、几何越界及选择目标消失带 object/phase/selected/reason；连续退化第一条 Warn，60 次 Error，由原 logger 写文件和 stderr。计数属于此 Selection 实例，同一有效移动/正常无变化及成功准备/删除重置；不把次数等同秒数。持续失败抑制与有效输入后恢复重置由实际失败指针验证。 |
| POINT-NOOP | 正常分支 | 没有节点命中、空选择、框选空白、未超过拖动阈值、无变化输入不产生编辑或故障。 |
| POINT-CANCEL | 正常/功能边界 | Esc、失焦、只读、投影失效、工程/合成/帧变化终止活动手势；已经开始的事务恰好发送一次 History(4)。取消不作为故障重试。投影失效另留原始原因。 |
| POINT-LIMIT | 功能边界 | 某一选中点无法生成有效几何时整批预览不提交，不能只移动部分点；有效移动可以恢复。延续原拖动协议，松键结束手势，越界期间保留最后一次有效预览。 |
| POINT-ENGINE | 既有覆盖 | 引擎命令的 start/in-flight、耗时、Err/panic/file/stderr/stdout 分流仍由既有通道处理；本批不修改 Present 热路径或吞掉回复。 |
| POINT-LOG | 新增失败验收 | 真实无效屏幕坐标循环：59 次无 Error，第 60 次一次；继续 80 次不重复；有效无变化输入恢复后再次 60 次才发第二次 Error。默认 MOTION_LOG 未设置；文件为 INFO，stderr 同时两条 Error，stdout 不含产品日志。 |

## 本地证据与进度

原生验收首次通过：`shell::path_selection_acceptance::point_selection_main_and_native_floating_routes_capture_and_restore`，32.21 秒。为中英文各创建主窗及独立原生浮窗 GPU surface，走真实 chrome/floating_chrome 输入，再通过引擎呈现与截图；共 8 张 1280×800 PNG，已解码并检查非空。中文主窗框选与浮窗移动图已实际查看。这个测试直接交付鼠标/键盘 Input 到真实路由，不声称验证操作系统鼠标硬件或所有 DPI。

截图：`artifacts/path-node-selection/native/{zh,en}-{main,floating}-{marquee,moved}.png`。默认日志及 stderr/stdout 在同目录。环境仍报外部 Epic Vulkan overlay JSON 缺失，未将其归因于本次路径实现。

默认并发的完整桌面测试结果：629 通过、2 失败、1 项既有忽略，197.14 秒。全部 11 项新增路径用例与原生路由截图通过；失败的是既有 layer_actions 双击重命名与 effect_controls/library 双击应用测试。两者依赖墙钟双击窗口，不能把本轮记为全绿；保留完整日志 path-node-selection-desktop-final.log，并保持断言不变，以两个测试线程复验完整套件。

最终完整复验使用 `--test-threads=2`：631 通过、0 失败、1 项既有忽略，396.83 秒；上述两项旧双击测试也通过。日志为 `artifacts/pr-queue/path-node-selection-desktop-bounded.log`。这证明本次低并发完整复验通过，不宣称已修复旧双击测试的墙钟耦合。原生用例同时重新生成最终 8 张截图，路径为 `artifacts/path-node-selection/final-native/{zh,en}-{main,floating}-{marquee,moved}.png`。

另独立执行真实失败子进程（未设置 MOTION_LOG），0.36 秒、退出码 0：INFO 文件初始化可见，文件与 stderr 各恰好两条 path.point_selection Error，stdout 无产品日志；保留 `artifacts/path-node-selection/failure-final/{result.json,motion-studio.log,stdout.log,stderr.log}`。样式消费者检查、provenance 1543 个跟踪路径、暂存 diff 空白检查与 dark.json 解析通过。未增加依赖或第三方资产。

早期失败日志保留：首次编译误用了 Rust 2024 的 let-chain（仓库为 2021，已改正）；第一轮新夹具把开放路径的正常 Seek 当作异常；随后分组夹具缺少项目版本 20，均修正新测试数据/预期，未删改既有断言。C 盘曾满，导致 Cargo 缓存访问警告，测试临时目录改在 E 盘，不删除其他分支的工作和缓存。

## DESIGN §0.7 基线处置

本批修改查看器路径交互，不重绘属性行。以下均 retained with reason；不把其他 PR 已有的属性控件当作本批整改，也不声称整个 DESIGN 已验收。

| 基线项 | 当前核对/保留理由 |
|---|---|
| path_controls 六种步距 | 属性部分已分到 mask_inspector/groups，当前行布局使用 row token；本批不改属性步距，完整视觉整改待独立复验。 |
| path_controls 两列硬编码 | 当前复用 mask_inspector::property_row；本批只改 viewport，不改列布局。 |
| path_controls 按钮/字段高度 | 保留原控件及命中，不靠缩小控件实现节点选择。 |
| path_controls 自绘数值底色 | 当前 mask_inspector 已使用 field.number；不修改数值编辑和背景。 |
| path_controls 布尔宽按钮 | 当前 mask_inspector 使用 Checkbox；不修改开关布局或状态。 |
| effect_controls 行高/步距 | 当前使用 effect/row token；该文件不改。 |
| effect_controls 布尔按钮 | 当前使用 Checkbox；该文件不改。 |
| effect_controls 枚举按钮 | 当前使用 Dropdown；该文件不改。 |
| effect_controls hover alpha | 当前可见项复用 Hovered 状态；该文件不改，不新增 hover 颜色。 |
| effect_controls 缺少 depth | 当前存在 tree.indent；该文件不改，不宣称覆盖所有效果树层级。 |
| timeline 11 列坐标 | 按兼容基线保留，不改时间轴列。 |
| timeline snap guide | 按兼容基线保留，不改吸附颜色。 |

§12 本批相关的命中、主窗/原生浮窗、zh/en、事务取消、撤销与持久化由上述实际路由/引擎用例覆盖。19 个面板全部状态、900×600 及全套 DPI/辅助功能/跨平台 GPU 人工检查未在本次重新验收，远端 CI 结果待补。
