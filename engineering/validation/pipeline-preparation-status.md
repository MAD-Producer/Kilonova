# 预览准备状态栏验收记录（#64）

## 实现边界

宿主通过只读原子句柄发布已有准备任务的 waiting 状态；读取不会访问 Session 锁、
发命令、复制快照、执行文件 IO 或新增线程。ensure/poll 返回后同步实际状态，
cancel/drop 立即清除。后台缓存保存本身不阻止预览，因此不显示为等待画面。
任务错误继续使用现有错误信道，原始失败与 ticket 不因提示而清除或重试。

桌面状态栏右侧在等待时显示“正在准备预览…”或“Preparing preview…”。左侧
既有渲染错误、拖动读数和状态消息仍按原顺序选择，并保留各自样式；给右侧文字
预留实际测量宽度和现有 padding token，避免覆盖。状态变化使主窗和浮窗重绘，
完成后不需要用户再移动鼠标；状态不变时不持续标记 dirty。
准备期间仅隐藏没有实际消息时的默认“就绪”占位，避免同时出现准备中和就绪。

## DESIGN 处置

| §0.7 基线位置 | 处置 | 理由 |
|---|---|---|
| path_controls 六种步长 | retained with reason | 未改属性行；本批仅状态栏 |
| path_controls 双列划分 | retained with reason | 未改数值列布局 |
| path_controls 按钮尺寸 | retained with reason | 未改按钮或命中区 |
| path_controls 数值背景 | retained with reason | 未改字段绘制 |
| path_controls 布尔按钮 | retained with reason | 未改布尔交互 |
| effect_controls 行高 | retained with reason | 未改效果属性行 |
| effect_controls 布尔 | retained with reason | 未改效果开关 |
| effect_controls 枚举 | retained with reason | 未改下拉行为 |
| effect_controls hover | retained with reason | 未改悬停绘制 |
| effect_controls 层级 | retained with reason | 未改效果树 |
| timeline 列坐标 | retained with reason | 保留兼容基线 |
| timeline snap guide | retained with reason | 保留原绘制行为 |

上述处置不表示这些既有问题已整改；对应面板没有新增截图验收声明。

新增绘制沿用 `label.small.muted`、`error`、`padding.panel`、`row.height`，没有
增加颜色、尺寸或主题 token。主窗已有 20px 状态栏背景几何保留；浮窗文字用已有
22px row.height 替换原等值字面量。没有增加命中区域、快捷键、动画、编辑命令或
历史操作；不扩大声明到十九面板和所有工作区的整体验收。

## 本批验证范围

- 真 GPU 的受控阻塞：成功、原始终止失败、取消和 drop 的只读观察生命周期。
- 实际 Windows 主窗和浮窗绘制，中英文、900×600/1600×1000、DPI 1/1.5/2。
- 提示出现和消失的像素差异、原错误区域像素不变、完成无输入触发 dirty、正常
  轮询不反复触发 dirty、工程和 revision 不变。
- 默认日志和 stderr/stdout 边界沿用既有验收；本批新增观察不得制造额外成功日志。

以下最终结果覆盖本批状态栏及只读观察功能；不把它扩展为 #64 全部剩余范围已完成。

## 已发生的验证与修正

- `preparation-status-host-desktop-v1`：退出 101，616.656 秒。新接线漏改启动
  channel 的 tuple 类型，且误用了不存在的 `StyleSheet::size` 方法。保留全部
  编译错误；没有把它描述为运行失败，也没有修改测试断言来回避编译错误。
- 修正为三个启动字段、从既有 panel 样式读取 padding，并通过既有 metric API
  读取 row.height。`preparation-status-host-desktop-v2` 退出 0，468.968 秒，
  其中默认日志回归 1 项通过，宿主预热 18 项全部通过，均无新增 ignore。
- 新原生夹具在首次执行前核对发现：resize 会改变 frame stamp 并正常取消旧帧。
  保留缩放前旧帧必须等待的断言，缩放后验证其正常取消，再请求最终尺寸的新帧
  并验证实际完成。原始新夹具和修正原因均保留，没有改动原有验收用例。
- `preparation-status-native-v1` 退出 0，48.844 秒。实际 24 组中英文、尺寸、
  DPI、主窗/浮窗对照通过，共 52 张 PNG。900px 场景使用超长 renderError，
  左侧错误像素不变；正常状态补拍四张。这里的 renderError 是 UI 状态夹具，
  **不是**强制实际 shader 失败的端到端证明。
- 看图后发现默认“就绪”和右侧准备提示并存；这没有被像素差分断言捕获。生产
  代码改为仅隐藏准备期间的默认占位，真实状态和错误不变，最终原生复验通过。

首轮原生证据 [pipeline-preparation-status-evidence.zip](pipeline-preparation-status-evidence.zip)
87 个成员、4324683 字节，SHA-256：
`560ec0f1c08d3c1afc63928c63a562616c1b3d3d8c01bd994d8548e1c1672a4d`。
该归档含首轮完整截图和源码，不代替修正默认占位后的最终验收。

## 最终结果

| 命令记录 | 实际结果 | 耗时 |
|---|---|---:|
| preparation-status-native-v2 | 真实 Windows/Vulkan，24 组对照，52 张 PNG；测试 1 passed、0 ignored | 218.500s，含编译；测试本体 52.18s |
| preparation-status-normal-build-v2 | 普通 motion-studio，FFmpeg / diagnostics / ai-depth；36 条既有 warning | 76.687s |
| preparation-status-normal-app-v1 | 八个独立普通应用进程均正常退出，所有原控制器断言保留 | 32.312s |
| preparation-status-style-check-final | 对整个 panels 子树执行仓库既有样式消费检查 | 通过 |

八个普通应用进程分别覆盖已添加效果启动、首次/再次添加效果、原历史包存在、损坏
历史包四种情形的冷/热对照。每种实际缓存 load 均从 missing 到 restored；同场景
冷/热完整 RGBA 相同，抓图没有新增前台效果管线。损坏包的原始 Error 进入默认
文件日志和 stderr，stdout 保持 JSON-RPC。使用当前最终 exe，无测试专用入口或
替代 logger。首帧毫秒数保留在原报告，不声称所有热启动必然提速。

最终原生、普通构建和普通应用命令的全部源码身份完全相同，各命令首尾也相同。
宿主 18 项和默认日志 1 项成功轮次与最终源码仅有 panels/mod.rs 的默认占位修正
这一处文件不同；所有宿主与日志实现、断言完全相同，修正由最终原生验收覆盖，
没有重复运行未改变的 GPU 宿主套件。

除了每组错误前缀像素相同、右侧提示像素变化、工程/revision 不变、无输入完成
触发 dirty、稳定轮询不触发 dirty，最终归档器还直接检查正常中文状态栏：准备中
左侧 200×20 区域只有背景，完成后恢复“就绪”字形；右侧准备中字形可见，完成后
只有背景。它补充了看图发现的占位矛盾，未修改原原生断言。

实际状态栏截图裁剪（不是设计稿）：

![准备中](screenshots/preview-preparation-status.png)

![完成后](screenshots/preview-preparation-ready.png)

完整 [主窗](screenshots/preview-preparation-main.png) 与
[浮窗](screenshots/preview-preparation-floating.png) 也保留。

最终归档 [pipeline-preparation-status-final-evidence.zip](pipeline-preparation-status-final-evidence.zip)
共 214 个成员、7918982 字节，SHA-256：
`5e26407582b539d491cf6e9712ce9734869653aa0a61e4f29e27f561ec67340d`。
包含命令、完整源码身份、当前变更源码、默认日志、普通应用协议和像素证据、原生
52 张截图、首次新夹具与修正原因、固定 FFmpeg 运行库/程序身份和归档控制器。
逐成员 SHA-256 和 zip CRC 均已核验。源码始终位于独立工作树，未编辑其他人的
主工作树或进行中 PR。

## 错误与观察责任

| 项目 | 分类 | 边界 |
|---|---|---|
| 64-ST-O1 原子 waiting 句柄 | 新增观察 | 单个有界布尔状态，无观察 key、原因副本或热路径日志；owner cancel/drop 清零 |
| 64-ST-E1 准备任务错误 | 既有覆盖 | 沿用 ticket、原始错误和默认日志信道；本批不截断、清除或重试错误 |
| 64-ST-N1 稳定状态轮询、取消 | 正常分支 | 不额外写文件，不逐帧告警，不将取消视为失败 |
| 64-ST-U1 状态文字与重绘 | 功能改动 | 主窗/浮窗绘制和 dirty 同步；与日志规则分开验收 |
| 64-ST-P1 worker/回复协议 | 保留边界 | 无生产线程、期限、重启或 respond 语义变更；原生测试的子进程看门狗仅限制验收进程 |
| 64-ST-L1 支持范围 | 待补证据 | 本机 Windows/Vulkan 样本不能代替其他平台或所有管线终止故障 |
