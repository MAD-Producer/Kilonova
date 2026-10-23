# 变换缩放的比例修饰键（#159 子项）

分支 `codex/transform-scale-modifiers`，基于 #443 的 `4ddb6e7e`。开始前核对所有工作树中缩放修饰键的未提交差异及开放 PR，没有重复实现。#443 处理命中与 Alt 隐式锚点，当前分支只处理缩放数学与修饰键；先依赖其实现，待其合入后更新到 main 并重新验证 CI，不把本分支提前合入父分支。

## 行为和证据

原角点默认等比，但 Ctrl 会变成分轴、Alt 不释放比例；边中点不能用 Shift 等比。现在默认角点仍等比，Alt 分轴自由；边中点默认只改对应轴，Shift 等比，同时按 Alt 则释放约束。Ctrl 不改变这些模式。3D 等比缩放同步 XYZ，2D 保留 Z；比例始终从手势起点求得，不把中途的预览值再次相乘。

默认角点的点积比例算法保留；Shift 边中点从所控制轴求有符号比例，左右/上下边均使用原有零分母保护。继续使用既有 vector 校验、工程命令、手势历史、取消和投影失败观测；没有新日志、线程、重试、错误结构、布局、尺寸、颜色、命中容差、依赖或资产。README 记录实际模式。

修改前的三个真实宿主探针均失败，exit 101：Alt 角点实际 `[160,160,100]` 而非 `[150,200,100]`，Ctrl 角点实际 `[150,200,100]` 而非 `[160,160,100]`，Shift 边中点实际 `[150,100,100]` 而非 `[150,150,100]`（原日志保留浮点原值）。证据在 `.tools/transform-scale-modifiers/original-scale-regression.log`，原源码和探针另存同目录。不是成功返回的模拟或编译失败；既有断言没有放宽。

最终变换套件 **19 通过、0 失败、0 忽略**：父分支的 13 项加本批六项，不与单独父分支结果相加。新验证包含：

- 2D/3D、四个角点和四条边、八种修饰键组合，共 128 次真实缩放、提交及一步撤销。固定期望表独立给出模型数值，覆盖负方向边的分母；每次 undo 恢复原工程。
- 三个原始回归探针在相同期望和数值误差范围下通过。
- 实际 Controls 输入路由中，单次拖动中切换 Alt、默认和 Shift+Alt，预览均从原种子计算，释放后一步撤销恢复原工程。
- 实际自由缩放预览后 Esc 只产生原 History(4) 取消，恢复原工程，释放手势。

真实模型、采样值、输入路由和历史结果证明交互行为，不冒充物理 OS 窗口、DPI 或最终 GPU 像素验证。正常产品 cargo check 成功，19 项既有警告。日志 `final-transform-controls.log`、`check.log`；新文件格式、样式消费者与差异检查通过。远程 CI 待本 PR 运行。

## 观测和剩余范围

| ID | 分类 | 处置 |
|---|---|---|
| TSM-MODE | 功能修正 | Alt/Shift 选择比例模式，128 次真实宿主编辑/撤销验证全部角点、边和 2D/3D。 |
| TSM-HISTORY | 协议边界 | 不改历史协议；中途切换模式和 Esc 的真实路由/模型结果通过，提交后一步撤销恢复原工程。 |
| TSM-MATH | 既有边界 | 保留投影观测、有限值/值域验证和零分母保护；不把正常无变化手势计为故障，没有新故障观测项。 |
| TSM-LOG | 既有边界 | 不修改日志级别、默认信道、Present、in-flight 或 worker，不把本次健康交互测试扩大为新增的错误信道验收。 |
| TSM-STACK | 待完成 | #443 合入后将本 PR 更新到 main 并跑新 head 的 CI；旧分支的绿色结果不冒充更新后的验证。 |

本批完成缩放比例修饰键；#159 的移动/缩放吸附等剩余范围继续开放。

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md；没有新素材、兼容名称或样式 token。本批相对 #443 的范围只涉及缩放行为。

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

不以编译成功或上述处置冒充完整视觉基线整改；实际功能证据与限制如上。
