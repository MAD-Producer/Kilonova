# 变换手柄的最近命中与锚点工具条件（#159 子项）

分支 `codex/transform-nearest-handle`，基于 main `c8504d15`。开始前与中断恢复后分别检查所有工作树 transform_controls.rs 的命中修改、四个拟用 token 及开放 PR；没有重复实现。其他资源预算、CPU 观测和错误原因分支保持独立，根工作树未提交内容保留。

## 行为与证据

原命中逻辑按锚点、旋转、角点、边中点提前返回，重叠时较远的候选会抢走目标；Alt 还会隐式启用锚点。现在在各自原容差内选择距离指针最近的候选，同距保留原稳定顺序。锚点只在锚点工具下参与竞争。光标、悬停绘制和按下捕获复用相同选择器与当前绘制窗口的样式。

四个语义尺寸先写入 dark.json，再接入命中代码：`transform.anchor.hit_radius=12`、`transform.rotate.hit_radius=9`、`transform.corner.hit_radius=8`、`transform.edge.hit_radius=7`。默认数值、严格 `<` 边界和既有绘制尺寸保持；没有缩小命中区。每次最多十个候选，仅保留一个局部距离/句柄，没有候选 Vec、全局缓存或每帧日志。只迁移这四个命中尺寸，不声称整个 transform_controls.rs 的既有绘制尺寸已 token 化。

原始实现上的两个探针实际失败：最近角点被 Rotate 抢走，以及非锚点工具在 Alt 下捕获锚点。失败日志 `original-hit-regression.log`、原源码 `original-transform-controls.rs` 和探针 `original-hit-tests.rs` 保留在 `.tools/transform-nearest-handle/`。这两个探针不是编译失败或成功返回的模拟。

最终相关套件 **13 通过、0 失败、0 忽略**，包括六项新验证和原七项回归：

- 通过真实宿主 Add 创建 4×4 小图层，生产 Pose 投影与 Controls 输入路由在四种缩放/偏移下处理四个角点、Alt 开关，共 32 次正确捕获；初次按下无工程动作，工程保持。
- 重叠旋转/角点/边中点取最近者；同距稳定，锁定、越界和非有限点没有目标。
- 移动/旋转/缩放工具在 Alt 开关下保持正常拾取；锚点工具在两种状态下都可捕获锚点。
- 四个默认容差、严格边界及窗口覆盖值均经实测；实际 Controls 的光标、唯一高亮描边位置和按下句柄一致。
- 原有 2D/3D 投影、视口缩放/平移、不可见手柄、非均匀父级移动、独立缩放、锚点补偿和撤销回归通过。

原有悬停测试中“Alt 启用锚点”的一项预期随本次明确的功能修正改为“锚点工具启用”；其他原有断言保留，新增真实输入路由覆盖移除 Alt 隐式模式的条件。这不是为通过门禁而放宽断言。

首轮扩展高亮探针错误读取填充色数组，11 项通过、1 项失败；实际描边在 Procedural Box.border 中。失败保留，改为检查真实描边槽、唯一高亮数量及目标位置，生产绘制代码没有因此修改。修正观测后的 12 项通过属于增加真实小图层探针之前的阶段，不与最终 13 相加。

最终日志 `.tools/transform-nearest-handle/final-projected-transform-controls.log`。正常产品 cargo check 通过，19 项既有警告；之后仅追加 cfg(test) 的投影探针，生产代码未改。新文件格式、token 默认值、样式消费者及差异检查通过。这里验证实际模型、输入路由和绘制命令，不冒充物理操作系统窗口/DPI 或最终 GPU 像素验收。完整远程 CI 待本 PR 执行。

## 观测与剩余范围

| ID | 分类 | 处置 |
|---|---|---|
| THIT-NEAREST | 功能修正 | 最近距离决定句柄，同距稳定；真实投影与合成重叠场景均覆盖。 |
| THIT-ANCHOR | 功能修正 | 移除 Alt 隐式锚点模式，保留显式工具；README 给出操作方式。其余缩放修饰键未改变。 |
| THIT-MISS | 正常分支 | 无候选、锁定、边界外和无效坐标返回无命中，不增加错误记录、计数或重试。 |
| THIT-STYLE | 样式来源 | 四个 token 保留旧默认容差，读当前 PaintList 样式，窗口覆盖实测生效。 |
| THIT-HOST | 既有边界 | 不改宿主协议、历史事务、worker、in-flight、panic 或日志。既有真实编辑/撤销测试通过，不把正常 miss 当作故障。 |
| THIT-ACCEPTANCE | 验证范围 | 没有新增故障观测项，不冒充重新完成默认日志信道或物理窗口验收。#159 的缩放修饰键、移动/缩放吸附等剩余工作继续开放。 |

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md。没有新依赖、第三方素材、兼容名称、颜色、控件行、绘制尺寸或布局变化。

| 基线行 | 处置 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：未修改布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件或命中区。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值字段。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔控件。 |
| effect_controls 行高/步距 | retained with reason：未修改效果行。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果控件。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改效果绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，未修改时间轴布局。 |
| timeline snap 指示线 | retained with reason：未修改时间轴主题或拖动反馈。 |

交互验证如上，不把编译、绘制命令或十二项处置扩大为整个视觉基线整改完成。
