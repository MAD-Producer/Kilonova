# 中文 UI 系统字体候选选择（#129，阻塞 #450/#451）

分支 `codex/ui-cjk-font-selection`，起点 main `dec6fec0`。开始前核对 331 个分支引用、124 个工作树及开放 PR，没有覆盖能力优先的字体选择实现或相同工作树改动。审计在本工作树 `.tools/ui-cjk-font-selection/branch-audit.json`。

## 实际故障与改动

#450 的诊断 head `db16f7c1`、CI `39925727728`、macOS job `120321493877` 实际记录：中文主窗提示没有字形，选中 `Helvetica Neue`，`此/版/本/中` 均不支持，原始 glyph UV 全零；系统候选中却有 `Heiti SC`、`Heiti TC` 与 Hiragino 等中文字体。日志保存在 #450 工作树 `.tools/export-shortcut-feedback/diagnostic-job-120321493877.log`。#451 的原非空字形断言也失败。原断言没有跳过或放宽。

原选择循环只要字体可解析就立即返回，没有在自动中文选择时检查字符覆盖。先把原循环机械提取到生产 `select_candidate`，保持选第一个可解析字体的行为，再用真实已安装 Latin/CJK 字体取得本地失败：候选已有中文字体，仍选择 Arial，`中` 的 glyph ID 为 0；3 项新探针为 2 通过、1 失败、exit 101。原文本源码和失败日志在 `.tools/ui-cjk-font-selection/original-text.rs`、`original-font-regression.log`。

现在自动中文选择与中文 fallback 会继续查找覆盖 `中` 的候选；补入 macOS 已安装 SC 字体的优先族名。用户明确指定的可用字体仍优先，英文选择保持原顺序；如果整个候选列表没有中文覆盖，继续保留原第一个可用 Latin 字体，而不是改变启动返回值。完全没有可用字体仍返回原错误。

最多额外保留一个首选 Latin fallback 的 FontArc 和族名；找到中文字体后释放它。选择发生在字体初始化/切换时，不在逐帧绘制中。没有字体下载、复制、打包、系统字体安装或新增依赖，运行时继续读取用户系统已安装字体；新增的是独立编写的选择逻辑和系统字体族名。

## 验证范围

三项新测试使用真实系统字体数据库与字体文件，通过同一生产候选函数验证：

- Latin 候选在前、CJK 候选在后时，自动中文模式选择 CJK，真实 atlas 为两条提示的每个非空白字符生成非零 bitmap。
- 不请求中文优先时仍选原 Latin；只有 Latin 时沿用原降级；空候选返回 None。
- 实际 `from_system_language` 和用户指定 Latin 的 `from_family` 都能生成完整中文提示；用户字体族保留，实际 text_style 的绘制字形数与所有非空白字符数一致。

`final-text-tests.log`：16 通过、0 失败、0 忽略，包含三项新测试，以及既有 atlas 增长、像素/指标保留、容量拒绝、DPI、缩放、测量、空白与默认级别观测验证。

`final-ui-cpu-tests.log`：50 通过、0 失败、12 忽略，显式 `--no-default-features` 的 CPU 套件；16 项文字测试已包含其中，不重复相加。12 项既有 GPU 测试不算通过。本分支没有增加 ignore 或更改 GPU 门禁，当前主线 CI 明确启用 UI/Desktop/Host/Media 的 gpu-tests。

本地证据基于 Windows 已安装字体，不能冒充 macOS 复验、物理窗口/TSF/全 DPI 或 GPU 像素验收。还需本 PR 最新提交的全部必需 CI，以及 #450/#451 带着本修复的实际状态栏验证；二者在平台结果到达前继续保持未验收状态。产品 cargo check 通过（`check.log`，19 项既有警告）；来源、样式消费者、新测试格式和差异检查通过。

## 观测与边界

| ID | 分类 | 处置与证据 |
|---|---|---|
| CJK-SELECT | 功能修正 | 自动中文选择不被可解析但缺字的 Latin 字体截断；真实候选顺序的原失败和 bitmap/绘制测试。 |
| CJK-EXPLICIT | 既有边界 | 保留用户指定字体与英文优先序；中文 glyph fallback 使用同一修正后的系统选择。 |
| CJK-DEGRADE | 保留的降级 | 无 CJK 时保留可用 Latin，空/不可用候选仍走原 None/错误边界；不能解释为未安装中文字体也已支持全部中文。缺字沿用既有 text.missing_glyph 观测。 |
| CJK-LOG | 既有覆盖 | 不改日志、计数器、阈值、恢复或 stdout；既有真实字体/样式默认级别观测测试通过，不把新选择测试描述成新日志信道验收。 |
| CJK-HOT | 正常路径 | 无新逐帧文件 IO、日志、重试、worker 或超时；初始化保留的 fallback 数量为一。 |
| CJK-PROJECT | 既有边界 | 不改项目格式、命令、历史或媒体排版模型；没有新的错误结构或协议返回值。 |
| CJK-REMAINING | 待验收 | Mac 与依赖 PR 的真实显示结果继续跟踪；复杂文字 shaping、双向排版和其他字体覆盖不扩大为本批交付。 |

## DESIGN §0.7 与 §12

已核对 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md。无新增第三方字体文件或其他资产，无样式 token、颜色、控件尺寸、命中容差或行结构修改；仍使用原 system-ui 规则及显式用户字体设置。

| §0.7 基线行 | 本批状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：未修改列布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件或命中区。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值编辑。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔交互。 |
| effect_controls 行高/步距 | retained with reason：未修改效果控件。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果开关。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线。 |
| timeline snap 指示线 | retained with reason：保留原拖动反馈。 |

§12 以实际字体覆盖、bitmap、文字绘制及后续平台/依赖 PR 结果验收，不声称所有面板、工作区与平台已完成视觉整改。
