# #250 动画菜单：实现与验收进度

本工作独立保存在 `feat/animation-actions`，现已同步包含主线精度设置的父级提交 `dcabd524`，功能提交为 `416ce406`。不包含于已合并的 #484／#488，也未创建本功能 PR；下述新证据明确对应 `416ce406`，不冒称新 main 的发行包验收。

## 实现与边界

| 项目 | 当前处置 | 证据／限制 |
|---|---|---|
| AM-01 插值编辑 | 新增类型化 `KeyframeInterpolation`；完整预检所选通道／帧后修改 ease 与自定义曲线元数据，保留时间、值和空间手柄 | 核心实际事务、保存重开、撤销重做和拒绝不变性测试；不等于已实现 #216 的新多选交互 |
| AM-02 菜单与快捷键 | 复用既有加减 key、删除及 F9 三类缓动的规划；线性／保持使用当前属性选区，行文显示关键帧数量 | 命令级比较中英文／主窗与浮窗路由；英文主窗与中文原生浮窗的实际属性选择、插值和 F9 已验证；加减 key 的完整键盘回归仍未通过 |
| AM-03 图层序列 | 按可见堆叠顺序拼接选中图层，保留第一层起点及源偏移关系，整批一个撤销项 | 仅支持既有模型的整帧位移；锁定、缺失或分数位移整批禁用。反向时间因缺少重映射能力明确禁用 |
| AM-04 过期提交 | 菜单冻结工程、合成、代际、修订、选层、属性／关键帧及焦点；类型化 `EditorAction` 在工作线程再次检查身份 | 新内部协议：过期返回当前快照及 `editorAction.cancelled/context_changed`，归类正常取消；不修改原 Edit/respond 语义 |
| AM-05 错误信道 | `KeyframeEdit` 保留操作、对象、通道与原始错误；通道上下文有长度限制，帧只列前八个及总数。新动作复用既有 worker 在途、Info start 和错误处理 | 无新增降级计数器、重试、重启或 Present 热路径日志；实际错误仍通过已有回复信道显示 |
| AM-06 原生输入 | 英文主窗的序列菜单及 Ctrl+Z、两种语言窗口的插值／缓动和取消矩阵已实际通过 | 加减 key 探针的快捷键／撤销仍未通过，不能关闭 #250 |

## 已完成验证

产物位于 `E:/Dev/aem/artifacts/pr-ci-followups/`，以下测试组存在交集，不累加为独立总数。

- `animation-interpolation-core-third.log`：关键帧剪贴板组 21 项通过，其中新增插值相关 4 项，覆盖静态／分离维度／空间手柄、效果／蒙版／节点几何通道、非法帧／锁定／缺失对象、单步历史与保存重开。
- `animation-actions-regression-second.log`：动画相关 14 项通过，包含真实默认 INFO 子进程的缺失 key、缺失对象、锁定目标三类故障，核对文件日志、Error stderr 与 stdout 隔离；同修订但不同工程身份、旧修订正常取消，以及有效动作和撤销恢复。原件保留在 `animation-action-errors-second/`。
- `animation-menu-cancellation-final.log`：7 项菜单测试通过，含真实绘制／命中的正对照；工程、代际、修订、合成、选层、属性、key 和焦点变化均在点击前关闭旧菜单。禁用项、失焦及外部点击不发编辑动作。
- `animation-timeline_shortcuts-regression.log` 48 项、`animation-effect_timeline-regression.log` 13 项通过；`animation-native-build.log` 的带 FFmpeg 桌面编译通过。
- `animation-os-main-sequence/`：实际鼠标多选图层并点击序列菜单，图层 7 从 `[0,20]` 移至 `[25,45]`，偏移变为 25，第一选层 8 保留 `[5,25]`，非选层 9 不变；一次 Ctrl+Z 后完整工程与初始工程相同，退出 0。

## 同步主线后的实际验证

`animation-main-sync-core.log` 的关键帧剪贴板 21 项、`animation-main-sync-desktop.log` 的动画相关 16 项通过；带 FFmpeg 的实际桌面编译通过，默认 INFO 的真实错误原件保留在 `animation-main-sync-error-output/`。这些与前述测试存在交集，不累加为独立总数。

`animation-main-sync-os-en/` 和 `animation-main-sync-os-zh-float/` 使用同一 `416ce406` 本地带 FFmpeg 程序，PATH 只含系统目录，Win32 实际鼠标／键盘输入，MCP 只读回状态。英文主窗与中文原生时间轴浮窗均完成：选中位置属性后整属性保持插值、一次撤销恢复完整工程、选中第 10 帧后菜单缓动、一次撤销、真实 F9 得到完全相同工程、再次撤销、Esc 与外部点击取消、保存后独立进程重开。三帧时间和值及其他图层不变；缓动只改变第 0／10 帧持有的相邻段曲线，第 19 帧保持不变。验证器检查完整工程和修订，而非只看退出码。证据为 `animation-main-sync-os-summary.json` 与 `animation-main-sync-os-verification.log`，两次均退出 0。默认 Info start 入文件、stdout 全部为合法 JSON-RPC；本机 Epic Overlay Vulkan loader 错误保留。此为 100% DPI，本地程序，不是官方包、150% DPI 或真实 IME 验收。

截图已逐张查看：英文的 `animation-property-menu.png` 展示三个属性 key 的插值作用域，中文的 `animation-key-menu.png` 展示单 key 的缓动作用域，浮窗的 `native-position-property.png` 保留真实属性行。

## 失败原件与未完成项

- 核心早期验证发现夹具错误：None 字段在 JSON 中被省略、空间手柄与分离维度预置不相容，以及 no-op 测试误撤销上一轮历史。保留 `animation-interpolation-core-{first,second}.log`；修正测试输入和独立历史后通过，没有改变产品拒绝条件。
- 编译阶段的模块可见性、测试枚举比较与直接引用仅属 dev-dependency 的模型常量问题均已修正。工作树切换后还发现共享缓存误用主线 `motion-ui` 的旧接口，保留 `animation-menu-cancellation.log`；重新构建实际工作树库后，`animation-menu-cancellation-correct-cache.log` 暴露测试的两个枚举比较编译错误，最终日志为 `animation-menu-cancellation-final.log`。未改生产返回值或预算来掩盖这些失败。
- 第一轮交互探针中窗口发生最小化／恢复，出现 `invalid composition viewport`，仍缺少该独立问题的受控复现和修复证明。后续使用新测试场景验证常规输入，未在产品中添加重启；不将这些场景冒充最小化恢复通过。
- `animation-os-main-en-fresh.log` 的探针步骤未提供关闭动作，驱动因 `StopIteration` 退出；后续完整步骤均明确关闭。`animation-os-main-en-final/` 虽退出 0，但输入没有选择到期望属性，插值／缓动没有产生工程修改，**不记为这些功能的成功证据**。需根据实际行几何修正驱动并保留原状态断言。
- `animation-main-sync-toggle-en/` 的加减 key 探针发生非预期图层重排；带逐步截图的 `animation-main-sync-toggle-en-observed/` 中，菜单删除位置 key 和添加缩放 key 符合预期，但随后 Ctrl+Z 与 Alt+Shift 组合没有改变工程，仍需受控定位输入／焦点原因。保留全部状态、截图和日志，不将第二轮记为完整成功，也不据此改生产返回值或断言。
- 待完成：加减 key 快捷键／撤销的原生焦点问题、中文对应验证、实际过期菜单场景和本功能组合 CI；完成后再提交 PR。#250 仍开放。

## 设计与观测核对

`DESIGN.md` §0.7 的路径控件六处步进、列宽、混合按钮高度、手绘字段、布尔按钮，效果控件行高、布尔按钮、枚举按钮、悬停背景及层级，以及时间轴既有列坐标和吸附线，共十二项均 retained with reason：本阶段未改这些绘制位置。新增动画菜单沿用既有菜单几何及语义样式，不新增颜色、尺寸或素材。§12 的全量视觉、DPI 与真实 IME 验收不在上述已通过测试的证明范围内。

AM-04 是独立的功能／协议边界；AM-05 主要复用已有观察，只增加类型化命令名称与原始错误上下文。正常取消和 no-op 不计故障；无新增阈值需要声称验证。实际错误、恢复与默认输出通过原始子进程验证，不以成功 mock 或测试数量代替错误证据。
