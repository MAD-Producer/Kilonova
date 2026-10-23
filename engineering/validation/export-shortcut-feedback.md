# 视频导出快捷键不可用提示（#158 子项）

分支 `codex/export-shortcut-feedback`，起点 main `72abf08d`。开始前核对 323 个分支引用（269 个不同提交）、所有工作树与开放 PR。命中差异只涉及既有一次日志或取消导出文案，没有相同反馈实现；审计保存在工作树 `.tools/export-shortcut-feedback/branch-audit.json`。

默认构建按 Ctrl+Alt+M 原来只有一次 Warn，界面不解释不可用原因。现在每次有效按键都用已有 panels.message 状态栏显示独立编写的中英文提示。支持视频导出的构建继续发出原 ExportMovie 动作；菜单的原可用性判断不变。这是功能反馈修正，没有引入新的日志观测。

## 失败对照和验收

相同三个探针先在原生产逻辑执行：1 通过、2 失败、exit 101。实际路由的 message 为 None，实际主窗底部字形是原就绪状态而非不可用提示；IME/文本/重复键守卫通过。原键盘源码和日志保存在 `original-keyboard.rs`、`original-feedback-regression.log`；没有通过编译失败或放宽断言制造对照。

新测试检查 En/Zh、Timeline/Composition/Nodes/NodePreview、停靠与浮窗 owner，既验证不发导出动作，也验证后续独立按键仍刷新提示。正常原生重复键、IME composing/consumed 与文本编辑保留现有消息和动作归属。实际 chrome/floating_chrome 在 900×600、1600×1000 下绘制完整本地化状态栏字形，比较 UV、坐标、尺寸与颜色，并验证快照不变且没有工程动作；不只断言 message 字段赋值。

第一次修复后套件为 17 通过、1 失败：绘制夹具用 from_system 创建的英文默认字体不能生成中文提示字形，触发原有非空断言。保留 `first-feedback-font-fixture.log`；仅把夹具改用应用支持的 from_system_language 路径，补齐语言字体选择和回退，非空及精确字形断言均保留，产品字体代码未改。

这些是实际 UI 路由和绘制命令验证，不冒充物理系统窗口、TSF 输入投递、DPI 或 GPU 像素验收。启用视频导出特性的原动作分支未改动；本地执行结果以默认构建为限，不能把测试内的特性分支描述为两种构建均已执行。

## 观测和协议边界

| ID | 分类 | 处置 |
|---|---|---|
| ESF-UI | 功能修正 | 不可用快捷键通过已有状态栏显示原因；主窗、浮窗和中英文绘制验收。 |
| ESF-UNAVAILABLE | 正常能力边界 | 未启用视频导出不是新 Err/崩溃。原进程级 Once Warn 保留，其抑制范围为整个进程；新提示在 Once 外，每次有效按键均可显示。 |
| ESF-OWNERSHIP | 既有边界 | IME、文本编辑、重复按键的守卫保持；现有键盘路由测试另行记录实际结果。 |
| ESF-ERROR | 既有边界 | 不清除渲染错误、不改状态栏已有错误/拖动提示优先级，不吞 Err 或 panic，不增加重试、超时或线程。 |
| ESF-LOG | 既有观测 | 没有新日志记录、级别、计数器、文件 IO、产品 stdout 或健康 Present 日志；不将健康功能测试描述成新增 Error stderr/日志信道验收。 |
| ESF-REMAINING | 待完成 | #158 其余快捷键语义、可发现性和输入分配子项保留开放，不关闭合集。 |

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE、DESIGN.md；只有原创产品提示，无新增第三方文本、资产、依赖、兼容名称、样式、尺寸、颜色、控件布局或命中容差。沿用现有主窗和浮窗状态栏。

| §0.7 基线行 | 本批状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：未修改列布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值字段。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔交互。 |
| effect_controls 行高/步距 | retained with reason：未修改效果面板。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果控件。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线。 |
| timeline snap 指示线 | retained with reason：保留既有拖动反馈。 |

§12 验收限定本批消息路由、绘制和输入归属，不作 19 面板、全部工作区或全平台视觉合规声明。编译不能替代上述可复现的功能验证。

## 最终结果

默认构建的键盘套件 `final-keyboard-tests.log` 为 18 通过、0 失败、0 忽略（包含本批三个测试，不与原 15 项相加）。主窗/浮窗、En/Zh、最小/默认尺寸的状态栏字形精确比较全部通过。新测试 rustfmt、来源与样式消费者检查通过；产品 cargo check 通过，19 项既有警告（`check.log`）；差异检查通过。远程 CI 待 PR 最新提交运行，全部必需检查通过后才合入。
