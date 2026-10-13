# 桌面样式表

默认样式定义在 [`styles/dark.json`](styles/dark.json)。它使用灰黑背景、蓝色焦点、紧凑间距；面板和控件按语义样式键引用，同一组件在停靠与浮窗中使用同一份样式。

## 三层结构

- `tokens.colors`：十六进制颜色，支持 `#RGB`、`#RGBA`、`#RRGGBB`、`#RRGGBBAA`
- `tokens.sizes`：字体、边框与内边距的逻辑像素尺寸；`font_family` 选择已安装的 UI 字体，`system-ui` 使用当前平台与语言的默认字体
- `styles`：组件样式，可用 `extends` 继承另一个样式，并覆盖 `selected / hovered / pressed / focused / invalid / disabled` 状态

基础类包括 `panel`、`panel.header`、`toolbar`、`canvas`、`button`、`button.primary`、`field`、`field.number`、`row`、`menu`、`tooltip` 和文本标签。可添加自己的类，如 `button.export` 或 `panel.nodes`。

## 覆盖示例

创建一个 JSON 文件，仅写需要改变的值：

```json
{
  "tokens": {
    "colors": { "focus": "#60C5A0" },
    "sizes": { "font.title": 18, "padding": 6 }
  },
  "styles": {
    "button.export": {
      "extends": "button.primary",
      "padding": 8,
      "states": { "focused": { "border": "$colors.focus" } }
    }
  }
}
```

```sh
cargo run -p motion-ui --example framework --locked -- --locale zh --style desktop/ui/styles/example.override.json
```

真实编辑器同样支持 `motion-studio --style <file.json>`，或环境变量 `MOTION_UI_STYLE`。正常启动时将样式路径保存到工作区偏好；`--smoke` 不修改偏好。下次启动重新加载该文件。

编辑器每 250ms 检查文件变化，亦可按 **Ctrl+R** 手动重载。颜色、字体与组件外观更新到所有原生窗口。加载失败会显示带文件路径的错误并保留上一份样式；载入成功前也会检查字体能否创建。重载取消正在进行的编辑手势并还原其数值，不生成额外撤销项。

## 接入方式

- 使用 `StyleSheet::from_json / load` 读取完整样式表，或 `with_overrides / load_overrides` 覆盖已有样式表
- 使用 `PanelHost::set_stylesheet` 安装已验证的样式；组件通过 `PanelFrame::styles` 读取
- 桌面编辑器的旧面板通过 `Panels::set_stylesheet` 安装同一份不可变样式快照，主窗口与浮窗的 `PaintList` 均携带它；临时绘制层必须使用 `list.child()` 继承。无上下文的默认样式仅用于初始化、测试和兼容入口。`StyleSheet::color / colors / metric` 都是加载后查表，不在绘制时解析 JSON。
- 使用 `resolve(class, states)` 获取已解析的颜色和尺寸，使用 `box_style / text_style` 绘制
- 使用输入框、数值框和按钮的 `draw_styled` 接口；按钮的 `draw_with_states` 可接收显式焦点状态
- 更改全局字体时，由窗口宿主用 `TextAtlas::from_family` 更新共享图集；编辑器的工程字体不受影响
- 字号按每 em 的逻辑像素计算，与设计稿保持同一口径；不同字号独立缓存字形，避免缩放低分辨率字形造成模糊

加载时检查引用、继承循环、版本、属性与尺寸范围，并预计算状态组合。绘制时只查表，不解析 JSON。禁用状态最后应用，错误状态覆盖焦点边框；子样式保留父样式未被覆盖的属性。

样式属性新增 `radius`（0–128px）、`shadow_color`、`shadow_blur`（0–64px）和 `shadow_offset_y`（0–64px）。圆角和阴影均由一个过程化四边形绘制，裁剪不会重新计算阴影域。

`tokens.motion.easing` 可选 `linear / cubic-out / cubic-in / standard / decelerate / accelerate`，默认保留已有的 `cubic-out`。`motion.*_ms` 仍表示时长。`motion.drag_threshold` 是逻辑像素阈值，属性、路径和通用数值控件共用；0px 与 12px 均有效。Shift 精细、Ctrl 粗调，开始拖动后统一使用 Grabbing 光标。

密度和尺寸 token 包括 `font.small`（11px）、`row.height`（22px）、`field.height`（18px）、`panel.header.height`（24px）、`label.column`、`value.min_width`、`icon.slot`、`checkbox.size`。属性字段根据内容宽度决定是否上下排列；时间轴和列表消费相同行高。分隔线维持整行、0.5px、低透明度，不增加斑马纹或轨道区参考竖线。

`widgets` 提供 Checkbox、Dropdown、ScrollArea、ListView（支持树深度与展开）、SearchField、StarRating。控件接收宿主提供的焦点；只有获焦控件响应键盘。下拉菜单打开时宿主屏蔽同面板的其他输入，并在其余内容绘制完后追加菜单层。搜索框 placeholder 由调用方传入本地化文本。

这是一套原生 UI 的语义样式表。布局的分割比例、命中区域和控件交互由 UI 框架负责，没有 CSS 选择器或 keyframes。
