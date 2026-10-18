# 窄时间轴列冲突及属性 PR 主线整合

基线：PR #329 `d30a05d`；修复提交 `5ee05d6`；整合已合入的主线锚点动画 `a801e54`。使用隔离工作树 `E:/Dev/aem/.worktrees/review-property-followups`，本地分支 `fix/timeline-switch-columns`，最终更新已有 PR #329。根工作树和用户未跟踪文件不作修改。

## 功能与证据

| ID | 分类 | 实现和证据 |
| --- | --- | --- |
| COL-01 | 已复现缺陷 | 360px 时间轴的 gutter 为 187.2px。旧算法将名称压为零宽后仍在 112..132 放 3D 开关，父级单元格为 93.2..179.2；一次点击既发 SetLayer3d 又打开父级菜单。`artifacts/timeline-switch-columns/before.log` 的真实 Engine/浮窗回归失败于菜单不应打开的断言。 |
| COL-02 | 布局/输入修复 | 名称、3D、混合列及父级列共用列计划。隐藏混合列后仍不足 `value.min_width` 时，父级标题、图标、单元格和命中区一起隐藏并让出名称分配；属性面板的父级入口保留。3D 使用完整 icon.slot；只有连该槽也无法容纳时才同时隐藏其标题和控件。列隐藏时通过既有 seen/overlay 规则取消下拉，不发提交。没有缩小命中宽度，也没有增加行。 |
| COL-03 | 真实回归 | 原点击回归保持断言并通过。新增 360/500/720/1280 宽、22/32 行高、双语、主窗/浮窗组合，验证名称/开关分离、仅一次正确 SetLayer3d、真实保存重开、撤销/重做、锁定/只读，以及缩窄后父级菜单无提交地关闭。`after-routing.log` 通过。 |
| INT-01 | 主线整合 | properties.rs 保留共享属性行计划，锚点复用真正的 Anchor 轨道和 AnchorAt 编辑，不再保留重复静态行。timeline_layout.rs 保留蒙版先于变换，并由属性计划分配锚点。design_acceptance.rs 同时保留锚点场景的效果过滤及离散参数表达式场景过滤。 |
| INT-02 | 夹具更新 | 整合后首次完整测试 434 passed / 1 failed / 1 ignored，`full-integrated-tests.log`。唯一失败是主线锚点测试按旧 inspector 坐标点击；蒙版组现已先于变换。只更新 Properties 的点击 y，保留 Timeline 坐标及所有动画、AnchorAt、取消和数据恢复断言，未降低断言。 |
| FIXTURE-01 | 夹具边界 | 首版扩展测试直接修改快照的 locked 而不更新 revision，未遵守行缓存的版本契约，导致锁定断言失败（`after-focused.log`）。改为通过真实 Engine Flags 命令锁定、真实 undo 恢复；原拒绝编辑断言保留，`after-routing.log` 验证通过。没有把夹具失败算作生产缺陷。 |
| OBS-01 | 正常分支 | 宽度不足、隐藏列、取消菜单、无操作和只读不是故障，不增加逐帧日志或重试。没有新增 worker、信道、超时或重启。 |
| OBS-02 | 既有覆盖 | SetLayer3d/Flags、保存及撤销沿用既有引擎错误信道。完整测试保留默认 Info 的真实错误文件、Error stderr、stdout 隔离及 in-flight/阻塞回归；截图夹具继续保留主失败和清理失败。 |
| OBS-03 | 待验证边界 | 新截图写入与清理双失败未另行注入；未声称修复所有可观测性缺口。 |

## 原生验收与构建

新增实际 GPU 场景 `timeline-layer-columns`（720×520）和 `timeline-layer-columns-minimum`（360×520），纳入原有独立 property smoke 的中英文和覆盖主题检查。使用真实面板、系统字体和 GPU 捕获；没有另画控件或菜单。没有改变右键菜单。

- 开发构建通过，2m28s，22 条既有警告；原生总 smoke 在仍持续生成场景时超过既有 40 秒预算，记录于 `native.log`。未放宽任何预算，改用优化构建复核。
- 本地优化构建仅为 motion-desktop 设置 16 个 codegen units，以缩短迭代编译；opt-level 和 thin LTO 保持 release 设置，仓库/CI 编译策略不变。命令和覆盖记录在 `release-build.log` 与 `release/build.json`。不是新运行模式或失败重试策略。
- 修正点击坐标后的完整桌面测试通过：435 passed / 0 failed / 1 ignored，205.93 秒，`full-final-tests.log`。
- 主线整合后的核心回归通过：Anchor 动画 4 项、维度合并 8 项；宿主维度合并 2 项通过（含实际命令和拒绝后状态）。真实 GPU 表达式 2 项与维度合并 1 项通过，分别比较预览像素、冻结输出计划及独立显式值参考，覆盖分数时间、图层偏移、分离轴和效果实例目标。日志为 `core-integration.log`、`host-integration.log`、`render-integration.log`。
- 工具测试首次在默认 Python 下因缺少 CI 固定依赖 PyYAML 而有两项导入错误，`tool-tests.log`。切换已有 `ui-pr-validation-env`（PyYAML 6.0.3）后完整通过：54 项，1 项跳过，38.84 秒，`tool-tests-final.log`；没有修改断言或项目依赖。
- 最终带 FFmpeg 优化构建通过，15m57s、22 条既有警告。整合后的完整原生验收通过，`release-native.log`：中英文普通窗口、实时节点、共享 MCP、默认/覆盖主题属性截图、覆盖主题浮窗；沿用原有 40/90/125 秒预算。
- 目视检查 `release/screens/property-panels-zh.timeline-layer-columns{,-minimum}.png` 和 `smoke-zh.layer-anchor-animation.png`：720/360 宽的名称与 3D 开关不重叠，360 宽不再绘制父级列；锚点以真实可动画属性行出现。360 宽的历史顶部播放/缩放工具仍拥挤，此次未修改，不据列修复宣称整个窄面板视觉合规。这组截图尚不含独立 PR #330 的底部滚动条。

## PR #327 合并后的主线整合

主线 `8d6698a` 已包含 #327 的入出点快捷键、列显隐及悬停反馈。整合时解决时间轴与路径绘制冲突，保留已有功能及断言：列显隐偏好与 3D 开关使用同一 `ColumnRects::styled` 计划；路径选中节点按真实索引定位，并同时保留淡显和手柄悬停。补入已在 #330 组合版本验证过的移位原点、单个手柄悬停与锁定/只读保护。没有引入 #330 的滚动条或边缘滚动。

整合后的完整桌面测试为 451 passed / 0 failed / 1 ignored，199.04 秒，`artifacts/timeline-switch-columns/main327-integration-tests.log`；工具测试 54 项、1 项跳过，42.22 秒，`main327-tool-tests.log`。原生构建及远端 CI 待该整合提交验证，前述 435 项和截图记录仍仅证明其原提交。

原提交 `9d606f2` 的 Linux Rust/GPU CI 在 `particle_emitter` 的普通表达式采样处返回执行预算超限；没有验证到本批代码缺陷。本地以原预算和原断言运行同一 11 项粒子测试全部通过，19.32 秒，`particle-ci-recheck.log`。请求单独重跑时 GitHub 因同轮 macOS 任务尚在运行而拒绝，未启动重跑。本次不放宽预算、不改断言、不在业务代码增加重试；新整合提交的 CI 继续验证此路径。

## DESIGN §0.7 逐项处置

| 历史基线 | 处置 |
| --- | --- |
| Path 六处纵向步长 | retained：本批不改属性布局。 |
| Path 两列分割 | retained：本批不改数值列。 |
| Path 混合按钮高度 | retained：不改字段尺寸和命中容差。 |
| Path 数值背景 | retained：不改。 |
| Path 全宽布尔按钮 | retained：不改。 |
| Effect 行高与步进 | retained：不改效果面板。 |
| Effect 布尔按钮 | retained：不改。 |
| Effect 枚举按钮 | retained：不改。 |
| Effect hover alpha | retained：不改。 |
| Effect 层级 | retained：不改。 |
| Timeline 固定列坐标 | retained with reason：保留可见列的既有坐标；新增紧凑视图的整列显隐规则已写入 §7.1，并以真实输入验证，不以缩小命中区换取密度。 |
| Timeline 吸附线颜色 | retained：不改吸附反馈。 |

本批没有新增资产、依赖、颜色或尺寸 token；仅复用已有语义 token。多 DPI 和其他平台仍由远端 CI/后续实际设备验证，不以本地截图宣称全面视觉合规。目标 issue 保持原范围：#209、#219、#235、#245、#262；合并及关单状态以 GitHub 为准。
