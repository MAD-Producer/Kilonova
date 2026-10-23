# 最小窗口的时间轴菜单验收夹具

基于 main `160a4731`，分支 `codex/timeline-menu-smoke-fixture`。核查开放 PR #425/#428/#429/#430 和工作树的 design_acceptance/timeline_columns 改动，没有相同采样列修正。旧工作树的未提交改动属于其他捕获项目；未修改它们或根工作树。

## 问题、修正与证据边界

节点索引分支 `229208f6` 的 Release 原生验收中，默认尺寸中英文分别完整通过（各 161 张报告图）；中文最小窗口在 33235ms 失败，原因为 `timeline dropdown capture must open without editing the project`。该分支的时间轴菜单夹具和列布局源码与本分支基线相同。原始失败不是主线二进制独立执行，也不是节点索引改变时间轴行为的证明；记录保留于该工作树 `.tools/node-socket-index/release-native.log` 和 `artifacts/node-socket-index-release/zh-runtime/`，包括原始 stderr、文件日志、diagnostics 和部分截图。

夹具已选择父级/混合列并隐藏其他可选文本列，但遗漏默认显示的三个采样开关。真实 ColumnRects::with_sampling 在窄布局优先保留采样和名称，会重新隐藏所请求的菜单列；旧点击随后落在其他控件，无法打开目标菜单。

修正仅在这两个菜单的临时捕获夹具中选择隐藏采样列，与已存在的其他列选择保持一致。保留原来的真实 chrome 点击、菜单确实打开、工程动作必须为空和完整工程不变断言，保留尺寸、期限、屏幕项目和所有节点断言。捕获后原 timeline 克隆恢复用户的完整列偏好；不改变正常窗口默认列、宽度适配或布局。

Release 构建及中英文默认/最小窗口的四组完整原生验收在提交时待完成。每组使用独立偏好目录和默认日志级别，保持原 125 秒收集期限；不以默认尺寸的成功代替最小尺寸结果。独立 PR 的远程 CI 待运行。来源检查、样式消费者和差异检查通过；新依赖、资产及兼容名称均为零。

## 观测审计

| ID | 分类 | 处置 |
|---|---|---|
| TMS-FIXTURE | 功能边界 | 临时捕获选择真实可见列，恢复完整 timeline；不改产品设置或工程。 |
| TMS-FAILURE | 既有观测 | 原生验收失败沿用文件及 Error stderr，保留原始原因。没有吞错、忽略断言、扩大期限或增加产品重试。 |
| TMS-PRESENT | 正常热路径 | 无正常产品绘制、worker、in-flight、日志或文件 IO 变化。 |
| TMS-NATIVE | pending verification | 完整中英/两种窗口验收及远程 CI 待独立结果；后续结果记录于 PR，不提前视为通过。 |

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md；只修改 shell 捕获夹具。

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
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，夹具只选可见列。 |
| timeline snap 指示线 | retained with reason：未修改主题或拖动反馈。 |

不把临时夹具修正宣称为以上视觉基线整改，也不以编译作为原生验收。Related #135，合集保持开放。
