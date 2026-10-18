# 时间轴列坐标与路径悬停回归修复

基于 PR #327 的 `1f53e583ed09899252d1d0da80218507f53180e4`，分支 `fix/desktop-interaction-regressions`。
对应原 PR 的 #254（列显隐）和 #271（手柄反馈）；入出点快捷键保持原行为。

## 缺陷、实现与证据

| ID | 已复现缺陷 | 修复与验证 |
|---|---|---|
| COL-01 | `ColumnRects` 把 gutter 宽度当作绝对右边缘，面板原点从 0 移到 350 后名称宽度由 226 缩到 1，其他列未平移 | 可选列和无列时的右边缘均加 origin。原点回归验证名称宽度及所有可选列的平移量，显隐、菜单命中与保存回归继续通过 |
| PATH-01 | 悬停目标只保存 bool，无法区分入/出手柄，单侧悬停画出两个高亮 | 悬停目标保留几何分量 2/4；绘制仅强调同一个分量。回归保持“只能一个高亮”的原断言；不改变手柄选取半径和拖动命令 |
| PATH-02 | 锁定层可被悬停检测选中，显示实际无法抓取的高亮 | 生成悬停目标前检查真实模型锁定及观察状态；未选对象、视口外和拖动中均为正常不高亮分支。锁定真实 VectorContent 工程回归验证没有可编辑高亮 |

本批没有新增依赖、第三方资产、面板颜色、尺寸或样式名称；不改变工程格式、关键帧、曲线插值或输入命令。

## 验证

- 原审查中三个回归均失败，记录 `artifacts/layer-anchor-animation/pr327-review-tests-final.log`；修复后相同核心断言全部通过，`artifacts/desktop-interaction-regressions/regressions-final.log`。
- 带 FFmpeg 的完整桌面测试：433 passed / 0 failed / 1 ignored；首次 38.32 秒，补充入/出手柄位置及独立只读分支后再次完整通过，38.63 秒，`desktop-tests-latest.log`。不修改既有功能断言；原有 HoverTarget 期望仅随字段由 bool 改为方向值调整表示。
- 样式消费者检查和 `git diff --check` 通过。
- 首次验证复用了其他 worktree 的旧 motion-ui 构建，出现本分支源码实际已包含的 Key 变体缺失。确认源码后刷新该文件时间戳，重编 motion-ui，测试通过；没有添加重复变体或改动接口来迎合缓存。
- 首轮 release 暴露截图夹具调用了仅测试可用的 `draw`，已改用生产入口 `draw_in_viewport`；同时刷新所有本地 Rust 源码时间戳，排除其他分支的 Anchor 类型构建产物。随后 release 构建通过（11 分 17 秒，22 条既有警告）。复核中发现截图需按原生像素比例渲染后裁剪，已修正，最终构建与验收以此后的记录为准。
- 最新优化构建通过；完整原生验收通过（`native-acceptance.log`），中英文、浮动样式、节点实时渲染与共享 MCP 均通过。已目视核对中文入/出手柄及移位时间轴截图。四个强制截图场景为 `path-hover-incoming`、`path-hover-outgoing`、`path-hover-locked`、`timeline-translated`，直接使用真实 Engine、路径控件和 GPU capture，不另画示意控件。

## 错误处理与可观测性

| ID | 分类 | 处置 |
|---|---|---|
| OBS-01 | 正常分支 | 锁定、只读、未选对象及未命中不是故障；不新增警告、错误重试或计数器。 |
| OBS-02 | 已有覆盖 | 数据提交、GPU 截图和历史操作继续使用已有 Engine 请求通道及日志；完整桌面测试包含既有默认日志、stderr 和 stdout 隔离验证。没有改变错误返回。 |
| OBS-03 | 新增验收边界 | 截图使用可取消事务，恢复 Panels 和工作区。正常退出验证工程完全还原。截图与回滚同时失败保留主错误并记录清理原因；该双失败分支未故障注入，不声称已验证。 |
| OBS-04 | 功能边界 | 本批改的是坐标和交互反馈，不是全局可观测性整改。没有新增生产 worker、线程、超时或自动恢复。 |

## DESIGN §0.7 逐项处置

| 历史基线项 | 本批处置 |
|---|---|
| Path 六处字面量纵向步长 | retained：未改属性布局。 |
| Path 两列分割 | retained：未改数值列。 |
| Path 混合按钮高度 | retained：未改控件高度和命中容差。 |
| Path 自绘数值背景 | retained：未改数字字段。 |
| Path 全宽布尔按钮 | retained：未改蒙版布尔控件。 |
| Effect 行高与步进 | retained：未改效果面板。 |
| Effect 布尔按钮 | retained：未改效果面板。 |
| Effect 枚举按钮 | retained：未改效果面板。 |
| Effect hover alpha | retained：未改效果面板。 |
| Effect 层级 | retained：未改效果面板。 |
| Timeline 固定列坐标 | fixed COL-01 的面板原点混用；保留原列宽、间距和显隐语义，未宣称极窄宽度所有列已适配。真实移位截图已目视验证。 |
| Timeline 吸附线颜色 | retained：未改吸附反馈。 |

多 DPI、其他平台和截图清理双失败仍待验证；本地测试不替代远端 CI。
