# 节点视口与位置提交的同帧规格复用（#134 增量）

基于 #602 的 `68e1415ea919f8215acc03801d0dbb2312f957c8`，独立分支 `perf/node-viewport-geometry`。开始前核对其他开放分支：#597、#599 的分页完整文件清单没有 `node_editor` 或 `panels/mod.rs`；没有修改根工作树或审查他人 PR 代码。

## 行为与边界

滚轮平移/缩放、中键平移、小地图导航与节点首次拖动，只变化世界位置或视口投影，不需要再次解析同帧端口规格。正常松开节点会增加非语义 revision，因此仅在原有节点拖动正常左键释放、没有语义变化、epoch/preview_revision 一致时允许投影复用。每条复用路径仍先验证全体节点的数量、顺序、ID 与指针身份；任一失配在更新任何卡片前拒绝复用，执行原重建。

只比较指针身份，从不解引用；卡片不跨 draw 存活。节点结构变化、父图导航和取消后替换文档仍重建。没有更改输入、提交、撤销、重试、日志、颜色、尺寸、命中容差或绘制顺序。

## 验证进度

- 原生产路径下，两条新计数回归实际 exit 101，均为构建两次而非一次。原程序完整记录器成功完成 24 种配置、312 帧，包括中英文、两种区域、三种缩放、折叠/展开；24 个移动对照均改变实际绘制与状态。
- 改进后，两条计数回归与同一个独立进程记录器通过。原/新 312 帧完整 shapes/colors/glyphs/order、响应、EditorSnapshot 与 undo 长度逐项精确相等，没有删除或归一化字段。
- 原有两条 `>= 2` 计数断言在生产优化后实际失败；在上述行为对照成立后，将它们收紧为 `== 1`，并更新导航测试名。没有弱化或删除几何、文档、历史、取消或父图导航断言。身份恢复的新测试明确要求重建两次。
- 最终程序通过 **212 项不同的定向回归**：节点编辑器 186、布局缓存 4、面板 13、Shell 输入事务 7、真实工作区计数 2；0 failed、0 ignored。独立重建几何、结构失效、取消、撤销及既有故障回归均包含在内。独立三条新增回归复跑与 186 项套件重复，不另算新增数量。
- 真实 Shell 稳定拖动的 48 个样本保持静止/拖动绘制数量一致；五种实际工作区预设、中英文、尺寸变化、最大化/恢复与 active 标签切换的 48 次变化均求解一次、随后稳定帧零次，并与原始几何一致。这些是已有工作区断言在本次最终源码上的复跑，不是新增原生点击或 GPU 验收。
- 带 FFmpeg 的测试程序与正式 dev 程序编译通过。正式程序 SHA-256 `06842125ff02d769a9d5b281377cb52c89cc7d5a46459057a019825899e7e6cd`；使用独立 LOCALAPPDATA，`--command-list` exit 0、stdout 为完整 JSON、stderr 为空，运行时没有覆盖 RUST_MIN_STACK。该检查不证明原默认配置目录权限问题已修复，也不证明窗口/GPU 呈现。
- [证据清单](node-viewport-geometry-evidence/manifest.json)逐项记录源码、原始记录与压缩包 SHA-256；[完整证据包](node-viewport-geometry-evidence/evidence.zip)保留原路径失败、旧计数要求失败、最终回归、完整绘制对照、编译输出与精确命令。源码快照为 `4f816a2d3655e860a61daeb1ab4aade40efed0d8`，后续证据提交不改变这些生产/测试源文件。精确提交 CI 及整个 #134 的剩余范围继续开放。

## 错误、设计与来源

| ID | 分类 | 处置与限制 |
| --- | --- | --- |
| NV-PROJECT | 性能修复 | 端口元数据不变时重投影已有卡片和插口；上述完整新旧绘制与状态对照通过。 |
| NV-REJECT | 正常分支 | 身份失配、结构/父图变化仍执行原构建；不是错误，不增加健康帧日志。 |
| NV-OBSERVE | 既有覆盖 | 每帧首次 cards 保留原 `observed("canvas.sockets", ...)`。不吞 Err/panic、不改变错误信道；移除的是重复解析及其重复观察。 |
| NV-HISTORY | 功能保持 | 正常释放仍一次提交，撤销恢复原工程；丢失按键取消仍恢复原文档并拒绝失配身份复用。 |
| NV-PENDING | 待验证 | 全部 #134 的结构变化帧、同帧布局求解上限、原生 Present/GPU、整帧耗时/内存峰值及最终集成仍开放。 |

已核对 PROVENANCE.md、ATTRIBUTION.md、NOTICE；无新增依赖、第三方资产或兼容名称。DESIGN §0.7 每项处置如下，retained 表示本批保留，不把历史描述直接认定为当前缺陷或已修复：

| 基线位置 | 本批处置 |
| --- | --- |
| path_controls 六处 y 步进 | retained with reason：未编辑属性行。 |
| path_controls 双列宽 | retained with reason：未编辑列布局。 |
| path_controls 混合按钮高度 | retained with reason：未编辑按钮或命中区域。 |
| path_controls 自绘数值背景 | retained with reason：未编辑数值字段。 |
| path_controls 全宽布尔按钮 | retained with reason：未编辑布尔控件。 |
| effect_controls 行高与步进 | retained with reason：未编辑效果属性行。 |
| effect_controls 布尔文本按钮 | retained with reason：未编辑效果开关。 |
| effect_controls 全宽枚举按钮 | retained with reason：未编辑选择控件。 |
| effect_controls 悬停透明度 | retained with reason：未编辑悬停绘制。 |
| effect_controls 缩进/层级 | retained with reason：未编辑效果树。 |
| timeline 列坐标 | retained with reason：保留原列布局及行为。 |
| timeline 吸附颜色 | retained with reason：未改颜色或 token。 |

§12 的物理输入、DPI 与全窗口视觉整改 pending verification；本次无可见样式变更，312 帧逐字段等价是计算输出证据，不称为全部视觉验收。
