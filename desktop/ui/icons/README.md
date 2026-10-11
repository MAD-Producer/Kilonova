# Motion Studio SVG 图标库

包含 Ardot 最新 `modern/sheet/icon` 的 40 个图标和按相同规范补充的秒表、标签菜单图标，保留导出路径、圆角和线宽，统一使用 `currentColor` 跟随样式表

- 使用 `manifest.json` 记录稳定名称、设计节点来源和 28 × 28 原始尺寸
- 使用 `motion_ui::icons::Icon` 查找图标，不在面板中重新描摹路径
- 使用 16 / 20 / 28 逻辑像素按比例绘制，原始线宽随尺寸同步缩放
- 使用 `button.icon` 的 hovered / pressed / selected / focused / disabled 状态统一配色
- 使用缓存三角网格绘制，无逐帧 SVG 解析、额外贴图或 GPU pass
- 使用精确三角形裁剪保留越过面板边缘的轮廓

```rust
use motion_ui::{icons::{self, Icon}, style::{default_stylesheet, State, States}};

let response = icons::button(paint, bounds, Icon::Pen, input,
    default_stylesheet(), States::default().with(State::Selected));
```

更新 Ardot SVG 后，替换同名源文件并运行：

```powershell
python tools/generate_desktop_icons.py
python tools/generate_desktop_icons.py --check
cargo test -p motion-ui --locked
cargo run -p motion-ui --example framework -- --icons
```

生成器接受当前组件使用的路径、圆、圆角矩形、平移与等比缩放，遇到尚未支持的 SVG 特性会报错，避免静默丢失图形。圆和圆角采用三次贝塞尔近似，网格容差为原始尺寸的 0.015 像素。当前图标复用现有单采样 UI 绘制管线；无需字体即可显示，边缘抗锯齿仍由 UI 渲染器后续完善

素材浏览器与效果面板新画板暂不接入，但它们的通用图标已包含在库中
