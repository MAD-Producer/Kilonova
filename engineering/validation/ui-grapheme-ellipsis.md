# 标签省略的字素边界验收

日期：2026-10-22（Asia/Shanghai）。基线 main `94f0bc30`；分支 `codex/ui-grapheme-ellipsis`。对应 #129 的 Unicode 标签边界子项，不关闭整个合集。

先核对开放 PR、最近分支、`text.rs` 全分支历史和 #129 timeline：PR #367 已实现 TextEditor 的字素编辑；#345 已消除标签临时分配；本次仅补 fitted_text 的省略边界。PR #419 修改描边，不编辑本次文字文件；#415 与资源预算分支也无重叠。

## 实际变化

| ID | 处置与证据 |
| --- | --- |
| UGE-01 | 功能修复：省略前缀从逐 Unicode 标量推进改为按 extended grapheme cluster 推进；整个 cluster 放不下时一起省略，保留前一个完整 cluster 与省略号。不改变完整标签的快速返回、测量度量、glyph 缓存/定位、字体选择、API 返回类型和绘制调用者。使用已经存在的 unicode-segmentation，无新依赖。 |
| UGE-02 | 原实现实测：四项新回归中一项分配检查通过，三项失败。实际返回 `A🇯…`，应为 `A…`；重音文本在宽度 23 时返回 `Ae…`，拆开 `e + U+0301`；实际 GPU 的省略号位置也与完整前缀不同。原始记录 `.tools/grapheme-before.log`。修复后相同边界、文本和像素相等条件通过；像素断言只缩短失败输出，不放宽条件。 |
| UGE-03 | 输入覆盖：组合重音、Hangul Jamo、Indic conjunct、肤色/ZWJ emoji、区域旗帜、家庭 emoji、keycap 和 CRLF。每 0.25 逻辑像素扫描可用宽度，验证返回值不超宽、前缀属于原文且结束于字素边界；完整可用宽度保持原文。另以已知旗帜边界验证不能放下时整体省略、放得下时保留完整 cluster。负宽度/NaN/无限宽度沿用既有约定。 |
| UGE-04 | 实际调用者：styled text 在五档 DPI、12/28 字号、左/中/右对齐共 30 组真实 GPU 场景，与明确的 `A…` 控制文本逐字节相同；控制文本必须有可见 ink，不能用空白渲染冒充通过。不要求系统字体支持完整 emoji，测试的是截断后不应进入绘制的 cluster；不宣称实现 emoji 或复杂脚本 shaping。 |
| UGE-05 | 性能边界：借用前缀和流式 cluster 迭代不创建临时容器。实际普通/样式文字在窄/宽区域，以受支持重音字体预热后绘制 1 与 200 个标签，均为零分配。公开 ellipsize 的 String 返回保持原行为；不声称它零分配，也不推断固定耗时收益。 |

## 验证

工作区 `E:/Dev/aem/.worktrees/ui-grapheme-ellipsis`；复用现有 MSVC 和锁定/离线依赖。本机 target 复用、禁用增量/链接符号，无仓库构建配置变化。

- `motion-ui` 全套：20 组、143 通过、0 失败、0 忽略。包括四项新边界/像素/分配回归及既有 ASCII/CJK 标签像素、图集生命周期、输入、布局和错误观察。记录 `.tools/ui-tests.log`。
- 生成 `artifacts/grapheme/prefix-{scale}x.png` 五档 DPI 原始 GPU 读回，已目检 2× 的 A 与省略号。该场景验证完整前缀及省略号，不冒充完整 emoji 排版截图。
- 重新构建本分支 framework 示例，1600×1000 / 900×600 的中英文主窗和原生浮窗验收均通过，覆盖数值输入、撤销、取消、焦点、停靠保存恢复。`artifacts/grapheme-framework-default/`、`artifacts/grapheme-framework-minimum/` 共 24 张 PNG 与 JSON；已目检最小尺寸英文主窗。它验证既有窗口交互没有回归，新 Unicode 截断的像素证据来自专用场景；归一化事件驱动不等于物理键盘或系统 IME 服务验收。
- 来源检查、独立新增文件格式检查及 git diff --check 通过。无第三方资产、字体、图标、文案、外部算法或兼容名称新增。

## 观察与剩余责任

裁短是正常布局，不是故障，不新增逐标签日志。缺失字形、图集耗尽、GPU 提交和现有失败/降级观察沿用原边界。完整 motion-ui 的默认日志文件、Error stderr、stdout 隔离、限频及恢复测试实际复验，归为既有覆盖；无新命令/线程、无重试或吞错。未更改既有断言、忽略、超时和日志阈值。

DESIGN §0.7 的 12 项均 retained with reason：Path 六处步进、双列、按钮高度、数值背景、布尔控件；Effect 行步进、布尔、枚举、hover、层级；Timeline 固定列与吸附色。本次不编辑 panels、主题、行布局、命中区、快捷键、工程数据或历史。对应 §12 的标签像素和分配证据见上，不宣称其他设计整改完成。

剩余：glyph 栅格化/布局仍按单 glyph 执行；完整复杂脚本 shaping、双向文字、组合标记定位、任意系统字体覆盖及 #129 其他条目继续开放。字素安全截断不能替代这些能力。跨平台 CI 与合并状态以本 PR 页面为准。
