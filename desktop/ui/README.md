# Motion Studio UI Framework

Rust + wgpu 桌面 UI 底座。面板、控件和工作区保持独立，应用通过 typed actions 对接自己的编辑命令与撤销系统。

颜色、字号、边框、间距与控件状态统一使用 [JSON 样式表](STYLES.md)，支持继承、覆盖和真实编辑器／演示窗口热重载。

## 分层

| 模块 | 职责 |
| --- | --- |
| `theme` | 灰黑色板、强调色、紧凑尺寸与缩放参数 |
| `paint` / `text` | 绘制列表、裁剪、共享 GPU Painter 与系统字体图集 |
| `icons` / `tabs` | Ardot SVG 图标、标签内菜单、激活下划线与图层上下文 |
| `registry` / `panel` | 稳定面板 ID、双语标题、布局元数据、统一内容接口与生命周期 |
| `dock` / `workspace` | 分割、标签页、关闭与重新打开、浮窗归属、可序列化布局 |
| `interaction` | 窗口 / 面板 / 控件三级输入范围、焦点、Tab 导航与指针捕获 |
| `widgets` | 数值与 Unicode 文本编辑、按钮、复选框、下拉、滚动／树列表、搜索、星级收藏、属性列布局 |
| `overlay` | 弹出层与模态层状态、关闭请求；关闭是否获准由应用处理 |

UI 库的正常依赖不包含 `motion-core`、`motion-host` 或 JS 工具链。`winit` 窗口示例属于开发依赖；实际编辑器继续使用自己的 Session、GPU 设备和工作线程。

桌面编辑器的面板标题采用内容宽度标签。选中标签显示细下划线，效果控件和属性标签可显示实际选中图层的名称及锁定状态。菜单图标紧跟当前标签，标题右键也能打开关闭、浮动、关闭其他标签和重置工作区菜单；绘制及拖拽命中复用同一标签几何。

SVG 源文件、生成与更新方法见 [图标库](icons/README.md)。`framework --icons` 可检查全部图标。

## 面板接口

应用提供 `PanelView`，用 `PanelHost::register` 注册标题和尺寸。停靠与浮窗用同一个 `PanelLayout` / `PanelFrame` 调用内容组件；组件读取输入并返回应用自己的 Action 类型。

`Attached`、`Detached`、`Hidden`、`FocusLost`、`Closed` 生命周期通知用于结束局部交互。关闭面板只修改工作区，工程保存和未保存提示由应用处理。`WindowGeometry` 保存浮窗的尺寸与位置，原生窗口创建和显示器范围校验属于窗口宿主。

控件 ID 在所属面板内保持稳定。窗口宿主先收集当前布局的 hit regions，再调用 `Router::begin_frame`；组件通过 `PanelFrame::input_for` 接收自己的输入，最后调用 `Router::end_frame`。打开模态层时调用 `set_modal`，隐藏或移走面板时通知生命周期并调用 `forget_panel`。弹出层应使用独立的输入范围，背景区域不能复用其控件 ID。

数值控件返回 `Begin / Change / Commit / Cancel`。宿主应在 Begin 捕获对象、属性、合成、轴与采样时刻，将后续事件绑定到这次事务；UI 库不改变工程关键帧、XYZ 分离或数值联动规则。

## 交互约定

The first held pointer button owns the accumulated gesture and its origin until its matching release, pointer exit or focus loss. Secondary button events remain visible without replacing that owner. Numeric scrubbing and button arming end only on the primary button's release; routed capture is released only by its owning native window. Widget navigation accepts initial Tab / Shift+Tab presses, visits each visible target once in registration order, and defers to IME and Ctrl/Alt chords. The direction comes from the key event's modifiers.

- 单击数值保留并选中原值，未修改按 Enter 保留完整精度
- 拖动超过 3 个逻辑像素开始事务，释放提交，Esc 或失焦取消并恢复起点
- Shift 精细调整，Ctrl 粗调整；空输入、非有限数值和越界输入不写入数据
- 文本支持 UTF-8 安全的光标移动、选择替换、删除、提交和取消
- 按钮在释放时触发，释放到按钮外取消点击
- 绘制与命中测试使用同一裁剪范围，隐藏或被裁剪的输入不会传到其他面板

文本整形、剪贴板、无障碍语义树、复合控件和曲线控件属于后续扩展。数值重映射曲线节点可以复用这一面板和输入框架，但曲线数据及 WGSL 求值仍应由图模型与编译器提供。

## 运行组件演示

```sh
cargo run -p motion-ui --example framework --locked -- --locale zh
cargo run -p motion-ui --example framework --locked -- --locale en
```

演示窗口可拖动面板标题、调整分割、合并标签、创建原生浮窗并重新停靠，可试用数值拖动、文本编辑和一步撤销。演示中的数值和文字是 UI 示例状态；没有打开工程或启动编辑会话。窗口空闲时按需重绘。

```sh
cargo test -p motion-ui --locked -- --test-threads=1
cargo run -p motion-ui --example framework --locked -- --smoke artifacts/ui/framework-zh.json --locale zh
```

`--smoke` 检查真实窗口与 Vulkan/GPU 呈现、数值事务、撤销、浮窗和布局保存恢复，并输出主窗口及浮窗截图。

Dock resizing shares `dock::clamp_split` between the public API and workspace
interaction. Limits use existing logical-pixel metrics, without a percentage
floor on large windows. An undersized parent reserves up to 45% for each child.
Stored split ratios must be finite and strictly between zero and one; the JSON
schema is unchanged. Rendering and nested splitter hit testing share the same
division geometry. Rejected split requests leave the original tree unchanged.

The independent gallery accepts `--size WIDTHxHEIGHT`. Its native smoke probe
drags a splitter to the pixel minimum, presents and captures both states, checks
that rejected splitting and disk round-tripping preserve that layout, then
restores the gallery without editing its model or undo history. The separate
`.dock.json` report records the actual logical size, scale factor and
resulting width.
