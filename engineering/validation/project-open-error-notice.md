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
- 带 FFmpeg 构建、原生窗口截图待记录。外观截图使用独立的缺失路径真实 IO 原因；该截图 fixture 不冒充完整 worker 交互验证，worker 和 UI 交互另由上述真实用例覆盖。

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
