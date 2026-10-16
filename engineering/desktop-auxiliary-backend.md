# 后端需求：信息 / 预览 / 字符面板

桌面 UI 已按设计稿落地三个面板的版式（`desktop/app/src/panels/auxiliary.rs`），其中部分控件以后端能力为前提，当前以禁用态占位。本文档列出缺口、建议的命令与数据形态，供后端按优先级接入。以下命令和模型字段是待评审的建议，不代表当前接口已支持；接入时仍需核对渲染、历史、旧工程兼容和预览会话的责任边界。

## 一、信息面板（Panel::Info）

| UI 元素 | 现状 | 后端缺口 |
| --- | --- | --- |
| 图层名 / 时间码 | 已接真数据 | 无 |
| X / Y 光标坐标 | 占位 `—` | 需要合成视口发布光标合成坐标 |
| R / G / B / A 像素值 | 占位 `—` | 需要像素探针命令 |

### 1. 光标合成坐标（建议先做，纯 UI 侧亦可落地）

合成面板渲染时已知画面矩形与缩放，可将光标位置换算为合成坐标后写入共享状态（如 `panels.composition.cursor`），信息面板直接读取。无需引擎改动；若希望引擎统一换算，可在 `state["desktop"]` 增加：

```json
"cursor": { "composition": [960.0, 540.0] }
```

### 2. 像素探针

建议新增只读命令，返回合成在指定坐标、当前帧的线性 RGBA（0..1 浮点）：

```jsonc
// 请求
{"op": "probe_pixel", "position": [960.0, 540.0]}
// 响应
{"value": [0.854, 0.854, 0.854, 1.0]}
```

实现提示：复用预览帧的 GPU 回读路径（`RenderPreview`/export::capture 已有 readback），按帧及坐标缓存，并限制探针请求频率，避免随鼠标移动反复全帧回读；坐标越界时返回 `null`。UI 侧期望展示为 0..255 整型与 A 的 0..255。

## 二、预览面板（Panel::Preview）

| UI 元素 | 现状 | 后端缺口 |
| --- | --- | --- |
| 首帧 / 播放暂停 / 末帧 | 已接真数据 | 无 |
| 画质切换（自动/完整/流畅/经济） | 已接真数据（`Action::PreviewMode`） | 无 |
| 快捷键下拉（空格键） | 占位禁用 | 播放快捷键可配置化 |
| 视频复选框 | 占位禁用（勾选） | 预览视频开关 |
| 音频复选框 | 占位禁用（勾选） | 预览音频开关 |
| 缓存后播放 | 占位文本 | 缓存播放模式开关 |

### 3. 预览音视频开关与缓存播放

建议扩展 `Command::PreviewMode` 或新增：

```jsonc
{"op": "preview_flags", "video": true, "audio": false, "cache_then_play": false}
```

并在 `state["preview"]` 回读 `{"video": true, "audio": true, "cacheThenPlay": false}`。注意与图层级 `Command::SetAudio { muted }` 区分：此处是**预览会话级**开关，不写入工程文档。

### 4. 播放快捷键配置

当前播放快捷键写死为空格（`panels/keyboard.rs`）。若要做成下拉可选，需要把快捷键映射表移到可配置存储（工作区设置），UI 侧已预留下拉位。

## 三、字符面板（Panel::Character）

| UI 元素 | 现状 | 后端缺口 |
| --- | --- | --- |
| 填充色（色板 + RGBA 弹层） | 已接真数据（`Command::SetColor`，文本图层） | 无 |
| 字体名下拉 | 展示真实字体名，禁用 | 修改字体命令 |
| 样式下拉（Bold 等） | 占位 `—` | 文本模型扩展 |
| 字号 | 占位 `— px` | 文本模型扩展 |
| 字距 / 行距 | 占位 | 文本模型扩展 |
| 描边 | 占位「无」 | 文本模型扩展 |
| 段落对齐（左/中/右） | 占位图标 | 文本模型扩展 |
| 重置 | 占位禁用 | 依赖上述字段落地后一键恢复默认 |

### 5. 文本模型扩展（核心缺口）

当前 `Content::Text` 仅有 `{ text, font, color, raster_asset }`（`crates/motion-model/src/model.rs`）。建议扩展为：

```rust
Content::Text {
    text: String,
    font: String,
    color: [f32; 4],          // 填充
    raster_asset: u64,
    // 新增（serde default 保持向后兼容）
    size: f32,                // 字号 px
    style: String,            // "Regular" | "Bold" | ... 随字体族枚举
    tracking: f32,            // 字距
    leading: f32,             // 行距 px
    stroke: Option<Stroke>,   // 描边：颜色 + 宽度；None = 无
    align: TextAlign,         // Left | Center | Right
}
```

配套命令建议：

```jsonc
{"op": "set_text_style", "object": 7, "size": 64.0, "style": "Bold",
 "tracking": 0.0, "leading": 80.0, "align": "left",
 "stroke": {"color": [1,1,1,1], "width": 2.0} }   // stroke: null 表示无
{"op": "set_text_font", "object": 7, "font": "<font-id>"}
```

注意点：

- 文本栅格化（`raster_asset`）依赖字体排版结果，上述字段变化后需要失效并重排；建议复用现有文本创建的排版管线。
- 字体族的可选样式列表需要在 `state["project"]["fonts"]` 中暴露（如 `"styles": ["Regular","Bold"]`），供样式下拉填充真实选项。
- 「重置」语义为恢复上述字段到默认值，UI 侧会在字段可用后启用。

## 接入顺序建议

1. 光标合成坐标（成本最低，信息面板立即受益）
2. 预览音视频开关（分别核对画面、真实音频输出与缓存播放的现有能力，逐项解禁）
3. 像素探针（GPU readback 路径复用，注意性能）
4. 文本模型扩展（最大，建议拆 PR：模型字段 → 排版失效 → 命令 → UI 解禁）
