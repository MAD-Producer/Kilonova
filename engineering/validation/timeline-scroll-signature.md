# 时间轴滚动缓存的集合边界与表达式签名（#134 子项）

分支 `codex/timeline-scroll-signature`，起点 main `00627cdb`。开始前核对开放 PR、328 个分支引用和 122 个工作树；追加 329 个引用的集合边界检查，没有现有长度分隔修复或相同工作树改动。两个审计分别在工作树 `.tools/timeline-scroll-signature/branch-audit.json`、`group-boundary-audit.json`。#449 的行缓存身份查找是另一处调用，当前分支不重复实现。

## 两项独立改动

原 scroll_extent_signature 连续写入不同集合的 ID，却没有为 expanded_effects、collapsed_effect_items、collapsed_audio、vector_open 写入长度。不同组的元素可形成同一字节流，导致行布局改变后仍命中原滚动上限。现在补齐四个集合的长度边界；原有其他输入、行布局与滚动计算保持。

原表达式行签名每次 format! Debug 文本，再散列整个临时字符串。先把原式机械移入生产辅助函数，保持原字节流和行为，取得失败对照；随后改为目标数量、变体、object、property/axis 或 effect/param 的直接散列。匹配所有字段与变体，不增加公开模型 trait、序列化字段、持久状态、依赖或缓存容量。签名只在进程内使用，不承诺跨版本数值或持久格式。

## 原始失败证据

最初复现夹具使用 `source` 作为合成 ID，被核心 ID 校验拒绝；`first-composition-fixture.log` 保留，此次失败不算缓存缺陷证据。改用合法 `comp-source` 后，真实 Engine 接受嵌套合成与效果/音频行，原缓存实现实际复现：

- 英文 900×180，初始上限 **298**。切换效果展开/音频折叠后签名完全相同，缓存仍给出 **298**，强制重新计算为 **12**，原精确一致断言失败，exit 101（`original-group-boundary-regression.log`）。
- 表达式签名生产辅助函数的 120 次调用，空行/属性/效果分别 **120、480、480 次分配**，期望零分配的断言失败。身份/顺序测试通过，共 1 通过、1 失败、exit 101（`original-allocation-regression.log`）。当前线程分配器有真实 String 分配 canary，原 helper 和 timeline 源码另存同目录。

没有把编译错误、构造失败或放宽断言作为产品修复证据。

## 修复后的验证

实际 timeline::render 对合法工程执行 En/Zh、900/1600 宽、180/240 高面板区域、两个方向切换，共 16 组。每组的缓存上限与清空缓存后的新投影精确一致，签名区分组，滚动位置不超出新范围，模型不变且没有工程动作。例如 900×180 正向 **298 → 12**，反向 **12 → 298**；900×240 为 **238 ↔ 0**。

两个生产签名单元测试覆盖扁平 ID 流相同但归属不同的集合、十种行结构输入，以及工程/合成/revision、查询、视口高度、行高的失效；相同输入重复计算保持同一签名。独立集成测试直接包含产品辅助函数，覆盖表达式变体、对象、属性、None/X/Y/Z、效果、空/Unicode/内嵌 NUL 参数和行顺序。

`final-allocation-tests.log`：2 通过、0 失败、0 忽略，同样 120 次签名的三类分配均为 **0**。这是表达式签名子路径的结果，不是整帧零分配或帧耗时收益声明。初验 `final-scroll-tests.log` 7 项通过，含三个新增 UI/签名测试；其与后续相邻套件重叠，不重复相加。最终 `final-timeline-tests.log` 为 92 通过，`final-effect-tests.log` 为 9 通过，二者均 0 失败、0 忽略；7 项初验已包含在这些相邻套件中。产品 cargo check 通过，19 项既有警告（`check.log`）。来源、样式消费者、新文件格式和差异检查通过；远程 CI 待最新 PR 提交运行。

真实渲染调用、缓存范围和行结构是本批交互证据；没有把它描述成物理窗口、TSF、全 DPI 或最终 GPU 像素验收。

## 观测和功能边界

| ID | 分类 | 处置与证据 |
|---|---|---|
| TSS-BOUNDARY | 功能修正 | 四个集合补齐长度分隔，避免错误复用滚动范围；实际原 298/12 失败与 16 组双向切换验证。 |
| TSS-ALLOC | 性能修正 | 生产辅助函数直接散列完整表达式身份，三类 120 次调用从 120/480/480 降至 0/0/0 分配。 |
| TSS-INVALIDATE | 既有覆盖 | 保留全部原失效输入和计算公式；视口、行高、查询、层级、过滤与表达式身份有直接测试。 |
| TSS-NORMAL | 正常分支 | 缓存首次建立、失效、空表达式行不是故障，不添加日志、计数或重试。 |
| TSS-ERROR | 既有边界 | 不改模型校验、命令、历史、返回值、Err/panic 传播、worker 或超时；原行投影中的观测保留。 |
| TSS-PRESENT | 正常热路径 | 没有新产品 stdout、文件 IO、健康帧日志或故障结构，不把健康缓存测试扩大成新的默认 Error stderr 验收。 |
| TSS-REMAINING | 待完成 | #134 的其余布局、绘制和过滤工作继续开放；不宣称哈希完全无碰撞或所有 UI 路径已零分配。 |

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md；全部为独立编写的缓存逻辑和测试，没有新资产、依赖、兼容名、颜色、尺寸、样式名、控件或命中容差。测试使用既有节点示例，未引入第三方素材。

| §0.7 基线行 | 本批状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：未修改列布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件与命中区。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值编辑。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔交互。 |
| effect_controls 行高/步距 | retained with reason：未修改效果控件。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果开关。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：保留既有行结构。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，修正缓存复用。 |
| timeline snap 指示线 | retained with reason：保留既有拖动反馈。 |

§12 限定滚动范围正确性、表达式键和相邻交互回归，不声明所有 19 面板、全部工作区、平台与 DPI 的视觉整改已完成。最新 PR 提交的必需 CI 全部通过后才合并。
