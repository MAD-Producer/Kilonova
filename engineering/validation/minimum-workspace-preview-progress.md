# 最小窗口节点预览的原生验收补充

在 900×600 窗口中，Nodes 工作区的嵌入预览因现有控件空间限制正常隐藏，原验收却要求普通布局必须出现节点叠加层。未改动的 `7b2aff0c` 原生 Windows/GPU 程序在 13.25 秒内以相同断言失败；这个基线早于当前 `302a15bf`，不宣称已运行后者的未改动二进制。

此改动只补充验收路径：分别记录普通布局结果；仅当 Nodes 普通预览无法容纳时，使用已有面板展开操作，验证实际呈现及 GPU 绘制计数增长，并保存截图，随后恢复布局、输入和焦点。保留原始项目及叠加层最终断言，且只认可实际呈现成功的叠加层。没有降低 135px 门槛，没有改变产品布局、样式、命中区域、工程格式或运行时重试策略。

## 已验证证据

- 源码基线 `302a15bf`，仅代码提交 `2911a696`；最终文档提交通过结果 JSON 中的三个源码 SHA-256 对应此程序。
- 四项面板展开/恢复回归通过，无失败、忽略；新增用例覆盖 900×600、1600×1000 的真实布局几何，原有三项测试及断言保留。
- Windows 原生最小英文工作区在 32.11 秒内通过，46 张设计截图检查及六次工作区呈现记录通过。普通节点叠加层为 false；已有展开视图产生 232×232 的真实预览，GPU 帧计数由 0 增至 3，布局、工程、修订和历史检查通过。
- Python 检查器接受真实原生报告，拒绝八项变造证据：恢复标记、历史标记、普通预览标记、叠加层标记、无 GPU 计数增长、缺失 PNG、越界视口、1px 视口。
- 原生程序 SHA-256：`84eec55b4bf901292a33182e8fbd5e2f46594a83a25b8daea19a76488738f46c`。

## 复现与边界

使用仓库 Windows/FFmpeg 构建环境，运行 `cargo test -p motion-desktop --bin motion-studio workspace::maximize::tests -- --test-threads=1 --nocapture` 并构建桌面程序。设置 `MOTION_SMOKE_MINIMUM=1`，执行 `motion-studio --smoke <独立目录>/report.json --locale en --project <独立目录>/project`。随后调用 `tools/ci_desktop.py` 的设计截图检查与展开预览检查。完整命令、文件散列和原始结果保存在归档内；其中 Windows 路径是原机器的证据位置，离线复验需指向解压后的截图。

已有 90 秒批处理超时与默认中英文成功记录保留在 #495 的验收记录中。本次独立最小窗口用例使用 240 秒外部测试上限，产品现有 5 秒呈现等待未更改。尚未声称完整八场景、所有 DPI、跨平台原生呈现或 DESIGN §12 全部验收通过。与 #495 缓存改动的完整组合矩阵另行验证。此项是 #134 的局部验收进展，不关闭集合 issue。

## 审计归类与来源

验收观察：新增普通/展开结果、实际 GPU 计数、截图及状态恢复证据。既有覆盖：原最终叠加层与工程断言、六次切换、五种预设、设计截图检查。正常分支：紧凑布局的预览缺席不是故障。功能/协议边界：只在原生验收流程调用已有展开操作；不改变产品路径、错误返回或日志等级。待验证项见结果 JSON。未新增依赖或第三方资产；截图是本仓库程序的原创测试输出，已核对 PROVENANCE.md、ATTRIBUTION.md、NOTICE。

开始前已检查开放 PR、417 个引用及 123 个工作树；验收源仅发现原版本，另一个工作树的同文件与主分支内容一致，未重复已有展开验收实现。

## DESIGN §0.7 基线逐项处置

下表表示本次改动的处置边界，不表示重新证明旧条目仍存在或已经修复；视觉合规仍待对应专项验证。

| 条目 | 状态 | 原因 |
| --- | --- | --- |
| path_controls.rs 六处行高 | retained with reason | 验收补充未修改路径属性行高 |
| path_controls.rs 双列划分 | retained with reason | 验收补充未修改列宽计算 |
| path_controls.rs 按钮高度 | retained with reason | 保留原有按钮尺寸与命中区域 |
| path_controls.rs 数值字段背景 | retained with reason | 未修改数值字段绘制 |
| path_controls.rs 布尔控件 | retained with reason | 未修改布尔控件及事件路由 |
| effect_controls.rs 行高与步进 | retained with reason | 未修改属性行布局 |
| effect_controls.rs 布尔文本按钮 | retained with reason | 未修改布尔控件 |
| effect_controls.rs 枚举按钮 | retained with reason | 未修改枚举控件 |
| effect_controls.rs hover alpha | retained with reason | 未修改悬浮样式或 token |
| effect_controls.rs 缩进层级 | retained with reason | 未修改属性树层级 |
| timeline.rs 标签列坐标 | retained with reason | 按兼容基线保留列宽与坐标 |
| timeline.rs 吸附引导颜色 | retained with reason | 保留现状；未新增或改写颜色 token |

原始证据：[数据归档](data/minimum-workspace-preview-2911a696.zip)（SHA-256 `51adbaf52b9fecc945076d05a66f021b1a9e941baa8916cf4c938b2aa35b3136`）；[结构化结果](minimum-workspace-preview-results.json)。
