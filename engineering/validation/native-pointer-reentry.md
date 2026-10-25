# 原生窗口鼠标重入与图层剪贴板闭环

## 问题与功能修正

继续核验 #204 时，普通构建 `78b43ff0` 的中文原生时间轴浮窗出现实际输入失败：主窗 Edit 菜单复制、浮窗文本草稿编辑、连续两次图层粘贴及两次撤销之后，回到原先图层名称的相同鼠标位置点击，图层没有重新选中，Ctrl+X 不产生剪切。工程及最后呈现的修订均为 4，不是后台编辑未呈现或键盘重复分派。

原件保存在 `E:/Dev/aem/artifacts/pr-ci-followups/`。`clipboard-final-os-verification.log` 保留完整工程／修订断言失败；`clipboard-float-cut-observed/` 保留逐步截图及状态。临时诊断构建 `clipboard-float-press-probe/` 记录了最终点击的 `pressed=Some((Left, [-inf, -inf]))`，此时没有弹窗阻挡、没有待提交剪贴板工作，也没有实际选中对象。`clipboard-float-routing-probe/` 证明 Ctrl+X 已到达正确的 Timeline 键盘路由。没有把无选择时的正常不操作记为故障。

依赖源 `winit-0.30.13/src/platform_impl/windows/event_loop.rs` 的 Windows `WM_MOUSEMOVE` 处理在发出 CursorEntered 后，仍会按上次客户区物理坐标去重 CursorMoved。原宿主在 CursorLeft 后将命中坐标设为负无穷，忽略 CursorEntered，因此同坐标重入无法恢复。

`d3074562` 为每个原生窗口独立保留最后的物理坐标，在 Windows CursorEntered 时按当前窗口缩放恢复 MouseMoved。后续真正的移动仍覆盖缓存。CursorLeft 仍使控件坐标失效并取消手势；延迟释放继续发生在无效坐标上；重入只恢复位置，不恢复按下、释放或拖拽。未见过坐标的窗口没有恢复事件。macOS／Linux 的进入事件处理不变。

这是一项输入功能修正，不是新增错误观察，也没有改动剪贴板事务、渲染、样式、重试或等待策略。没有新增依赖、资产或工程格式。

## 证据与边界

| ID | 分类 | 处置 |
|---|---|---|
| NP-01 | 新功能修正 | Windows 原生主窗与浮窗各自缓存物理鼠标坐标；同坐标重入恢复命中，窗口之间不共享位置。 |
| NP-02 | 原有协议保护 | 保留 PointerLeft 的手势取消与无效命中；新进入不恢复旧按下。常规窗口删除会释放相应缓存，新窗口从空缓存开始。 |
| NP-03 | 正常分支 | 未知位置、无图层选择、空撤销历史仍是正常分支，不新增 Error、日志计数、IO 或重试。 |
| NP-04 | 既有观察 | 图层读／提交沿用默认 INFO start、在途状态、文件及 Error stderr 信道；本修正未增加或屏蔽故障，未扩大历史验证的证明范围。 |
| NP-05 | 实际回归 | 首轮 `clipboard-pointer-entry-native-tests.log`：原生 HWND／ActiveEventLoop 的已有测试通过，1 项，19.61s，包含新增的 100%／150%／200% 逻辑缩放、同坐标重入、延迟释放、后续移动及主浮窗隔离断言。测试显式提供 WindowEvent，不冒称真实系统 DPI 或 OS 键鼠服务。 |
| NP-06 | 普通构建实际输入 | `clipboard-pointer-entry-os-en-activated/` 与 `clipboard-pointer-entry-os-zh-activated/` 均通过原始完整工程／修订断言和独立进程重开；详见下表。 |
| NP-07 | 交付边界 | 当前修复 PR 的完整 CI 与合入仍待完成；#204 在合入前保持开放。原生主窗／浮窗的真实系统 DPI、真实 IME 服务及所有启动激活时序不由本组场景证明。 |

首轮带 FFmpeg 桌面构建通过，原件为 `clipboard-pointer-entry-build.log`。随后将分支快进到最新 main `302a15bf`，保留其他分支已合入的矢量列表／裁剪分配优化。`clipboard-pointer-entry-current-main-native.log` 的同一窗口事件回归再次通过，1 项，17.64s；当前基线测试编译耗时 4m13s，原有 22 项测试编译警告保留。普通构建 `clipboard-pointer-entry-current-main-build.log` 通过，1m32s，保留原有 21 项编译警告。没有将较早基线的结果直接计作当前 main 的结果。

所有临时输入／选择日志已从三个产品文件完整移除，原补丁及日志只保留在本地验收目录。原生失败、临时探针成功与最终普通构建成功分开记录，不修改旧断言或覆盖旧失败目录。

## 当前普通构建的完整系统输入

两组通过场景均使用 `d3074562` 的普通带 FFmpeg 程序、真实 Win32 键鼠、系统目录组成的 PATH；逐次输入前检查目标 HWND 归属、系统焦点与鼠标位置实际命中的顶层窗口。驱动先点击时间轴空白控制区建立输入上下文，再选择图层，不依赖尚未建立的启动选择。没有给产品添加重试或重启。

| 检查点 | 两种语言的完整状态断言 |
|---|---|
| 原图层复制和重命名草稿 | Copy Layers 不修改工程；草稿 Ctrl+X／Z／V 与 Esc 后完整工程及修订仍为初始值；文本没有替换图层缓存。 |
| 菜单及键盘连续粘贴 | 新 ID 分别为 10、11；每份复制的变换、关键帧、时间、内容与源一致；所有原图层及其他工程字段不变。修订依次为 1、2。 |
| 两次撤销 | 第一次完整还原只有第一份粘贴的工程；第二次还原全部原工程，修订依次为 3、4。 |
| 同位置重入后的剪切 | 点击最初图层名称位置后 Ctrl+X，只删除图层 7，其他字段完全不变，修订为 5；一次撤销还原全部原工程，修订为 6。 |
| 保存重开 | 实际 Ctrl+S、关闭退出 0；同一普通程序在独立进程重开，完整工程与原工程完全相同。 |
| 默认输出 | 原有 clipboard_read／clipboard_commit 的 INFO start 在文件出现；stdout 全为无错误的 JSON-RPC；没有编辑器 Error、临时探针或 invalid composition viewport。原 Epic Overlay loader Error 保留。 |

强断言验证器为 `verify-clipboard-pointer-entry-os-activated.py`，输出 `clipboard-pointer-entry-os-activated-verification.log` 与 `clipboard-pointer-entry-os-activated-summary.json`，两组均通过。主窗为英文 1280×850；中文主窗另带 1100×450 原生时间轴浮窗；实际系统 DPI 均为 96（100%）。[英文菜单](native-pointer-reentry/en-edit-menu.png) 与 [中文浮窗重新选中](native-pointer-reentry/zh-float-reselected.png) 已实际查看，来源与哈希见 [captures.json](native-pointer-reentry/captures.json)。本修正没有新增菜单或右键入口。

未通过和未满足前置条件的场景仍保留：第一组 `clipboard-pointer-entry-os-zh-float/` 初始点击未建立图层选择，后续 Ctrl+A／X 作用于图层而非重命名草稿，不能计为剪贴板闭环通过；`clipboard-pointer-entry-os-en/` 被实际 OS 焦点断言拒绝。增加指针命中窗口检查后的 `*-guarded` 组中，中文通过，英文仍未建立期望选择，原验证器因此在粘贴份数处失败。最终 activated 组显式建立选择上下文，并保留相同的完整工程和修订断言，没有通过放宽期望值掩盖这些失败。额外 `old-control`／`old-ready-control` 的启动激活／重排异常也不计作本功能的成功或失败对照；首次向驱动插入就绪条件时换行不匹配、没有实际插入，保留该脚本原件，随后独立的 settled 驱动明确检查插入结果。

### 相同驱动的修复前后对照

settled 驱动在输入前读取实际状态，确认 GPU 已附着、主浮窗均已创建、首帧及当前视图修订已呈现；随后使用完全相同的原生键鼠步骤、窗口焦点／指针归属保护，分别运行旧普通构建 `78b43ff0` 和新普通构建 `d3074562`。这是验收前置条件检查，不是产品的重试或等待变更。

`clipboard-pointer-entry-old-settled-control/` 与 `clipboard-pointer-entry-new-settled-control/` 在复制、草稿编辑、两次粘贴和两次撤销的所有检查点，完整工程与修订均逐项相同。第 29 步，旧构建仍是原工程、修订 4，剪切没有发生；新构建仅删除图层 7、修订 5。第 31 步，旧构建仍修订 4；新构建一次撤销恢复原工程、修订 6。两次均正常退出 0。`clipboard-pointer-entry-matched-control.log`／`.json` 保留严格对照结果，未更改最终成功场景的断言。

`verify-clipboard-pointer-entry-os-settled.py` 又对新 settled 中文浮窗和既有 activated 英文主窗执行相同的完整工程、默认输出及独立进程重开检查，两组均通过。可复核原件、脚本、步骤、状态及日志压缩于 [验收数据](data/native-pointer-reentry-d3074562.zip)，本地全部未通过场景仍保留。

## #204 已有交付与剩余门禁

原图层闭包、父级／资源／效果与关键帧的事务行为已随 #484 合入，默认故障信道、取消和旧身份保护见 [分阶段记录](layer-clipboard-progress.md)。`clipboard-current-main-core.log` 的当前组合核心图层剪贴板 12 项通过（4.51s），覆盖资源依赖、内部引用、新 ID、拒绝不变性、跨工程保存重开与时间偏移；这不是新指针缓存的错误观察测试。

TM-06／PC-07 中的路径独立缓存已经随 #484 的路径功能交付，#269 的系统操作／打包证据另有原件；本阶段补齐当前主线组合的中英文主浮窗实际系统键鼠与保存重开。既有文本／关键帧／路径／节点缓存和故障边界测试仍按原阶段证据计入，不累加重叠测试数量。完整打包式 FFmpeg 验收及五项必需 CI 已在 #484 的确切提交 `f0e26a83` 上通过；本次修复的确切提交仍需要自己的 CI，不能用旧的绿色检查代替。

## 复现操作

1. 使用三个图层的工程，把时间轴浮成独立窗口，选择最下面的图层并记住名称区域内的鼠标位置。
2. 使用主窗 Edit → Copy Layers；回到浮窗 Enter 重命名，Ctrl+A、Ctrl+X、Ctrl+Z、Ctrl+A、Ctrl+V 后 Esc。草稿不修改工程，文本缓存不能替换图层缓存。
3. 使用主窗 Edit → Paste Layers，再在浮窗 Ctrl+V；两次粘贴必须有独立新 ID。Ctrl+Z 两次还原原工程。
4. 将鼠标一次直接返回第 1 步的同一客户区位置，点击选中，再 Ctrl+X。期望剪切一次，工程只删除该层；一次 Ctrl+Z 还原原工程。
5. Ctrl+S 后关闭，在独立进程重开，完整工程保持一致。英文主窗与中文原生浮窗重复，取消与旧修订仍沿用原剪贴板验证。

## 设计处置

没有编辑 panel 绘制代码。DESIGN §0.7 的十二项基线均 retained with reason：路径六处行步进、双列、混合高度、自绘数值、布尔控件；效果行步进、布尔、枚举、hover、层级；时间轴列坐标与吸附线，全部沿用 [剪贴板记录](layer-clipboard-progress.md) 末尾逐项处置。本次不改变命中范围、行结构、颜色或尺寸，也不将编译与事件测试视为 DESIGN §12 的完整视觉验收。
