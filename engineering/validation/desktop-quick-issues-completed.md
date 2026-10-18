# 桌面六项 issue 完成记录

日期：2026-10-18（Asia/Tokyo）

基线：`origin/main` / `018ca860ef9d4ffc56831fb17a6d6a044bc902e1`

分支：`fix/desktop-quick-issues`

工作树：`E:/Dev/aem/.tools/desktop-quick-issues`

范围：[#201](https://github.com/MAD-Producer/Kilonova/issues/201)、[#233](https://github.com/MAD-Producer/Kilonova/issues/233)、[#242](https://github.com/MAD-Producer/Kilonova/issues/242)、[#156](https://github.com/MAD-Producer/Kilonova/issues/156)、[#136](https://github.com/MAD-Producer/Kilonova/issues/136)、[#206](https://github.com/MAD-Producer/Kilonova/issues/206)

## 交付范围

| Issue | 基线核验与最终行为 | 可复现证据 |
| --- | --- | --- |
| #201 图层重命名 | main 已有行内重命名、原生命令、保存与撤销实现；复用并运行原有验收，不重复另建编辑器 | `layer_actions::tests::inline_rename_reaches_the_real_host_in_main_and_floating_windows_and_both_languages`、`name_enter_commit_cancel_ime_and_outside_click_are_scoped`、`double_click_name_only_opens_for_unlocked_layers`、`context_change_or_removal_rejects_open_menu_and_name_draft`、`scoped_menu_edit_survives_save_reopen_and_one_step_undo_in_both_languages` |
| #233 钢笔断柄 | 新点放置时 Alt 仅修改出手柄；释放 Alt 恢复对称，再按 Alt 保留对侧；松开鼠标结束本点放置，后续无关拖动不再改写手柄；复用已提交节点的手柄计算 | `path_controls::pen_tests::pen_placement_alt_handles_commit_cancel_and_save_roundtrip`，覆盖矢量/蒙版、两种语言、坐标偏移、闭合单命令、取消、保存重开及撤销 |
| #242 表达式启停与用户补充要求 | 表达式不再有独立窗口入口；编辑区默认收起，已有表达式也不自动展开；右键属性选择“编辑表达式”或 Alt+秒表，在该属性下面打开一行源码与启用复选框；效果参数使用相同内联控制器；切换只改 enabled，保留 source/target/seed/profile；源码提交保持 disabled 状态；错误表达式可继续编辑 | `expressions::tests`：真实主窗/浮窗和中英文切换、效果参数右键菜单与精确 target、非法源码失败保持原工程、Enter 提交不启动图层重命名、IME、Esc 无历史；原生 `expression-inline-*` 截图；`legacy_expression_layouts_redirect_to_properties_without_touching_project_data` |
| #156 节点焦点 | Space 在 Nodes/NodeProperties/NodePreview 切换共享节点预览，一次按键只切换一次；允许无节点冲突的工程命令；节点文档的删除/复制/撤销和画布导航继续归节点；旧画布内部焦点不抢占当前时间轴播放 | `keyboard::node_scope_tests` 的命令可达性矩阵、主窗/浮窗/双语 Space 及陈旧画布焦点测试；详细规则见 `desktop/KEYBOARD-SHORTCUTS.md` |
| #136 文档漂移 | 清理已实现/规划的矛盾键位；记明标题栏拖动、Ctrl+滚轮、Alt+左键平移与作用域；同步布局文件名和 15 个图示例；将根 docs/design 明确为本地参考并共享排除规则；历史私有文档的旧计数不再作为产品依据 | `shortcuts::documentation_tests::default_binding_reference_matches_live_resolver` 枚举真实 Key、scope 和精确修饰键，核对 `desktop/DEFAULT-KEY-BINDINGS.md`，并检查示例数量和布局文档；CONTRIBUTING 记录生成命令与文档归属 |
| #206 时间轴缩放 | +/=、− 缩放，Ctrl+滚轮锚定鼠标所在时间，鼠标不在轨道时键盘缩放保持播放头位置；`;` 切换当前单帧与整段；片段与曲线独立记忆范围；缩放不进入工程历史，不变更选择 | `timeline_zoom::tests`：有限数值、锚点不漂移、长合成单帧、独立片段/曲线范围、主窗/浮窗真实键位与 Ctrl+滚轮、文本/IME/重复输入保护 |

表达式 Enter 提交源码；Esc 或关闭图标放弃未提交草稿并收起，关闭本身不删除表达式。移除表达式是单独的属性右键命令，可通过原有工程历史撤销。布局仍保留历史 ID 9 的反序列化能力，但隐藏原独立面板，旧停靠/浮窗请求重定向到属性；工作区五种预设均保留。

## 验证记录

统一构建目录：`E:/Dev/aem/.tools/observability-validation-build`。依赖使用锁文件和离线缓存，无新增依赖、资产或项目格式。未运行/声称 GitHub CI 或关闭远端 issue。

| 检查 | 结果与记录 |
| --- | --- |
| `cargo check -p motion-desktop --offline --locked -j2` | 通过；`.tools/quick-issues-check-accepted.log` |
| `cargo test -p motion-desktop --offline --locked -j2 -- --include-ignored --test-threads=2` | 404 项通过、0 失败、0 忽略；`.tools/quick-issues-tests-accepted.log`；不跳过 ignored，不修改原有断言 |
| `cargo test -p motion-ui --offline --locked -j2 -- --include-ignored --test-threads=2` | 82 项通过、0 失败、0 忽略（含集成测试，文档测试为 0）；`.tools/quick-issues-ui-tests-accepted.log` |
| `python tools/ci_desktop.py --binary .../debug/motion-studio.exe --output artifacts/quick-issues-inline` | 通过；中英文桌面 smoke、中英文 node smoke、共享 MCP 会话、两种语言的样式覆盖浮窗；四份 smoke 报告各有 28 张真实 GPU 截图且 `designCaptureProjectUntouched=true`，样式验收均渲染 19 个可用面板；`.tools/quick-issues-native-inline-r2.log` |
| `python -m unittest discover -s tools -p 'test_*.py'` | 54 项全部通过、无跳过；构建真实 `effect_tool` 后设置 MOTION_EFFECT_VALIDATOR，额外核验效果库的真实 schema/WGSL 加载、打包和检查；`.tools/quick-issues-python-accepted.log` |
| panels 样式扫描 | `verify_style_consumers()` 通过，无新增 `palette::` 或 `default_stylesheet(` |
| 来源检查 | `python tools/validate_provenance.py` 通过；只观察用户提供的交互示例，未复制截图、图标、文案或二进制资源 |
| diff 检查 | `git diff --check` / `git diff --cached --check` 通过 |

本机验收产物：`artifacts/quick-issues-inline/`（构建证据目录，不纳入版本控制）；包括 `smoke-en.json`、`smoke-zh.json`、`node-smoke-en.json`、`node-smoke-zh.json`、`mcp-ui-smoke.json`、`styled-panels-en.json`、`styled-panels-zh.json` 及引用的 PNG。已目检中英文表达式启用/停用的标准和窄时间轴截图；源码与复选框位于同一属性的子行。

本批代码提交：`a81cf79`（钢笔手柄）、`da42090`（节点路由与时间轴缩放）、`d25aa0e`（内联表达式）；文档与完成记录单独提交。

首轮完整桌面测试曾通过 398 项；增加文档与焦点守卫后通过 400 项。这两个中间结果不能替代包含内联重设计的最终验收。第一次原生验收与完整测试并行时超过 40 秒；后续内联截图夹具进入另一条通用 Timeline 夹具导致工程恢复检查失败，已将两种夹具分开；保留恢复断言，最终串行原生验收完整通过。

## DESIGN §0.7 基线逐项处置

以下位置沿用规范中的基线定位，不是本提交的最新行号。保留表示本批没有实施该条视觉整改，不据此宣称整个 DESIGN 已符合。

| 基线位置 | Before / 本批 After | 证据 / 状态 |
| --- | --- | --- |
| path_controls 六处 y 步进 | 原基线的 30/34/36 等步进；本批仅修改钢笔输入生命周期与手柄数学 | 保留：路径属性行布局不属于这六项功能范围；交互回归见 pen_tests |
| path_controls 两列分割 | 原硬编码分列；本批未改 | 保留：避免扩大属性布局整改范围，窄面板系统整改另行验证 |
| path_controls 按钮高度族 | 原混合高度；本批未改 | 保留：本批没有调整既有点击容差 |
| path_controls 自绘数值场 | 原 SUNKEN/ACCENT 数值绘制；本批未改 | 保留：本批不重写路径数值控件 |
| path_controls 布尔全宽按钮 | 原基线布尔控件；本批未改 | 保留：本批没有修改该属性行 |
| effect_controls 行步进族 | 原基线行密度；新增表达式子行只读 row.height | 保留：不宣称完成全文件密度整改；新子行由原生截图和效果回归覆盖 |
| effect_controls 布尔按钮 | 原基线布尔语义；本批未重写既有参数控件 | 保留：新表达式开关使用现有 Checkbox |
| effect_controls 枚举按钮 | 原基线枚举语义；本批未重写既有枚举控件 | 保留：内联表达式没有新增枚举按钮 |
| effect_controls 手动 hover alpha | 原基线手动 hover；本批未改 | 保留：表达式字段复用 field.number，不新增色值 |
| effect_controls 层级 | 原基线层级；表达式只在当前参数后增加内容子行 | 保留：未进行全树层级重排 |
| timeline 11 列坐标 | 原兼容坐标；本批保持 | 保留：遵守规范兼容基线，不借缩放/表达式任务重构列；600 宽截图仍显示既有工具条拥挤，非本批新增承诺的最小窗口尺寸 |
| timeline snap guide 硬编码颜色 | 原兼容颜色；本批保持 | 保留：没有以主题修改扩大范围 |

## 验证边界

- Windows 原生窗口与 GPU 验收已运行；未声称 macOS/Linux、全部真实系统 DPI 档位或所有显示器组合均验收完成。已有布局未重绘；900px 标准时间轴及 600px 压力截图与 row.height=32 样式浮窗可回看。更窄旧工具条的拥挤仍保留为既有布局边界。
- 原生测试覆盖五种工作区、19 个可用面板及原有蒙版/路径/效果/属性截图；不把这次功能交付当作 DESIGN 12 项视觉基线的全部整改。
- Info 统计正常产出；本次没有采集同负载的长期帧时对照，不宣称性能改善。只有用户主动打开的表达式编辑区增加子行和控件，没有给逐帧渲染增加日志。
- 本地默认构建未启用 FFmpeg；导出命令可达性测试按编译特性区分可用与既有不可用状态，实际视频编码链不属于本批新增实现。
- 保持 PR #321 的时间尺 Seek 修复独立，本批未修改时间尺拖动/Seek 算法；未处理 PR #322 的工程打开提示。
- 完成记录只证明该分支本地实现和上述验收，不等同于远端 issue 已关闭或主分支已合并。

## 后续：效果搜索命名独立化

- 将快捷键常量、状态类型、字段、模块与绘制入口统一为 `EFFECT_SEARCH`、`EffectSearch`、`effect_search` 和 `render_effect_search`，产品文档统一称为“效果搜索”，同步设计说明和观测台账中的源码路径
- 保留 Ctrl+空格及原有输入路由；未加入备用快捷键、原生监听或动态挂载，也未改动布局、样式、命令载荷和工程格式
- 核对六个 Rust 文件的完整内容，确认差异仅为对应标识符及注释替换；搜索原命名无残留，保留原有测试断言
- 重新运行 `cargo check -p motion-desktop --offline --locked -j2` 和完整桌面测试（含 ignored）：404 项通过、0 失败、0 忽略；日志为 `.tools/effect-search-rename-check.log`、`.tools/effect-search-rename-tests.log`
- panels 样式扫描、来源校验和 diff 检查通过；DESIGN §0.7 各项延续上表处置，本补丁仅重命名，未再次运行原生截图或增加视觉验收结论

## 后续：局部变量与 issue 命名同步

- 补齐效果搜索内部的局部变量、参数、闭包和测试名称：状态变量使用 `search`，测试夹具使用 `search_state`，相关测试使用 `effect_search` 命名；继续保留 Ctrl+空格和既有行为
- 更新 GitHub issue [#107](https://github.com/MAD-Producer/Kilonova/issues/107)、[#134](https://github.com/MAD-Producer/Kilonova/issues/134)、[#155](https://github.com/MAD-Producer/Kilonova/issues/155)、[#158](https://github.com/MAD-Producer/Kilonova/issues/158)、[#163](https://github.com/MAD-Producer/Kilonova/issues/163)、[#198](https://github.com/MAD-Producer/Kilonova/issues/198)、[#226](https://github.com/MAD-Producer/Kilonova/issues/226) 中的功能名称或命名说明，并同步 #226 标题；保留固定旧提交的摘录和链接作为历史证据，未改变 issue 状态与验收勾选
- issue 更新逐条核对原标题与正文，写入后核验返回内容；原文、补丁和结果保存在根工作目录 `.tools/effect-search-issue-updates/`，命名说明明确记录此分支尚未合并
- 重新运行编译检查及含 ignored 的完整桌面测试：404 项通过、0 失败、0 忽略；日志为 `.tools/effect-search-variables-check.log`、`.tools/effect-search-variables-tests.log`
- 根工作目录原在旧分支，先前命名提交位于独立工作目录；交付时将根工作目录切到 `fix/desktop-quick-issues`，保留旧分支及用户原有未跟踪文件，不以独立工作目录的结果冒充根目录已更新
