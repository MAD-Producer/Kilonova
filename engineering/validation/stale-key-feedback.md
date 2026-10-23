# 失效关键帧操作反馈（#158 子项）

分支 `codex/stale-key-feedback`，起点 main `6d24305b`。开始前核对 326 个分支引用、119 个工作树及开放 PR；历史分支只有既有失效选择守卫，没有相同提示实现。九个无共同祖先的分支没有时间轴快捷键源文件。审计保存在工作树 `.tools/stale-key-feedback/branch-audit.json`。

现有键盘路由已保留失效关键帧的操作归属，避免 Delete 回落成删图层。本批沿用这层保护，为 Delete、F9/Shift+F9/Ctrl+Shift+F9 的显式失效变换关键帧增加中英文状态栏说明。没有选择的正常分支、其他属性域、导航、编辑动作和历史协议保持原有路径。

提示通过现有 panels.message 显示。实际选择重新有效、明确清除、或撤销恢复该关键帧后，在既有键盘路由刷新点清除本批提示。用两种固定译文识别本批状态栏消息，保留其他消息；没有引入新 UI 错误结构。无提示时立即返回，不分配、不扫描通道、不添加日志或文件 IO。

## 失败对照与验证

四个新探针先在原生产逻辑执行：2 通过、2 失败、exit 101。失效动作仍被安全拦截，实际 message 为 None，实际状态栏字形仍是就绪提示。重新选择后的真实编辑/一步撤销、IME/文本/重复键守卫原本通过。原源码和日志保存为 `original-timeline-shortcuts.rs`、`original-feedback-regression.log`。

第一轮提示修复后，47 项快捷键测试通过（43 项既有加四项新测试）。追加的提示生命周期探针发现重新选择后旧提示残留，实际失败保存在 `notice-lifetime-regression.log`，0 通过、1 失败；随后接入消息刷新。原有非空、精确绘制、所有权、历史断言均保留，没有修改既有断言来通过验收。

最终默认构建的结果：

- `final-shortcut-tests.log`：48 通过、0 失败、0 忽略，含本批五项；不把第一轮 47 项再相加。
- `final-keyboard-tests.log`：15 通过、0 失败、0 忽略，复验调用方的节点/主窗/浮窗、输入归属、图层动作和搜索路由。
- `check.log`：产品 cargo check 通过，19 项既有警告。

失效矩阵覆盖 En/Zh、Timeline/Curves、主窗/浮窗 owner、工程身份切换、合成身份切换、真实 Engine 删除关键帧和真实删除图层，四种编辑键共 128 组配置，各两次按键；全部没有编辑动作，原 key ownership、项目和 undo/redo 状态保留。标识切换使用 UI 快照，删除使用真实核心命令，二者不混为同一种证据。

重新选择有效关键帧后，真实 easing 和 Delete 修改模型，一步撤销分别恢复原工程。生命周期测试另外覆盖两种原消息语言与两种当前语言组合下的重新选择、明确清除和 Undo 恢复，保持尚未恢复时的提示与其他已有消息。

实际 chrome/floating_chrome 在 En/Zh、900×600/1600×1000 下绘制状态栏，精确比较字形 UV、位置、尺寸和颜色，并验证没有工程动作、模型不变。沿用应用支持的语言字体初始化。这里证明 UI 路由与绘制命令，不冒充物理窗口、TSF、全 DPI 或最终 GPU 像素验收。

## 观测和功能边界

| ID | 分类 | 处置和证据 |
|---|---|---|
| SKF-FEEDBACK | 功能修正 | 失效的显式变换关键帧编辑有可见的本地化原因；实际状态栏字形验收。 |
| SKF-OWNERSHIP | 既有覆盖 | 保留失效 key 所有权，Delete 不回落删层；原有与新增真实路由断言均通过。 |
| SKF-RECOVERY | 功能修正 | 有效重选、明确清除和 Undo 恢复后清除本批旧提示；其他消息保留。先失败再通过的生命周期探针有记录。 |
| SKF-NORMAL | 正常分支 | 失效 UI 选择、无选择、IME/文本/重复键不是新 Err；不增加故障计数或告警。 |
| SKF-HISTORY | 既有边界 | 不改核心命令、事务、历史和返回值；有效重选后的编辑/一步撤销通过。 |
| SKF-LOG | 既有观测 | 没有新增日志记录、stdout、文件 IO、worker、超时、重试或重启，不清除其他错误消息。不把健康功能测试扩大为新增的默认日志/Error stderr 验收。 |
| SKF-REMAINING | 待完成 | #158 的其余快捷键语义、可发现性和输入分配工作继续开放；其他属性域的提示不扩大为本批交付。 |

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md。提示文本独立编写，无新增第三方文本、资产、依赖、兼容名、颜色、尺寸、布局、样式名或命中容差；继续使用现有状态栏。

| §0.7 基线行 | 本批状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：未修改列布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件与命中区。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值编辑。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔交互。 |
| effect_controls 行高/步距 | retained with reason：未修改效果面板。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果控件。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线。 |
| timeline snap 指示线 | retained with reason：保留既有拖动反馈。 |

§12 仅声明上述状态栏绘制、消息恢复和相邻输入/历史验证，不声明所有 19 面板、全部工作区、平台或 DPI 已完成视觉整改。来源、样式消费者、新测试格式和差异检查按实际结果记录；最新提交 CI 全部必需检查通过后才合并。
