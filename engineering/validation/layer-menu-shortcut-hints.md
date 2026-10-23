# 图层新建菜单的快捷键提示（#158 子项）

分支 `codex/layer-menu-shortcut-hints`，基于 main `160a4731`。开始前检查所有工作树 menus.rs 的未提交差异及开放 PR #425/#428/#430/#431/#432；没有四个新建项的绑定提示实现。其他工作树与根目录的改动保持原样。

新建纯色、文字、空对象和调整图层的顶层 Layer 菜单，改用已有 shortcut_bindings.label，分别关联 Application/Solid、Text、Null、Adjustment。显示当前有效绑定，沿用既有解除绑定时不显示提示的行为。没有硬编码键位、重新分配 Ctrl+Y、改变动作或启用条件。只处理 #158 的菜单提示子项，合集继续开放。

复用现有原生快捷键截图工作流增加一张顶层 Layer 新建菜单；原上下文菜单、编辑菜单及所有无工程动作/工程和撤销状态不变断言保留。没有添加新的单位测试或改动已有断言。8 项既有 shortcuts 回归通过，0 失败、0 忽略；正常 dev 构建通过，19 项既有警告。日志在 `.tools/layer-menu-shortcut-hints/shortcuts.log`、`build.log`。

中英文真实原生窗口截图及原面板验证在提交前另据实际结果记录；保留原 90 秒期限和默认日志级别。不把构建或绑定单测作为视觉验收。没有新依赖、第三方资产、兼容名称、颜色、尺寸、布局或命中区；已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md。

首次 Debug 英文完整面板验收在原 90 秒期限超时，原因尚未证实，不算通过。`.tools/layer-menu-shortcut-hints/native.log` 和 `artifacts/layer-menu-shortcut-hints/en/` 保存 stderr/stdout、文件日志、diagnostics 与部分截图。Release 的组合源码复验待完成，原始失败保留，不修改原期限、忽略项或既有断言。

## 观测与功能边界

| ID | 分类 | 处置 |
|---|---|---|
| LMS-BINDING | 正常分支 | 沿用实时绑定目录；默认、改键和无绑定没有新错误日志、计数或缓存。 |
| LMS-ACTION | 功能边界 | 菜单动作和启用条件不变，捕获使用真实 chrome；保留无工程动作及工程/撤销状态不变断言。 |
| LMS-LOG | 既有边界 | 不更改宿主错误、panic、请求、Present、in-flight、重试或阻塞处理；没有新增观察项。 |
| LMS-NATIVE | pending verification | 在实际截图完成前保持待验证；不声称物理 DPI、全部平台或整个菜单体系已验收。 |

## DESIGN §0.7 与 §12

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
| timeline 标签栏坐标 | retained with reason：保留兼容基线，未修改时间轴。 |
| timeline snap 指示线 | retained with reason：未修改主题或拖动反馈。 |

本批只补足四个已有菜单动作的可发现性；没有把十二项处置当作已修复视觉缺陷。完整远程 CI 待独立 PR 执行。
