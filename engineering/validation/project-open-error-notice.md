# 打开工程失败的可见提示

日期：2026-10-17。用户报告 change_project / os error 2。该错误说明一次文件操作找不到目标，可能是最近工程被删除或移动，也可能是其他文件缺失；只凭原日志不能确定具体对象。此补丁针对打开/切换工程的失败反馈，不实现全局错误中心。

## 行为

- 此前 poll_project_change 仅在旧 ticket.dirty=true 时保留错误对话框；干净工程的失败只写入 panels.message，首页没有清晰反馈。现在失败提示持续到用户确认。干净工程使用单独的确认界面，有未保存修改的工程保留既有保存/放弃/取消决定及失败说明。
- 确认界面显示操作、目标路径和原始原因。长中文路径及错误按字符换行，内容过长时复用 ScrollArea；Enter、Escape 或“确定”关闭。IME 和按键重复不会误确认；不因重绘自动重试或重新发送保存/放弃。
- 打开失败不替换当前工程，不自动创建同名空工程、不删除最近记录。确认后可重新选址。模态期间首页、主面板、浮动面板和已有 action guard 共用原有 project_modal 路由。
- 最近记录指向已不存在的目录时保留该目录为目标；不再因 is_dir=false 将其当作普通文件而打开父目录。存在的工程 JSON、缺失的 .json 文件及 .msproj/.aem 项目包保留既有路径转换。
- canonicalize 失败的 worker 回复补充 open project + 目标路径，原始 OS 错误串保留。没有把“找不到文件”一律翻译成“工程被删除”，也不靠额外 exists 检查掩盖 TOCTOU 或权限错误。

## 错误处理审计

| 编号 | 分类 | 证据与限制 |
| --- | --- | --- |
| POE-01 | 新的用户反馈 | 真实移动/删除工程目录、损坏 JSON、缺失包经过 Shell → Engine worker → 回复 → poll → modal；错误仍通过原回复信道传递。当前工程 JSON 字节、root、revision、历史标志保持。 |
| POE-02 | 新的原因上下文 | canonicalize 的原始 IO 原因保留，并补上目标目录。实际 child-process 用例验证 Unicode 路径及原因同时出现于提示、默认 info 文件和 Error stderr，Info start 位于同 request 错误之前，stdout 无日志；三个正常重绘不额外发送请求。 |
| POE-03 | 正常分支 | 未产生新回复时的 try_recv Empty、用户确认、模态阻止底层编辑、滚动、关闭后的重新选择均不是故障；不增加失败计数或 per-frame 日志。 |
| POE-04 | 既有覆盖 | 未保存工程的失败仍走既有决定框，保留原保存、回滚、并发 revision 校验、显式重试/取消行为。没有新增后台线程、重试、错误清除或锁内 logger。 |
| POE-05 | 边界 | 本次针对已进入 project transition 的打开失败。project_ticket / flush_bound_source 等进入切换前的同步失败仍沿用原有上层反馈；不是所有菜单操作的通用通知系统。损坏资源、权限拒绝、移动当前活动目录等具体故障不能仅凭原用户日志认定已修复。 |

## 验证记录

- project-open-error-tests-fixed.log：专项 16 项通过，包括 4 项新用例（实际移动/删除和恢复、损坏工程/缺失包、原始错误信道、长 Unicode 文本换行），原保存/新建/打开/导入/退出与并发确认用例保留。
- 首轮编译因新测试从错误的 motion_ui 根模块导入 Event/Modifiers 失败，已改为 input 模块；记录 project-open-error-tests.log。
- 第二轮样式校验拒绝新增的 560px token（既有限制为 512），14 项 Shell 初始化因此失败；将 token 与 fallback 一起改为 480，不放宽校验或断言。失败记录 project-open-error-tests-final.log。
- project-open-error-desktop-regression.log：完整桌面回归 371 通过、1 项需要实际音频设备而忽略、0 失败；123.77s。project-open-error-style-tests.log：6 项样式回归通过。
- CI Python 工具共 50 项通过、1 项需要外部 Rust 效果验证器而跳过。初跑 34 项中的两个模块因本地 Python 缺 PyYAML 而未加载；使用已有固定版本 python-ci-deps 后，仅重跑这两个模块，19 项通过。分别保留 project-open-error-ci-tools-tests.log 和 project-open-error-ci-tools-dependencies-fixed.log，没有放宽断言。
- 代码提交 c52d894 的带 FFmpeg Release 构建完成（仓库原有优化 + line tables、thin LTO），8 分 19 秒，22 项既有编译警告；记录 project-open-error-release-build.log。为复用同绝对路径下的依赖缓存，将已提交的分支切换到 curve-playback-pacing worktree 构建；project-open-error-notice worktree 保留相同代码提交的干净 detached 快照，没有移动或覆盖其他工作。
- 正式打包工具生成含 7 个 FFmpeg DLL、许可与来源清单的 Windows 包，并在独立 project-open-error-runnable 目录解压。tools/ci_desktop.py 针对该解压程序完成中英文原生窗口、全部面板与五种布局、样式覆盖、节点 GPU 与共享 MCP 验收，记录 project-open-error-native.log；新增截图已纳入必需清单。
- 目视核对 project-open-error-native/ 下默认 1600×1000 中英文及最小 900×600 中英文提示，标题、原始原因、换行和确定按钮没有裁切；英文最小窗口同时使用 row.height=32 等样式覆盖。测试系统实际 DPI 为 96（100%），更高 DPI 未作本机实测。原有键盘与焦点回归保留，新增真实 Shell 用例另验证模态优先级、Enter/Escape 确认及关闭后重选。
- 最小尺寸的普通 --smoke 在既有 workspace_presentation_probe 的“未呈现独立 node overlay”检查处失败；用父提交 921a96d 的解压程序相同参数复现。没有改检查或算作通过；记录 project-open-error-minimum-native.log 与 baseline-minimum.stderr.log。随后使用既有 --node-smoke 绑定图层节点场景完成两语言的 900×600 原生截图与验收，记录 project-open-error-minimum-native-followup.log；此结果不代表普通最小窗口节点布局已修复。
- 外观截图使用独立缺失路径产生的真实 IO 原因；该截图 fixture 不冒充完整 worker 交互验证，worker 和 UI 交互另由上述真实用例覆盖。之后的文档提交不修改 Rust/资源，交付程序对应代码提交 c52d894。

## 样式来源与 DESIGN.md 基线

新增 dialog.notice.width=480、max_height=320、button_width=96，先写入 dark.json，再由新提示组件引用；其余使用已有 padding.panel、row.height、panel.header.height、border、shadow/panel/error/text 语义 token。复用原生 button 和 ScrollArea，未增加依赖或外部资产。

§0.7 的十二项均 retained with reason；此处没有处理这些既有视觉问题，也没有据此宣称整个 §12 合规：

| 基线项 | 处置 |
| --- | --- |
| path_controls 六处纵向步进 | retained：文件未改。 |
| path_controls 两列宽度 | retained：文件未改。 |
| path_controls 混合按钮高度 | retained：文件未改，原命中范围未变。 |
| path_controls 自绘数值字段 | retained：文件未改。 |
| path_controls 全宽布尔按钮 | retained：文件未改。 |
| effect_controls 行高与步进 | retained：文件未改。 |
| effect_controls fx/— 布尔按钮 | retained：文件未改。 |
| effect_controls 全宽枚举按钮 | retained：文件未改。 |
| effect_controls hover alpha | retained：文件未改。 |
| effect_controls 缩进/层级 | retained：文件未改。 |
| timeline 十一列坐标 | retained：文件未改，保留兼容基线。 |
| timeline 吸附线硬编码颜色 | retained：文件未改，本次只新增错误提示。 |
