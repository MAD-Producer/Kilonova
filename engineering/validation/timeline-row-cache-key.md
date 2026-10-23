# 时间轴行缓存命中分配（#134 子项）

分支 `codex/timeline-row-cache-key`，起点 main `9de4f3ea`。开始前核对 320 个本地/远程分支引用、115 个工作树以及开放 PR。历史 `pr109`、`pr139` 与 `fix/desktop-input-cancellation` 仍使用原有的提前复制键逻辑；九个无共同祖先的分支没有时间轴源文件，没有发现重复修复。完整审计保存在该工作树 `.tools/timeline-row-cache-key/branch-audit.json`。

## 范围与对照

原时间轴即使命中 `rows_cache`，也先复制 root 和 composition 两个字符串。现在从当前快照借用这两个值与 revision 比较，命中直接克隆已有 Arc；只有失效才重建行并保存拥有所有权的键。缓存仍使用原 TimelineState 字段、同一三元组和同一行投影函数，没有新增状态模型或历史机制。

先把原缓存块移入生产辅助函数而不改变逻辑，再运行同一分配断言：200 行、120 次命中产生 **240 次分配**，结果 2 通过、1 失败、exit 101。原源码、辅助函数和失败日志分别保存为 `original-timeline.rs`、`original-row-cache.rs`、`original-allocation-regression.log`。随后只修改查找逻辑，同一断言为 **0 次分配**，三个测试全部通过；分配器有实际字符串分配 canary，计数限于当前测试线程。没有改断言或放宽预期。

测试直接包含产品实际使用的辅助函数，验证三个身份分量分别失效、旧缓存释放、缺失/类型不符身份沿用空字符串和零、空行缓存、非身份 UI 状态变化不重建，以及行顺序和 Arc 复用。计数只覆盖缓存查询，不包括 JSON 快照生成、查询过滤、布局和绘制，也不宣称整帧零分配或给出未经测量的帧耗时收益。

实际时间轴渲染验收与相邻测试结果见下方最终记录；不以编译成功代替交互验收。

## 观测与功能边界

| ID | 分类 | 处置和证据 |
|---|---|---|
| TRC-HIT | 性能修正 | 同一生产辅助函数、相同 120 次命中，240 → 0 分配；复用相同 Arc，不调用行构造闭包。 |
| TRC-MISS | 正常分支 | root/composition/revision 变化与首次空缓存沿用重建，不增加故障日志或计数；三个分量分别有测试。 |
| TRC-ROWS | 既有覆盖 | 保留 rows 的层顺序、属性投影、解析和既有 observe! 调用；默认身份与空行不是故障。 |
| TRC-FAILURE | 既有边界 | 未增加 Result、吞 panic、重试、超时、线程或重启；构造仍为原同步路径，在成功构造后替换缓存。不把健康测试描述为新的失败信道验收。 |
| TRC-PRESENT | 正常热路径 | 没有新增产品日志、stdout、文件 IO、健康帧 start/done 或告警；测试输出只来自测试进程。 |
| TRC-LIMIT | 待完成 | #134 的其他布局、节点、过滤和绘制成本继续开放；物理窗口、GPU 像素和全 DPI 验收不在本批结果内。 |

## DESIGN §0.7 逐项处置

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md。独立编写的 Rust 缓存逻辑，无新增依赖、资产、兼容名称、颜色、尺寸、样式名、控件、布局或命中区。

| 基线行 | 本批状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径布局。 |
| path_controls 两列硬编码 | retained with reason：未修改列布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件或命中区。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值编辑。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔控件。 |
| effect_controls 行高/步距 | retained with reason：未修改效果列表。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果控件。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，缓存改动不触及坐标。 |
| timeline snap 指示线 | retained with reason：保留原反馈和 token 边界。 |

§12 不作全局合规声明；本批验收限定缓存身份、真实行数据刷新和相同状态下的绘制命令一致性。既有样式、坐标、f64 时间、输入路由和历史调用均未改动。

## 最终记录

缓存测试 `final-allocation-tests.log`：3 通过、0 失败、0 忽略，120 次命中为 0 分配。实际渲染与相邻时间轴套件 `final-timeline-tests.log`：91 通过、0 失败、0 忽略，包含本批一项真实渲染测试，不与既有套件重复相加。En/Zh、900×600/1600×1000 的缓存复用与强制重建得到完全相同的绘制形状、颜色、字形及绘制顺序；真实 Engine 重命名后分别改变 root/composition/revision，实际 render 的行名称全部刷新。相邻测试包含列菜单、3D 开关、时间轴图表、修饰键、键选择/删除、撤销和缩放。产品 cargo check 通过，19 项既有警告（`check.log`）。新文件 rustfmt、差异、样式消费者与来源检查通过。远程 CI 待 PR 最新提交运行，合入前必须检查其结果。
