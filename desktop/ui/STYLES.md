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

编辑文件后按 **Ctrl+R** 重新加载。颜色、字体与组件外观更新到所有原生窗口，数值和撤销历史保持不变。加载失败会显示错误并保留上一份样式。

## 接入方式

- 使用 `StyleSheet::from_json / load` 读取完整样式表，或 `with_overrides / load_overrides` 覆盖已有样式表
- 使用 `PanelHost::set_stylesheet` 安装已验证的样式；组件通过 `PanelFrame::styles` 读取
- 使用 `resolve(class, states)` 获取已解析的颜色和尺寸，使用 `box_style / text_style` 绘制
- 使用输入框、数值框和按钮的 `draw_styled` 接口；按钮的 `draw_with_states` 可接收显式焦点状态
- 更改全局字体时，由窗口宿主用 `TextAtlas::from_family` 更新共享图集；编辑器的工程字体不受影响

加载时检查引用、继承循环、版本、属性与尺寸范围，并预计算状态组合。绘制时只查表，不解析 JSON。禁用状态最后应用，错误状态覆盖焦点边框；子样式保留父样式未被覆盖的属性。

这是一套原生 UI 的语义样式表。布局的分割比例、命中区域和控件交互由 UI 框架负责；目前没有 CSS 选择器、动画、阴影或圆角属性。
