# 效果应用反馈与连续搜索验收

日期：2026-10-22（Asia/Shanghai）。关联：[#226](https://github.com/MAD-Producer/Kilonova/issues/226)。基线：main `b0dfb416`；独立分支：`codex/effect-search-feedback`。

## 范围与重复检查

实施前检查本地分支/工作区、全部开放 PR、最近合并记录和 #226 timeline。当时开放的 #415（着色器错误身份）与 #416（节点预览预算）不交付本功能。旧 `fix/desktop-property-followups` 已有搜索基础，主线已有同样的单次添加、重复键和窗口归属保护；本次复用这些实现，不重新开发搜索或插件命令。

| 项目 | 分类与结果 |
| --- | --- |
| ES-01 成功反馈 | 新功能：只对成功返回的 add 快照比较新增实例，显示效果名、主目标及实际目标总数。保持多选；锁定图层沿既有选择规则排除。搜索、效果库、菜单沿同一个 Plugin 动作处理。 |
| ES-02 定位 | 新功能：显示效果控件，清除该宿主的属性筛选/添加选择模式，展开新实例；沿既有高度计算只滚动一次，并使用原有 selected 颜色标识标题。连续搜索期间不请求切换原生窗口焦点。 |
| ES-03 连续搜索 | 新功能：每次新打开默认单次；原生复选框或搜索内 Tab 明确开启连续模式，保留查询与结果选择；每个 Enter/点击只发出一个既有插件动作。关闭、Esc、无目标、空结果不编辑工程。 |
| ES-04 输入与事务 | 既有边界复用并复验：主窗/浮窗归属、无关窗口命令、失焦关闭、IME 确认及重复键过滤；批量添加仍由宿主执行一个原子事务，不修改历史协议或效果 ID 规则。 |
| ES-05 失败与恢复 | 新反馈、既有观测：失败保留原工程和原始 Err，界面显示失败原因；未知效果、锁定/缺失目标的失败后可正常添加。没有新增重试、吞错、线程超时或返回值变换。 |
| ES-06 默认日志 | 既有观测实际复验：真实失败在默认 Info 文件中有 plugin start，Error 同时进入文件和 stderr，stdout 不含日志或错误原因。健康 Present 日志策略未改。 |

## 验证证据

工作区：`E:/Dev/aem/.worktrees/effect-search-feedback`。本地使用锁文件、离线依赖和已有可移植 MSVC；无新增依赖、资产、格式版本或外部实现。

| 检查 | 实际结果 / 证据 |
| --- | --- |
| 编译 | `cargo check -p motion-desktop --locked --offline -j2` 通过。 |
| 搜索相关回归 | `cargo test -p motion-desktop --locked --offline -j2 effect_search -- --test-threads=1`：11 通过，0 失败。覆盖四组语言/窗口的实际宿主、多图层单步撤销/重做、保存重新加载、长效果栈滚动与结果参数命中；记录 `.tools/effect-search-tests.log`。 |
| 失败与恢复 | `effect_add_failure_keeps_project_and_original_error_then_recovers`：实际执行通过；不使用成功 mock。 |
| 原生 GPU 截图及日志 | `effect_search_native_pixels_and_default_failure_sinks` 通过。两个真实隐藏 HWND 分别绑定主/浮窗 surface；中英文、900×600/1600×1000、400×300/220×180、1.0/1.5 渲染比例，共 16 张 PNG；像素尺寸/可见文字有断言。默认日志的真实未知效果失败、双 Error 信道和 stdout 隔离通过。 |
| 来源与样式 | `tools/validate_provenance.py`、`ci_desktop.verify_style_consumers()`、`git diff --check` 通过。所有新增布局数值先进入 `dark.json`；新复选框复用原生控件。 |
| 完整桌面/UI 与跨平台 CI | 提交时另行记录运行结果；不将编译成功或局部通过推断为全量验收。 |

原生证据位于 `artifacts/effect-search/`：16 张 PNG、`receipt.json`、`motion-studio.log`、`stderr.log` 和 `stdout.log`。已目检主窗英文和窄浮窗截图：新增复选框与结果同行区域不重叠，窄窗提示裁剪在可用宽度内，目标效果标题使用选中态。后续目检记录见 PR。渲染比例是受控 GPU 比例；收据另记真实 OS scale，不冒充物理显示器 DPI 切换、系统 IME 服务或人工鼠标验收。Windows Vulkan loader 输出了本机缺失的 Epic overlay 清单告警，未隐藏；它与效果请求的两个失败记录分别保留。

全量构建曾因 E 盘无剩余空间中断，不能计为测试失败或通过。增量缓存和已生成符号文件迁移到 `C:/Users/nova/.cache/motion-studio-incremental-20261022/` 保留，未删除源码或其他分支修改；后续使用关闭增量/省略本地链接符号的验证方式，不更改仓库编译配置或测试断言。

## DESIGN §0.7 逐项处置

以下均为本次改动的处置，保留不等于确认历史缺陷仍在当前主线，也不等于完成全规范整改。

| 基线项目 | 处置与原因 |
| --- | --- |
| path_controls 六处 y 步进 | 保留：未编辑路径属性行。 |
| path_controls 双列分割 | 保留：不属于搜索与应用反馈。 |
| path_controls 按钮高度族 | 保留：没有改变已有按钮命中区。 |
| path_controls 自绘数值场 | 保留：未编辑数值场。 |
| path_controls 全宽布尔控件 | 保留：未编辑路径布尔属性；新连续模式使用原生 Checkbox。 |
| effect_controls 行步进族 | 保留：参数行布局未改；定位复用现有高度计算。 |
| effect_controls 布尔按钮 | 保留：既有参数控件未改。 |
| effect_controls 枚举按钮 | 保留：既有枚举控件未改。 |
| effect_controls 手工 hover alpha | 保留：不修改该参数 hover 站点；搜索结果背景读取已有 selected token。 |
| effect_controls 层级缩进 | 保留：未增加或重排行；展开沿既有状态。 |
| timeline 11 列坐标 | 保留：保持兼容坐标与命中区。 |
| timeline snap guide 颜色 | 保留：未修改时间轴绘制或 token。 |

§12 限定验收：本次搜索/成功定位的画面、输入保护及实际事务已列证；没有进行全编辑器性能基线、全部属性样式整改或 macOS/Linux 人工原生验收。跨平台自动检查以 PR 页面为准。
