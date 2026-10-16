# 桌面 UI 阶段交付与验收边界

本草稿整理 `feat/desktop-ui-interaction` 中已实现的桌面 UI，依赖尚未合并的 [PR #302](https://github.com/MAD-Producer/motion-studio/pull/302)。差异以 #302 的分支为基线，避免重复审阅时间轴和曲线布局的前置改动；#302 合并后需将目标分支改回 `main` 并重新验证。

本文件不是 DESIGN.md 全部通过的声明。下表中的 `fixed` 只覆盖列出的实现与证据；未验证的原生窗口行为、显示比例和视觉状态仍保留为合并前待办。

## 本次范围

- 蒙版归入图层属性，蒙版路径和数值动画接入共享时间轴；兼容读取旧独立面板 ID。
- 矢量内容树、填充、描边、修剪路径、中继器及其已有关键帧能力接入属性和时间轴。
- 效果参数与矢量颜色使用共享色板，RGBA 通道复用数值编辑事务。
- 信息、预览控制和字符面板重新排版。可用数据和命令保留真实连接，未支持项维持禁用态。
- 延续原生 PaintList、窗口输入、主题、工程与历史模型。矢量参数命令扩展已存在于本分支，不能将本 PR 描述为纯视觉改动。
- 新增中英文辅助面板截图及窄面板截图；字符截图创建真实测试图层后撤销，并验证工程恢复。原生渲染按真实像素比例裁切，避免将面板拉伸到主窗口大小。

网站、macOS 打包脚本、本地研究资料、构建目录和 agent 记忆不属于此 PR。

## DESIGN.md §0.7 逐项核对

旧行号以 DESIGN.md 的审计位置为锚点；代码已迁移时以下列文件与函数为准。

| 基线项 | 当前实现与证据位置 | 状态 |
| --- | --- | --- |
| `path_controls.rs` 六处 30/34/36/30/28/28 行步长 | 蒙版行移至 `path_controls/mask_inspector.rs`；矢量内容行移至 `path_controls/groups.rs`，使用 `row.height`、`padding` | fixed；完整视觉状态矩阵仍待验收 |
| `path_controls.rs` 两列固定间距 | `mask_inspector::property_row` 与内容树参数布局复用样式尺寸并约束可用宽度；`shape_content_geometry_tree_and_fields_stay_inside_narrow_and_wide_panels` 覆盖窄宽面板 | fixed；系统 DPI 手动验收待完成 |
| `path_controls.rs` 控件高度混用 | 新的蒙版/内容属性使用行高、字段高度、图标槽及复选框；动作与属性区分 | fixed；旧路径绘制中的其他尺寸未作整体清理 |
| 蒙版数字自绘背景 | 新蒙版字段通过 `numeric::Field` / `numeric::passive` 接入 `field.number`，保留拖动、编辑和历史 | fixed；对应输入/取消测试及原生截图见验证记录 |
| `Mask enabled` 全宽按钮 | `mask_inspector.rs` 使用 Checkbox，保留启用状态命令 | fixed；不应将效果头的 `fx` 按钮误计为同时修复 |
| `effect_controls.rs` 30/26/34 行高及步长 | `render_mode` 仍有固定布局；参数标题与值仍可能分行 | pending verification；需单独压缩为同行属性，并验证滚动、弹层和命中范围，阻塞就绪 |
| 效果布尔值 `fx` / `—` | 参数布尔值已用 Checkbox，效果头启用状态仍为 `fx` / `—` 按钮 | pending verification；头部开关尚未满足设计规范，阻塞就绪 |
| 效果枚举全宽按钮 | 枚举已改 Dropdown 并保留选择/模态路由，但仍使用较宽参数布局 | pending verification；紧凑布局与完整状态验收未完成 |
| 效果悬停 `.with_alpha(0.2)` | 本次改为解析 `row` 的 `Hovered` 状态，使用现有 `$colors.hover`；没有新增颜色 token | fixed；其他 `.with_alpha` 用法仍需逐一审计 |
| 效果树层级缺失 | 效果折叠与参数虽有缩进，但仍是固定坐标，未完成可验证的 depth 布局 | pending verification；阻塞就绪 |
| 时间轴 11 列坐标 | 保留既有列坐标与命中边界；本 PR 新增的蒙版、矢量属性行在名称区复用共享布局 | retained with reason：兼容基线，不做无差别主题重构 |
| 时间轴黄色吸附线 | 保留 `timeline.rs` 中既有 `Color::rgb(255, 210, 60)` | retained with reason：DESIGN.md 明确要求保留现状，后续先定义 token 再改反馈 |

## 额外发现与合并阻塞项

- `auxiliary.rs` 的新版面板仍含固定尺寸、步长和重置文字的透明度设置；必须迁移到语义 token，并检查窄面板的填充色文字和预览控件裁切。截图只能说明当前状态，不能代替此项修复。
- `path_controls.rs` 的绘制草稿颜色、`effect_controls.rs` 的其他透明度和固定尺寸尚未清理；不能声称 `dark.json` 已经是实现中所有值的唯一来源。
- 缺少原生 Windows 100% / 150% / 200% DPI 与最小 900×600 窗口的完整交互验收；逻辑坐标测试不等价于系统缩放验收。
- 8×7 控件状态矩阵、长名称、键盘焦点与 IME、原生失焦/离窗取消、五种工作区布局，以及 Info 性能数据与基线比较仍需完成。
- 系统 Reduced Motion 接入未在本轮确认。

## 后端能力边界

详细缺口与待评审的接口建议见 [辅助面板后端需求](../desktop-auxiliary-backend.md)。

| 面板 | 已接入 | 仍未实现/未接入 |
| --- | --- | --- |
| 信息 | 图层名称、时间码、工程摘要；有性能数据时显示现有计数 | 合成光标 X/Y、像素 RGBA 探针 |
| 预览控制 | 首帧、播放/暂停、末帧、现有画质模式 | 快捷键选择、视频/音频开关、缓存后播放；需分别核实真实音频和缓存能力 |
| 字符 | 字体名读取、填充颜色与 RGBA 编辑、新建文字 | 字体修改、样式、字号、字距、行距、描边、段落对齐、重置 |

未实现项不能因版式已经出现而写成“功能完成”。后端接入应分项提交，包含旧工程兼容、排版失效、撤销和会话设置验证。

## 复现与验收

Windows 需先初始化 MSVC 开发环境；本机使用本地 `.tools/msvc-portable/env.ps1`，该脚本不随 PR 分发，其他机器可使用 MSVC 开发终端。资源受限机器采用单任务编译。

```powershell
cargo build --locked -j 1 -p motion-desktop --bin motion-studio
cargo test --locked -j 1 -p motion-desktop -p motion-ui -p motion-host -p motion-desktop-media -p motion-core -- --test-threads=1
python tools/generate_desktop_icons.py --check
python tools/validate_provenance.py
python tools/ci_desktop.py --binary target/debug/motion-studio.exe --output artifacts/ui-handoff-20261016/native
```

本次 Windows 本地验证结果：

| 检查 | 结果 |
| --- | --- |
| 相关五个 Rust 包测试 | 628 通过，2 忽略；忽略项为手动性能测量和真实音频设备 |
| 最新桌面程序单独补跑 | 310 通过，1 忽略；覆盖效果 RGBA、蒙版/矢量键操作、取消、撤销、主窗/浮窗路由 |
| 干净工作树的 Python CI 工具测试 | 38 项运行，1 项因未配置效果验证器而跳过，其余通过 |
| 原生窗口验收 | 中英文、节点真实渲染、共享 MCP、样式覆盖及全部 20 个当前可用浮窗通过；旧独立蒙版面板已退役 |
| 辅助面板截图 | 每种语言 15 张设计截图，包含 360×520 和 220×360 辅助面板；比例检查与测试工程恢复检查通过 |
| 来源、54 个图标、样式上下文静态检查、差异空白 | 通过；静态检查不代表所有硬编码尺寸均已清除 |
| 格式 | 新增截图模块通过 rustfmt 检查；没有将全仓库格式合规记为通过 |

初次高并行编译耗尽本机内存，最终使用 `-j 1` 完成。原工作目录的工具清单检查受无关未跟踪 `tools/package_macos.py` 影响，因此 Python 工具套件在保留相同受审改动的干净工作树重跑并通过；没有删除或纳入该脚本。

原始日志与全部 PNG 在本地 `artifacts/ui-handoff-20261016/`。CI 的 Android、Linux、macOS、可选 FFmpeg 构建结果另以对应工作流为准。

### 当前截图

以下为本仓库临时验收工程的真实原生渲染；面板拼接只并排排列 PNG，没有补画控件。窄面板截图中字符色值的省略等问题仍属上面的待办。性能计数随捕获步骤变化，本组截图不能替代性能基线对照。

![整体工作区](screenshots/desktop-ui-workspace.png)

![三个辅助面板，单面板 360×520](screenshots/desktop-ui-panels.png)

![三个辅助面板，单面板 220×360](screenshots/desktop-ui-panels-narrow.png)

下面两张来自样式覆盖验收，行高为 32，强调色也由覆盖样式指定；用于验证语义样式能传播到蒙版和矢量内容树。

![蒙版属性与样式覆盖](screenshots/desktop-ui-masks.png)

![矢量内容树与样式覆盖](screenshots/desktop-ui-shapes.png)

## 资产与来源

本 PR 沿用 Motion Studio 原有原生 UI 和图标体系。新增对齐等图标为本项目原创补充，来源标识在 `desktop/ui/icons/manifest.json`；图标总数为 54。无新增依赖或第三方素材，既有字体与第三方组件继续使用 `ATTRIBUTION.md` 中的来源与许可记录。

本文件中的截图由临时验收工程和本仓库原生 GPU 渲染生成，不包含第三方产品截图或产品资产。没有检查 Adobe 二进制、字符串表或资源段。
