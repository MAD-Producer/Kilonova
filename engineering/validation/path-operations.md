# 作者路径菜单与几何复制

关联 #269，复用 #204 的独立路径剪贴板，属于同一分支与 PR #484。实现前核对了现有工作树和分支，其他活动工作集中于效果库、精度与渲染资源；本改动没有修改其他工作树或审查其他 PR。

## 行为与转换规则

钢笔／蒙版工具下，在作者路径的顶点上右击，可设为首顶点、复制为另一种路径，以及使用路径剪贴板。空白处右击提供粘贴；没有顶点目标时首顶点和转换禁用。参数形状的尺寸角点不是作者顶点，不提供首顶点。菜单跟随所属主窗口／合成浮窗，其他窗口的纯重绘不会关闭菜单；失焦、Esc、点外部、切换目标或工程身份变化会取消，取消帧不下传给画布或快捷键。

- 首顶点只旋转闭合路径的节点数组，所有节点 ID、相对手柄和几何 Track 原样保留，不在播放头采样。开放路径及不足三个点的路径明确拒绝。向量组按作者几何 item 定位；蒙版按 mask/path 定位。
- **复制为蒙版路径**保留源形状，在同一图层新增默认 Add 蒙版。形状中心坐标加半个源尺寸，换成蒙版左上原点；嵌套组的静态旋转、缩放、锚点、倾斜和几何位置按现有组矩阵合成。锚点按完整仿射矩阵变换，相对手柄只乘线性部分。基值和所有关键帧均转换，时间、插值和闭合性保留；不复制填充、描边、修剪、重复器、效果或组不透明度。
- **复制为形状路径**保留源蒙版和图层，在相邻的新矢量图层复制作者几何，使用默认白色填充；坐标减半个源尺寸。新层沿用源层尺寸、变换轨道、父级、3D、旋转顺序、采样设置与时间范围／偏移，节点时间无需移动。源蒙版的羽化、扩张、反相、混合方式及不透明度不是路径几何，不转为形状外观；新层不继承效果、其他蒙版或音频。
- 动画组变换／几何位置以及源层启用的变换表达式无法在另一表示中无损保留，明确拒绝；不采样烘焙、不重试、不静默丢弃。参数几何需先转为作者路径。现有坐标、节点、蒙版和图层限额照常执行。每次复制和首顶点调整各为一个历史事务，源不删除。

开放路径不被强制闭合；按照现有蒙版语义，开放蒙版不参与遮罩。

## 审计记录

| 项目 | 分类 | 处置与边界 |
| --- | --- | --- |
| PO-01 | 功能 | 新 Vector/Mask 首顶点 action；CopyPathAs 复用命令注册和全工程事务。复制几何不引入资源文件。失败保留操作名、对象、路径、节点和原始错误链。 |
| PO-02 | 新观察入口、复用既有观察 | PathOperation 接入现有非 Present 的 Info 开始、在途状态、计时与 panic 回复通道。未增加线程、超时、重启、重试、热路径日志或观察计数器。 |
| PO-03 | 正常分支 | 执行前校验 root、composition、document_generation、revision；过期返回当前快照和 superseded 状态，不能按复用 ID 编辑新工程。没有目标／空剪贴板和取消是普通分支。 |
| PO-04 | 交互／样式 | 新菜单使用 context_menu、context_menu.row、context.row.height、context.menu.width 和 padding.panel；保持原命中半径。短窗口可滚动到其余命令；菜单不引入颜色／尺寸 token 或树行变更。 |
| PO-05 | 来源 | 已核对 PROVENANCE.md、ATTRIBUTION.md、NOTICE。没有新增依赖、第三方资产、兼容名称、工程格式或渲染语义，几何与 UI 实现独立编写。 |
| PO-06 | 既有观察的实际验证 | 默认 INFO 子进程实际执行缺失首顶点 node=999、复制源 path=999 的失败，文件与 Error stderr 都保留操作／对象／路径和原始原因，stdout 不泄露；随后正常复制、单步撤销验证恢复。沿用工作器既有在途／阻塞和 panic 机制，本阶段没有在每个路径函数内部另注入永久阻塞。 |
| PO-07 | 功能修正 | 原生测试发现主窗属性缓存更新调用 Controls.cancel，取消了浮窗有效菜单。缓存重绑现在只保留身份、图层和工具均匹配的路径菜单，不改变真正的失焦／上下文取消；真实跨窗重绘与菜单点击已验证。 |
| PO-08 | 实际验证与剩余 | b1d91f4b 的四项必需 CI 与依赖审计通过；Windows 实际键鼠通过完整 window_event 路由，在英文主窗和中文浮窗验证双向复制、独立路径剪贴板与历史。独立 Debug 运行目录验证启动与保存重开；官方发行包仍待单独验收。不以这些结果宣称所有操作系统的 IME 或文本服务已验证。 |

## 验证记录

`E:/Dev/aem/artifacts/pr-ci-followups/path-conversion-core-second.log`：首顶点三项和转换三项通过。验证动画路径全部字段、分数帧几何、非平凡静态组基变换与实际组采样结果、开放／闭合、源保留、保存重开及单步撤销。缺失、锁定、蒙版限额、坐标越界和动画组拒绝均保留完整工程与原错误链。最初新测试误用了 Engine 构造名和旋转类型，按真实 API 修正，原失败日志保留，未改生产断言。

桌面测试夹具最初缺少 Path 的借用，测试探针可见范围也不足，编译失败日志保留；修正仅涉及新增夹具与仅测试编译的探针，没有放宽既有断言。

`path-operations-core-final.log`：核心 26 项通过（首顶点 3、转换 4、既有路径剪贴板 8、蒙版 3、矢量组 8）。额外验证非矢量源上的蒙版、父级、3D、图层动画与时间偏移沿用、启用变换表达式时拒绝，以及 CopyPathAs 注册／解析入口。

`path-context-controls-regression.log`：路径控件 85 项通过，25.82 秒；`path-context-clipboard-regression.log`：剪贴板 82 项通过，184.65 秒；两集有重叠，不合计为独立测试总数。新增组件测试覆盖中英文标签、闭合／锁定禁用、单次选择、工程身份／选择变化、失焦／Esc／其他窗口真实输入的取消、其他窗口无输入重绘保留，以及 88px 短窗口滚动到粘贴。第一轮短窗口新夹具把鼠标放在弹窗外，现按真实可见行定位滚轮；原始失败保留，断言未放宽。

`path-operation-bridge-first.log` 两项通过；默认信道证据位于 `path-operation-errors-final/`，实际跨 root／合成／generation／revision 的旧请求均先拒绝，工程与历史不变。没有以返回 Ok 的模拟替代失败验证。

`path-context-native-second.log` 原生测试通过，21.47 秒；`path-context-native-second/` 包含 18 张实际 HWND／wgpu 截图：四种中英文／主窗浮窗组合下的两个右键菜单、复制出的形状和跨窗粘贴，另含两个主窗 Edit 菜单。英文 1280×800／100%，中文 1920×1200／150%。逐项执行菜单动作后核对源保留、首顶点 ID、单步撤销、保存重开；其他窗口重绘不能关闭菜单。第一轮浮窗实际取消缺陷及日志保留；没有通过删除跨窗步骤或改变动作断言通过验收。

截图验证菜单与选择轮廓，不证明最终填充／渲染像素。既有英文路径帮助文本的字体显示问题与窄属性值截断保留，不宣称整体 UI 已符合 DESIGN。原始 stderr 中本机 Epic EOSOverlay Vulkan JSON 缺失记录保留，未抑制。

`path-context-test-build-fourth.log` 测试程序编译成功；SHA-256 `f8bb19976414fcf2ad31cca79a884d690bc8b91419ac2dd003d529ed576013a9`。`path-context-bin-final.log` 带 FFmpeg 的实际桌面程序编译成功，54.15 秒。新模块定向 rustfmt、样式消费者和 diff 空白检查通过；既有 warnings 保留。随后只整理核心新增闭包缩进，不改变行为。

缩进整理后再次编译实际桌面程序通过（`path-context-bin-formatted.log`，1 分 07 秒），exe SHA-256 `dbd2c118c62f74a4a48c2287886e8f3048265bccf101f53f0abf30d8ab3db9d9`。带 FFmpeg 测试程序再次实际执行插件取消用例，通过（`path-context-plugin-cancel-final.log`，0.55 秒）。4facd082 的远端 Linux 与 Rust/GPU 检查通过；该记录不冒充本阶段后续提交的 CI 验证。

PR #484 的首轮 Linux job 120910708116 在已有插件取消菜单测试失败：新增剪贴板项后仍点第三行。4facd082 保留原有中英文六种状态断言，在原小窗口中实际滚动到取消项再点击。本地 `path-clipboard-menu-ci-fix-second.log` 一项通过。原始 Linux 日志保留于 path-clipboard-linux-job.log；没有修改生产菜单布局或禁用测试。

### Windows 系统输入与独立运行目录

`E:/Dev/aem/artifacts/pr-ci-followups/path-os-input-driver.py` 从外部启动实际 `motion-studio.exe --mcp-ui`，通过进程 ID 枚举窗口，只有自有 HWND 成功取得系统前台焦点后才发送 Win32 键鼠事件。事件进入 winit／生产 window_event；没有直接构造 Input、注入 Shell 动作或通过 MCP 代替待测的编辑动作。MCP 只读取实际工程状态和保存，截图来自 ImageGrab 的实际窗口。测试工程、布局、LOCALAPPDATA 全部隔离；退出后恢复先前窗口焦点。新增首顶点与复制操作使用默认 INFO，stdout 的每行均为 JSON-RPC，常规 start 不写 stderr。本机 Vulkan loader 错误原文仍保留。

最终自包含流程在 `path-os-main-en-final/`（英文主窗）和 `path-os-float-zh-final/`（中文合成浮窗）实际退出 0；两者都是系统 DPI 96／100%，不冒称修改系统 DPI。此前组件输入测试已另行覆盖中文 150%。对应步骤为 `path-os-main-final-steps.json` 与 `path-os-float-final-steps.json`，核验脚本 `verify-path-os-input.py` 已执行通过，结果为 `path-os-input-summary.json`。每个关键动作都保留独立的 reply-N.json，而非从截图猜测成功：

- 设首顶点后只将节点顺序从 `[10,11,12,13]` 旋转为 `[12,13,10,11]`；完整工程其余字段不变，revision 只加一。一次实际 Ctrl+Z 恢复完整原工程。
- 复制为蒙版后原形状原样保留，新增蒙版每个锚点增加半尺寸 `[16,16]`。从属性面板选择该蒙版的钢笔入口，再在同一顶点右击复制为形状；新增第三层的节点轨道、闭合性正确，源蒙版及源层完全不变。两次 Ctrl+Z 恢复原来的两个图层。
- 四点追加选择、Ctrl+C、切换目标层、Ctrl+V 使用独立路径缓存，得到新路径／节点 ID 和相同几何。一次 Ctrl+Z 撤销，Ctrl+Shift+Z 重做；重新选择四点并 Ctrl+X 后只清空目标作者路径节点，保留源层，Ctrl+Z 恢复完整粘贴结果。空作者路径容器是现有剪切语义，不等同于删除图层。
- 通过实际程序保存后正常关闭，再以独立的 `--mcp` 进程打开同一工程；完整 project 与保存前实际状态相等。

独立目录 `path-os-input/local-debug-runtime/` 包含上文同一 SHA-256 的 FFmpeg Debug 程序与其已用于编译的共享 DLL；清掉 FFmpeg 的 PATH，仅保留 Windows 系统目录后完成上述窗口运行与重新打开。`runtime-manifest.json` 记录所有文件的 SHA-256。该结果证明独立 Debug 目录的原生加载，不等于官方 Release ZIP、视频解码或各平台打包验收。

外部驱动探索原件均保留：第一轮缺少 MCP initialized 通知，被协议正确拒绝；人工分阶段探测的第二轮在后续操作前进程已终止，不列为通过；第三轮修改后未先保存，关闭等待超时，最终流程显式保存后实际退出 0。第一次完整英文探索未先在属性面板选中蒙版，且误把产品的 Ctrl+Y（新建纯色）当重做，状态记录如实显示这些操作，未算入最终通过。最终步骤改用真实蒙版钢笔入口与现有 Ctrl+Shift+Z，不改生产快捷键、返回值、等待期限或已有断言来通过验证。

远端 run [40112038149](https://github.com/MAD-Producer/Kilonova/actions/runs/40112038149) 对 b1d91f4b 的 Linux、Windows、macOS Intel、Rust/GPU 及依赖审计全部通过。PR 事件按现有工作流跳过 FFmpeg Release 构建、打包与归档视频验证，故不能用这次全绿冒称发行包已验收；需要另行触发本分支的完整验证。

## 完整打包验证发现的原生截图夹具问题

完整手动 run `40150186519` 的 Linux Release 构建、打包和视频验证已执行，随后原生窗口验收失败于 `timeline dropdown capture must open without editing the project`。保留原件 `path-package-linux-job.log`。本地以既有独立 Debug 程序复现同一失败，记录在 `path-smoke-before.log`，不是仅按 CI 日志猜测。

原因是 `shell/design_acceptance.rs` 的时间轴菜单截图仍按旧的 64px 表头定位，遗漏工作范围栏；改为读取 `timeline.header.base.height`、`timeline.work_area.height` 和行高计算目标中心。不改生产布局、菜单逻辑、返回值或等待期限；保留无编辑动作且菜单必须打开的双重断言，并补充菜单名、点击位置和动作数量的失败上下文。

修正后带 FFmpeg 的实际桌面程序重新编译，中英文 `--smoke` 均退出 0，每种语言生成 46 张截图；`tools/ci_desktop.py::verify_design_screens` 验证通过，`designCaptureProjectUntouched` 均为 true。证据为 `path-smoke-fixture-build.log`、`path-smoke-after-{en,zh}.json/.log` 及对应 PNG。系统 Vulkan 加载器对已移除 Epic Overlay 的错误原样保留，未通过清日志隐藏。该证据仍是本地 Debug 原生窗口验证，修复提交需要新的完整 Release 打包 run。

第二轮完整 run `40153937486` 的 Linux 已通过前面的普通原生验收，随后样式覆盖阶段暴露 `shell/acceptance.rs` 的浮动时间轴音频菜单仍右击旧坐标 y=105。32px 行高时该点落在工作范围栏，后续方向键没有菜单所有权；原件见 `path-package-linux-second.log`。同样改为按表头、工作范围栏和行高定位第一行中心；保留无编辑断言，并加强为每一步必须继续持有实际菜单，失败包含 step／point／动作数量。

`path-float-menu-build.log` 编译修正后的 FFmpeg 桌面程序。`path-style-manual/` 的中英文 `--panels-smoke` 均在原有 90 秒限制内退出 0，各验证 19 个原生浮窗，实际覆盖 32px 行高、12px 拖动阈值和 linear easing；`verify_design_screens` 均通过，完整工程保持不变。`styled-en.panel-4-audio-menu.png` 复核实际音频子菜单。未改生产逻辑、既有超时或重试。

本地整套 `ci_desktop.py` 另有未通过记录：Debug 的首个 `--smoke` 在持续生成 capture_ui 截图时触及既有 40 秒总限制，见 `path-full-desktop-validation.log` 和 `path-full-desktop-user/` 的默认日志。没有延长期限、删除步骤或把后续定向验证当成整脚本通过；完整 Release CI 仍待新提交验证。

## 嵌套表达式夹具的独立 CI 修正

`612f5754` 的 PR run `40157397456` / Rust and GPU job `121061597727` 在既有 `nested_expression_views_preserve_time_and_dimensions_without_copying_the_graph` 首帧原工程采样失败：对象 1 的 position 返回 `expression cancelled or execution budget exceeded`，尚未进入本批路径测试。原始输出保留在 `path-rust-gpu-third.log`。该消息没有区分编译、上下文准备和用户执行的耗时，因此不能据此断言平台根因。

夹具原来直接向 `Project.expressions` 插入源码，首次采样才编译。现在沿真实 `SetExpression` 命令建立输入；既有命令先编译、再启用原有采样预算。原来的四个帧、原／嵌套矩阵逐项相等和无图拷贝断言全部保留，另用不含表达式的独立几何参考检查 `x = 33 + frame / 3`，防止两侧同时忽略表达式时虚假通过。没有修改生产 JS、20ms 执行期限、100ms 整帧期限、取消、重试或 CI 配置。

本地诊断 `path-expression-fixture-probe.rs/.log` 的两条路径均成功，直接插入的首帧为 12ms，经命令编译／验证为 5ms、后续首帧为 2ms；**本地没有复现远端超时**，这只是输入准备的对照，不是根因或性能结论。`path-expression-compositions-fixed.log` 的合成组 10 项通过。两个真实负面对照分别把表达式改成 `value` 和移除 `time * 10`，均以 101 退出并在新增矩阵断言失败，原件为 `path-expression-negative-unevaluated.log`、`path-expression-negative-wrong-time.log`；未修改生产文件。

恢复源码后，`path-expression-regression-final.log` 的合成 10、表达式沙箱 7、表达式专项 9 项全部通过，包含真实无限循环预算拒绝、取消、原轨道不变和失败后健康恢复。诊断探针不纳入提交。新提交仍需远端完整门禁；冷导入工程的编译时延与全平台高负载期限不在本次夹具修正的证明范围内。

## DESIGN §0.7 基线逐项处置

以下十二项均保留，未以新增菜单的验收结果宣称它们已整改：

| 基线项 | 处置／原因 |
| --- | --- |
| path 六个纵向步进 | retained：未改属性行节奏。 |
| path 两列拆分 | retained：未改字段分栏。 |
| path 混合按钮高度 | retained：未改既有按钮与命中范围。 |
| path 自绘数字字段 | retained：未改数字编辑或显示。 |
| path 全宽布尔按钮 | retained：未改布尔控件。 |
| effect 行步进 | retained：未改效果行。 |
| effect 布尔文本按钮 | retained：未改效果控件。 |
| effect 全宽枚举按钮 | retained：未改枚举控件。 |
| effect 手工 hover alpha | retained：未改效果悬停绘制。 |
| effect 缺层级缩进 | retained：未改效果树。 |
| timeline 既有列坐标 | retained：兼容基线，未改时间轴。 |
| timeline 黄色吸附线 | retained：兼容基线，未改时间轴反馈。 |
