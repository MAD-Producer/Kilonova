# 属性面板 CI 失败修复

范围：PR #557，原提交 `9df243fe` 的桌面 CI run `40833617304`。
本轮同步主线 `2ffeeda7`；不以原分支或其他 PR 的绿色检查代替最终提交的 CI。

## 原失败与修复

- 三个平台的节点检查器测试仍点击旧的独立删除行之后。按当前 header、标题/删除、ID 行的语义尺寸计算点击位置；字段激活、实际写入 `23.5` 与保存快照断言全部保留。
- Windows 原生剪贴板用例在蒙版阶段仍使用旧数值列。真实重现 `History(0)` 外泄；改用共享 InspectorColumns 命中不透明度字段，并新增进入实际编辑状态的断言。保留草稿撤销/重做、菜单、跨窗粘贴、工程历史、保存等全部既有断言。`track_caller` 只改善测试失败定位。
- Linux 的普通向量格宽度不足。普通向量使用 `effect.number.cell_width` 完整宽度，组件间距另由 `effect.number.gap` 计入；带拾取器的坐标仍保留现有紧凑格宽。既有 `12.000` 宽度断言与坐标精确宽度断言均未修改。
- 合并主线时，蒙版属性列采用本 PR 的共享布局，同时保留主线对羽化联动按钮的标题空间预留。不是删除主线功能解决冲突。

本机首轮复用共享 target 命中了旧 UI 编译产物；强制重新编译本分支 UI/render 后解决。随后测试 EXE 因未配套 FFmpeg DLL 无法启动；复制到独立 runtime 并提供固定 DLL 后才执行测试。这两类启动/编译失败均保留，不计为测试通过。
第一轮扩大了所有向量格，真实原生验收发现坐标宽度由 39 变为 42；已收敛修复范围并保留该失败。原生验收新增投影与普通向量同屏的 220/360px 截图及普通向量宽度检查。

## 验收边界

最终组合的执行结果、原始日志和源码/截图 SHA-256 见同名 JSON 与证据 ZIP。
截图来自实际 Windows/wgpu 窗口和产品绘制，输入由原生用例提供组件 Input；不宣称物理键盘注入、多显示器或 Linux/macOS 本地验证。
本机失效的 Epic Vulkan overlay 注册项仍产生 Error，原始 stderr 保留。没有新增产品故障捕获、重试、计数器或降级语义；既有拾取失败的默认文件日志、Error stderr 和 stdout 隔离单独验证。

## DESIGN §0.7 逐项处置

| 基线项 | 本轮处置 |
|---|---|
| Path 六处纵向步进 | retained with reason：未改行高，沿用本 PR 既有处置，不追加已修声明。 |
| Path 两列分割 | retained with reason：保留本 PR 共享列，同步主线羽化按钮预留。 |
| Path 按钮高度 | retained with reason：未改控件命中高度。 |
| Path 自绘字段背景 | retained with reason：未改编辑/背景规则。 |
| Path 布尔宽按钮 | retained with reason：未改布尔控件。 |
| Effect 行高/步进 | retained with reason：未改行高、滚动或裁剪。 |
| Effect fx 布尔 | retained with reason：沿用现有 Checkbox。 |
| Effect 枚举 | retained with reason：沿用现有 Dropdown。 |
| Effect 手工 hover | retained with reason：未改状态颜色或背景。 |
| Effect 层级 | retained with reason：未改树的层级、折叠或缩进。 |
| Timeline 标签列 | retained with reason：未改兼容坐标。 |
| Timeline 吸附线 | retained with reason：未改绘制或拖拽反馈。 |

本轮修复向量格宽度缺陷，不宣称 DESIGN §12 全部完成。未新增 token、第三方依赖或资产。

## 最终组合实测结果

40 项自动用例与 2 项显式执行的手动原生 GPU 用例通过（42 个不同用例）。自动 feather 过滤中原生用例先按声明忽略，随后单独实际执行通过，不重复累加。节点 25、效果行 2、导航 10、蒙版布局 1、羽化复选框 1、跨窗口剪贴板 1，以及拾取/宽度和羽化两项 GPU 用例。羽化 GPU 用例包含中英文、主窗口/浮动路由及 1/1.5/2 倍例，共 24 张状态图；浮动路由复用 attached surface，不把它记为独立浮动 HWND 验证。

220/360px 截图上半部 `(0,0,width,340)` 与原分支逐像素一致，证明未挪动坐标区。普通向量宽度及相邻控件由现有几何断言和新增真实截图核验。FFmpeg 程序构建及独立 runtime 的 `--help` 启动通过；最终测试/产品构建分别保留 26/28 条 warning。原生拾取真实错误仍在默认级别进入文件与 stderr，stdout 隔离检查通过。

[220px 实图](screens/panel-ci-repair/effect-vector-220.png) · [360px 实图](screens/panel-ci-repair/effect-vector-360.png) · [羽化浮动路由](screens/panel-ci-repair/mask-feather-zh-floating-route-1x-linked.png) · [主窗口解除联动](screens/panel-ci-repair/mask-feather-zh-main-1x-unlinked.png)

[机器回执](panel-layout-ci-repair-results.json) · [原始证据](data/panel-ci-repair.zip)。最终提交的跨平台 CI 尚待运行，不宣称已绿色。
