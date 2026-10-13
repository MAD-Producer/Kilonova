> 用户提供的设计规格快照，核验基线为 cee9686。当前实现进度和冲突处理见 [UI 集成说明](IMPLEMENTATION.md)。本文不表示全部功能已实现。

# Motion Studio · 设计稿 vs 代码现状 对比报告

> 对比基线
> - **设计稿**：Ardot `仿AE软件 UI 设计稿`（Motion Studio，Page 1），14 个画板
> - **代码**：`github.com/MAD-Producer/motion-studio` @ `cee9686 feat(desktop): 接入节点内控件、真实子图与折叠持久化`，1187 文件
> - **文档**：`Motion-Studio-UX.md` v1.0

---

## 摘要：一句话结论

**设计稿描述的是一个完整的桌面动效工作台；代码里桌面端目前只有「基础编辑流程 + 曲线/时间轴 + 节点编辑器」。二者交集约占设计稿的 55%，最大缺口集中在拉曲线的方式（设计稿定义了 5 套语义，代码实现了 1 套）和工程主页/工作区预设（设计稿有，代码完全没有）。**

最需要你拍板的三件事：
1. **影响权重（33.3%）要不要做** —— 代码里 0 匹配，是刻意简化还是待补？
2. **速度图表要不要做成 AE 式独立模式** —— 代码把它降级成"定义空间切换"，是有意的架构决策
3. **设计稿的工程主页/5 个工作区预设要不要保留** —— 代码里是"窗口菜单列面板 + 布局 JSON"，路线不同

---

## 一、设计系统：颜色与尺寸全部错位

### 1.1 色板逐项对比

| 语义 | 设计稿（实测） | 代码 `desktop/ui/src/theme.rs` | 差异 |
|------|---------------|---------------------------|------|
| 窗口底 | `#1D1D1D` (29) | `APP_BACKGROUND #111111` (17) | **+12 亮** |
| Dock 底 | `#181818` (24) | `SUNKEN #111111` (17) | **+7 亮** |
| 面板底 | `#232323` (35) | `PANEL #1C1C1C` (28) | **+7 亮** |
| 工具栏/菜单栏底 | `#242424` (36) / `#1D1D1D` | `CHROME #232323` (35) | 角色错位：设计的"面板底"值 = 代码的"CHROME" |
| 描边/分隔 | `#3B3B3B` (59) | `BORDER #363636` (54) | +5 |
| 强调蓝 | `#61A8F5` (97,168,245) | `ACCENT #378EF0` (55,142,240) | **设计稿更浅更淡，代码更深更饱和** |
| 关键帧黄 | `#E3D94D` (227,217,77) | `KEYFRAME #E4D84C` (228,216,76) | ✅ 几乎完全一致 |
| **曲线绿** | `#76C46A` (118,196,106) | **不存在** | ⚠️ **代码里曲线用 `palette::ACCENT` 蓝色画** |
| 文本主 | `#D1D1D1` (209) | `TEXT #DCDCDC` (220) | +11 |
| 文本次 | `#A6A6A6` (166) | `TEXT_MUTED #ADADAD` (173) | +7 |
| 文本弱 | `#858585` (133) | **不存在**（只有两档灰） | ⚠️ 设计稿是三级灰阶，代码只有两级 |
| 轨道色 | 无 | `TRACKS #747443 / #4D597F / #41623B`，`TRACK_CAMERA #685782` | 代码有，设计稿没画 |

**结论**：设计稿整体比代码**亮约 5–12 个色阶**，且**色板角色错位一格**（设计的"面板底"值被代码用作"工具栏底"）。这不是笔误，是两套独立演进的设计系统。

### 1.2 尺寸对比

| 指标 | 设计稿 | 代码 `metrics` | 差异 |
|------|-------|---------------|------|
| 菜单栏高 | 22 | `MENU_BAR 24` | +2 |
| 工具栏高 | 30/32/34（三值，设计稿自身不一致） | `TOOL_BAR 32` | 统一到 32 |
| 面板 Tab 条高 | 24 | `TAB_BAR 26` | +2 |
| 时间轴图层行高 | 44 | `ROW 24` | **差 20px，设计稿密度低一倍** |
| 属性行高 | 22 | `PROPERTY_ROW 22` | ✅ 一致 |
| 时间轴标尺槽宽 | 268（图层列表头） | `RULER 92`（标尺 gutter） | 语义不同，需重新对齐定义 |
| 曲线属性树宽 | 320 | 代码无独立宽度常量 | 待定 |
| 分割条 | gap 4px | `SPLITTER 4` | ✅ 一致 |

`ROW 24 vs 44` 是最刺眼的一条：**设计稿的时间轴行高比代码松 83%**，视觉上会显得"空"。需要确认设计稿是不是按 1.5× 缩放画的。

---

## 二、拉曲线：设计稿 vs 实现（**最大缺口**）

这是你特别点名的部分，逐层拆。

### 2.1 插值类型：设计稿是 6 个平级选项，代码是**两层互斥模型**

**设计稿**（`5:2596` 插值类型下拉）
```
线性 · 贝塞尔 · 缓入 · 缓出 · 瞬间 · 保持
```

**代码**：两个独立字段，同一 key 上共存但互斥（`set_ease` 清 `curve`，反之亦然）

```rust
// crates/motion-model/src/animation.rs:4-13
pub enum Ease { Linear, In, Out, InOut, Hold }        // "linear","in","out","in_out","hold"

// crates/motion-model/src/curve.rs:18-40
pub enum CurveShape {
    Quadratic { control: [f64;2], start: f64, end: f64 },
    Cubic     { control1: [f64;2], control2: [f64;2], start: f64, end: f64 },
    Elastic   { oscillations: f64, damping: f64 },
}
```

**差异点**：
| 设计稿 | 代码 |
|-------|------|
| 6 个平级选项 | `Ease`(5) × `CurveShape`(3) **两层**，且**没有 "瞬间/Instant"**——只有 `Hold` |
| "贝塞尔"作为插值类型 | `CurveShape::Cubic` 是独立于 `Ease` 的形状层 |
| — | 代码有设计稿**没有的**：二次贝塞尔 Quadratic、弹性 Elastic |

> `Ease::Hold` 的实现是"t<1 返回 0，t==1 返回 1"（`animation.rs:23-29`），且速度恒为 0，注释写着 `// The end discontinuity has no finite derivative.` —— 它同时承担了设计稿里"瞬间"和"保持"两个语义。**设计稿需要把这两者合并，或者代码需要拆开。**

### 2.2 影响权重（入点/出点 33.3%）：**代码里完全没有**

设计稿：底栏常驻 `入点影响 33.3% · 出点影响 33.3%`（`5:2564`），属性树有 `入点影响` / `出点影响` 两个下拉字段（`5:2601`、`5:2606`），我在文档 §5.8.4-D 还给了档位建议。

代码全仓库实测：

| 关键词 | 匹配数 |
|-------|-------|
| `\binfluence\b` | **0** |
| `\btension\b` | **0** |
| `\bbias\b` | **0** |
| 曲线/关键帧语境下的 `\bweight\b` | **0** |

**代码的实际做法**：`CurveShape` 的控制点是**归一化 [f64;2]**，天然就编码了手柄长度与时间比例，**不需要 AE 那套显式 influence 百分比**。约束是：

```rust
// curve.rs:135-160
p[0] ∈ 0.0..=1.0                    // 时间轴
p[1].abs() <= 8.0                   // 数值轴，允许超越/回弹
CurveSpace::Progress => start==0.0 && end==1.0
CurveSpace::Velocity => integral().abs() >= 0.05   // "速度曲线面积不能为零"
```

**这其实是个合理的简化**，但它和设计稿是两套心智模型。我需要你拍板：

- **A. 保持代码现状** → 删掉设计稿的"入点影响/出点影响"字段和底栏文案，改为显示 `α / L` 或 `控制点 (x, y)`
- **B. 按设计稿加 influence** → 需要改 `CurveShape` 的序列化格式，工程格式要升版本
- **C. 折中** → 保留归一化控制点，但在 UI 上把 `α + L` 换算成百分比显示（只读，不落盘）

### 2.3 速度图表：设计稿要 AE 式双模式，代码是"定义空间切换"

**设计稿**（`5:2552`/`5:2554`）：两个 92×24 Tab 互斥，`速度图表`（Y = px/s）/ `数值图表`（Y = px），带积分关系教学文案。

**代码**：

```rust
// curve.rs:6-12
pub enum CurveSpace { Progress, Velocity }
```

- `CurveSample` **同时返回 `progress` 和 `velocity`**（`curve.rs:60-65`），两个空间都算
- `Velocity` 空间的语义是"你按速度曲线**画**，progress = 归一化面积"（`curve.rs:221-232`）
- 桌面上只是一个按钮 `Progress Curve` / `Velocity Curve`（`animation_controls.rs:462-485`）
- **桌面时间轴值曲线（`timeline_graph.rs`）画的永远是 progress（值），没有速度模式**
- **只有 Android `CurveEditor.kt:125-126` 能切换绘制字段**（`"进度"` / `"速度"`）

**这是代码有意做出的简化**：速度不是"第二种观察方式"，而是"另一种**创作**方式"。好处是模型统一（同一套 `CurveSample`），坏处是**丢失了 AE 用户最熟悉的那个心智模型**——"我要让这段匀速/我要看速度峰值"。

> `desktop/PANELS.md:59-63` 有一段很清醒的自述，值得引用：
> > 时间轴的速度曲线需要积分得到进度，不能直接当成数值重映射。共享现有曲线数据、编辑控件和求值方法时应保留这一区别。
>
> 这说明团队**明确知道**速度/进度是有区别的，且刻意的。

### 2.4 手柄：设计稿 3 档修饰键，代码 0 档

| 操作 | 设计稿（我的文档 §5.8.4-B） | 代码 |
|------|--------------------------|------|
| 裸拖手柄 | 改切线，两侧保持链接 | 拖 `control1`/`control2`（Quadratic 时拖 `control`） |
| `Alt` + 拖 | 解除链接 | ❌ **无此概念** |
| `Ctrl` + 拖 | 锁定时间比例 | ❌ **无此概念** |
| `Shift` + 拖 | 角度吸附 15° | ❌ **无此概念** |
| 手柄链接/镜像标志 | 有 | ❌ **全仓库无 linked/auto-smooth/mirror 字段** |
| 命中区 | 我建议 ≥16px | `paint::diamond(list, p, 5.)` → **菱形半径 5px，即 10px 视觉** |

**F9 快捷缓动**（代码已有，设计稿没有）：

```rust
// desktop/app/src/shortcuts.rs:75-77
(Animation, Key::F9,        false, false, false) => EaseBoth,
(Animation, Key::F9,        false, true,  false) => EaseIncoming,
(Animation, Key::F9,        true,  true,  false) => EaseOutgoing,
```

即 `F9` 平滑两侧 / `Shift+F9` 入段 / `Ctrl+Shift+F9` 出段。实现是把任何曲线转成 Cubic，然后把手柄 y 拉平（`timeline_shortcuts.rs:145-155`）。

⚠️ **注意**：我在设计文档里写的是 `Ctrl + 拖手柄 = 锁定时间比例`，这和代码的 `Ctrl+Shift+F9 = 出段缓动` 概念冲突，**需要统一到一条文档里**。

### 2.5 读数卡

设计稿：`168×50` 卡片，`Y 位置 · 速度` + `671.0 px/s`（`5:2645`）
代码：❌ **曲线编辑器无任何读数 HUD**。数值拖动有（`animation_controls.rs`，但不是曲线区）

### 2.6 撤销 —— 我的文档判断正确，代码已验证

我文档 §4.3 写的「**整段拖拽记为一步**（不是每帧一步）」，代码不但实现了，还有专门的测试：

```rust
// crates/motion-core/tests/acceptance.rs:122-154
e.begin_gesture().unwrap();
for x in 0..50 { e.apply(set(0, [540.0 + x as f32, 960.0, 0.0])).unwrap(); }
assert!(e.undo().is_err());          // 手势进行中禁止撤销
e.end_gesture(true).unwrap();
e.undo().unwrap();
assert_eq!(e.project(), &original);  // 一次撤销回滚全部 50 步
```

- 历史深度：`128 条` 或 `32MB`，取先到者（`editor.rs:974-988`）
- 桌面曲线拖拽：`Action::History(2)` 按下 / `Action::History(3)` 松开（`animation_controls.rs:584,633`）
- **我文档写的"栈深 200"要改成 128**

✅ 这条可以放心，不需要返工。

### 2.7 拉曲线小结

| 能力 | 设计稿 | 代码 |
|------|-------|------|
| 进度/速度双模式 | ✅ AE 式双 Tab | ⚠️ 降级为定义空间切换，桌面无速度图 |
| 插值类型 | 6 平级 | `Ease`(5) × `CurveShape`(3) 两层 |
| 影响权重 | ✅ 33.3% 可编辑 | ❌ 不存在 |
| 贝塞尔手柄编辑 | ✅ | ✅（但语义不同：归一化控制点 vs 切线） |
| 修饰键矩阵 | ✅ 4 条 | ❌ 0 条 |
| 读数 HUD | ✅ | ❌ |
| 吸附（整帧/出入点） | ✅ | 部分（数值拖动有 3px 阈值） |
| 拖拽=单步撤销 | ✅ | ✅ 已验证 |
| 曲线复制粘贴 | ✅ | ✅ Android 系统剪贴板 / 桌面进程内 |
| 多通道叠加 | ✅ | ❌ |
| 弹性曲线 | ❌ 设计稿没画 | ✅ 代码有，桌面有按钮 |

---

## 三、画板级覆盖对比

| # | 设计稿画板 | 代码 | 状态 |
|---|-----------|------|------|
| 1 | 工程主页 `2:2` | ❌ 无 | **完全没有**。桌面直接开编辑器，无主页/最近项目/模板 |
| 2 | 合成设置 `2:3` | ✅ `Ctrl+K` | 有，基础可用 |
| 3 | 合成工作区 `2:1` | ✅ | 有，布局不同 |
| 4 | 面板拖放停靠 `5:5968` | ✅ **更成熟** | 代码支持：拖到面板中央=标签页、拖到边缘=分割、拖出=原生浮窗、`↗`创建浮窗、`×`隐藏 |
| 5 | 浮动与双预览 `5:6050` | ✅ | 有原生浮窗；双预览未确认 |
| 6 | 面板与工作区管理 `5:6132` | ⚠️ 部分 | 有"窗口菜单列所有面板" + `desktop-layout-v2.json` 布局保存，但**无对话框式的面板清单/联动组勾选/工作区预设** |
| 7 | 时间轴 `5:3342` | ✅ | 有，但行高 24 vs 设计稿 44 |
| 8 | 曲线编辑器·速度 `5:2401` | ⚠️ | `Panel::Curves`「曲线编辑器」/「Graph Editor」存在，但无速度图模式 |
| 9 | 曲线编辑器·数值 `5:2649` | ✅ | `animation_controls.rs` + 时间轴内嵌值曲线（`Shift+F3`） |
| 10 | 钢笔与路径 `5:3971` | ✅ | 空间手柄 `SpatialTangents` 已实现，路径/蒙版面板齐全 |
| 11 | 节点编辑器 `5:351` | ✅✅ **远超设计稿** | 22 个文件，`view/inspector.rs` 达 40KB，含子图、折叠持久化、异步编译导出 |

### 3.1 设计稿**漏掉**但代码已有的能力

| 能力 | 代码位置 | 快捷键 |
|------|---------|--------|
| **FX Console 效果搜索浮层** | `panels/fx_console.rs` | `Ctrl+Space` |
| **表达式引擎**（QuickJS） | `motion-core` + `crates/motion-core/src/README.md` | — |
| 效果包管理（安装/启用/卸载 `.msfx`） | `Registry` | — |
| 轨道遮罩 / 15 种混合模式 | `host-compositing.md` | — |
| 3D 摄影机（推进/平移/环绕） | `motion_core::camera` | — |
| 嵌套合成 / 预合成 | `motion_core::composition` | — |
| 音频（8 种格式导入、波形、混音、音频级关键帧） | `motion-media` | — |
| 节点子图（`Ctrl+G` / `Tab` 进入） | `node_editor/grouping.rs` | `Ctrl+G`/`Alt+G`/`Tab`/`Ctrl+Tab` |
| Alt+右键划线批量断线 | `view/interaction.rs` | `Alt+右键划线` |
| MCP 接口（`--mcp` / `--mcp-ui` / `--mcp-read-only`） | `desktop/MCP.md` | — |

> **建议**：把这些补进设计稿，否则研发会觉得设计稿"漏了很多"而各做各的。

### 3.2 设计稿快捷键 vs 代码实际

| 我文档写的 | 代码实际 | 冲突 |
|-----------|---------|------|
| `U` 打关键帧 / `Shift+U` 删除 / `Ctrl+U` 清除 | `U` = 显示已打关键帧的属性；打关键帧是 `Alt+Shift+P/S/R/T` | ⚠️ **需改** |
| `J`/`K` 倒放/停止/播放 | `J`/`K` = 跳到上/下一个关键帧 | ⚠️ **冲突，需改** |
| `F9` 平滑 | `F9` 平滑两侧 ✅ / `Shift+F9` 入段 / `Ctrl+Shift+F9` 出段 | ✅ 补齐 |
| `空格` 播放 | ✅ 一致 | — |
| `,`/`.` 上一帧/下一帧 | `PageUp/PageDown` 或 `Ctrl+←/→` | ⚠️ **需改** |
| `Space+拖` 平移 | 中键/Shift+中键拖动（仅节点区） | ⚠️ 合成区未实现 |
| `Ctrl+滚轮` 缩放时间轴 | `Ctrl+滚轮` = 缩放画布（**仅节点**） | ⚠️ 时间轴缩放是 `+`/`-` |
| — | `Shift+F3` 时间轴↔值曲线切换 | **设计稿没有** |
| — | `P/S/R/T` 属性筛选（再按收起）、`Shift+P/S/R/T` 追加筛选 | **设计稿没有** |

---

## 四、代码仓库自身的文档矛盾（建议顺手修）

| 文档 | 说法 | 事实 |
|------|------|------|
| `README.md:114` | 「节点创作…主分支**尚未提供节点编辑器**」 | `desktop/app/src/node_editor/` 有 22 个文件 |
| `desktop/README.md:48` | 「节点编辑器**尚未实现**」 | 同上 |
| `desktop/README.md:60` | 「完整曲线编辑器…和节点编辑器**仍待实现**」 | 两者都已实现 |
| `desktop/NODE-AUTHORING.md:3` | 「状态：设计方案，**尚未实现**节点编辑器或编译器」 | 同上 |
| `SDK-PARITY.md:89` | 「Node authoring and msfx export…**remain a design proposal**」 | 同上 |
| `SDK-PARITY.md:32` | 「**59** core effect packs」 | `README.md` 说「**95** 项效果」 |
| `README.md:114` | 「桌面视频解码已提供；桌面音频播放与 MP4 编码仍在开发」 | 与 `SDK-PARITY.md:53` 一致 ✅ |

> 这 5 条节点编辑器的矛盾说明：**节点编辑器是最近才落地的，文档没跟上**。建议在 Ardot 里把节点编辑器画板标注为"已实现"，避免误判为待做。

---

## 五、需要修订设计稿的项（按优先级）

### P0 —— 会导致实现错误

| # | 设计稿位置 | 问题 | 建议 |
|---|-----------|------|------|
| 1 | `5:2564` 底栏 + `5:2601`/`5:2606` 字段 | 「入点影响/出点影响 33.3%」代码无此概念 | 拍板 A/B/C 方案（§2.2），然后二选一 |
| 2 | `5:2625` 曲线色 `#76C46A` | 代码用 `palette::ACCENT` 蓝色画曲线，与选中态同色 | 决定是否新增 `CURVE` 色常量 |
| 3 | `5:2552`/`5:2554` 速度/数值双 Tab | 代码是"定义空间切换"，桌面无速度图 | 拍板（§2.3） |
| 4 | `5:2596` 插值类型 6 项 | 代码是 `Ease`(5) × `CurveShape`(3) 两层 | 重画为两个控件 |
| 5 | 工具栏关键帧快捷键 | `U`/`J`/`K` 与代码语义冲突 | 按 `KEYBOARD-SHORTCUTS.md` 重写 |

### P1 —— 设计质量

| # | 问题 | 建议 |
|---|------|------|
| 6 | 色板比代码亮 5–12 色阶，角色错位一格 | 以 `theme.rs` 为准整体重绘，或明确以设计稿为准并改代码 |
| 7 | 文本只有两级灰（设计三级） | 代码补 `TEXT_WEAK`，或在设计稿降为两级 |
| 8 | 时间轴行高 44 vs 代码 24 | 确认设计稿是否 1.5× 缩放 |
| 9 | 曲线手柄 12.7/8.5px，代码菱形半径 5px | 两边都改到 ≥16px 命中区 |
| 10 | 蓝字配 `#304559` 对比度 2.9:1 不达 AA | 代码 `#378EF0` 配深底对比度更好，建议统一到代码值 |

### P2 —— 补齐设计稿

| # | 补充项 |
|---|------|
| 11 | FX Console（`Ctrl+Space`）浮层 —— 代码已有，设计稿无 |
| 12 | 表达式工作区 —— 代码有 QuickJS，设计稿只在"建议"里提 |
| 13 | `Shift+F3` 时间轴↔值曲线同面板切换 |
| 14 | `P/S/R/T` 属性筛选交互 |
| 15 | 3D 摄影机 / 混合模式 / 轨道遮罩 / 音频面板 |
| 16 | MCP 模式（`--mcp-ui`）的用户可见说明 |

---

## 六、我的设计文档需要修正的条目

对照代码后，`Motion-Studio-UX.md` 需改动：

| 章节 | 现内容 | 改为 |
|------|-------|------|
| §3.1 颜色表 | 设计稿实测色 | 标注"设计稿值" vs "代码值（权威）"两列 |
| §4.2 关键帧快捷键 | `U`/`Shift+U`/`Ctrl+U` | `Alt+Shift+P/S/R/T` 加/减关键帧，`U` = 筛选已打帧属性 |
| §4.2 播放 | `,`/`.`、`J`/`K` 播放 | `PageUp/PageDown`、`Ctrl+←/→`；`J`/`K` = 跳关键帧 |
| §4.3 撤销 | 栈深 200 | **128 条 / 32MB** |
| §5.1 工程主页 | 整章 | 标注"设计稿有，代码无，需确认是否排期" |
| §5.2 合成设置帧率 | 12–120 | **1–240 整数**（README 明确） |
| §5.7 时间轴行高 | 44 | 对齐代码 `ROW 24` |
| §5.8.4-B 修饰键矩阵 | 4 条建议 | 标注"代码均未实现"，并加 `F9`/`Shift+F9`/`Ctrl+Shift+F9` 三档 |
| §5.8.4-D 影响权重 | 档位建议 | **待你拍板**，先不写死 |
| §5.8.4-E 插值类型 | 6 平级 | 改为 `Ease`(5) + `CurveShape`(3) 两层 |
| §8 实现优先级 | — | 补「FX Console / 表达式 / 3D 摄影机已在代码中」 |

---

## 附录：核查方法

```powershell
git clone --depth 1 https://github.com/MAD-Producer/motion-studio.git ms
# 关键文件
crates/motion-model/src/curve.rs        # CurveSpace / CurveShape / 贝塞尔数学 / 校验
crates/motion-model/src/animation.rs     # Ease / Keyframe / Track / SpatialTangents
crates/motion-core/src/editor.rs         # History / gesture / apply_batch
crates/motion-core/src/command_registry.rs
desktop/ui/src/theme.rs                  # 权威色板与尺寸
desktop/app/src/panels/animation_controls.rs   # 桌面曲线编辑器
desktop/app/src/panels/timeline_graph.rs       # 时间轴内嵌值曲线
desktop/app/src/shortcuts.rs             # 快捷键真值
android/app/src/main/java/com/motionstudio/editor/CurveEditor.kt
desktop/PANELS.md · desktop/KEYBOARD-SHORTCUTS.md · desktop/SDK-PARITY.md
```

**关键命令**
```powershell
# 验证影响权重确实不存在
Select-String -Path crates\**\*.rs -Pattern '\binfluence\b|\btension\b|\bbias\b'

# 验证桌面曲线用什么颜色画
Select-String -Path desktop\app\src\panels\animation_controls.rs -Pattern 'palette::'
# → paint::line(list, samples, palette::ACCENT)   ← 蓝色，不是设计稿的绿色
```