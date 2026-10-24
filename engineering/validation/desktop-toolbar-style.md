# 桌面工具栏填充与描边验收（#231）

工具栏原先显示固定色块和 `3 px`，现在读取所选矢量图层的实际填充、描边和宽度。颜色弹窗提供 RGBA 输入与拖动、填充规则、无填充／无描边及应用／取消；宽度直接支持数值输入和 scrub。多选显示混合值，修改颜色的单个通道时保留各目标其他通道。

目标是所选矢量图层的全部已有绘制样式，弹窗显示图层数量；并非修改某个钢笔节点或蒙版样式。无选区、含非矢量／锁定图层、蒙版工具、观察模式和非整数局部帧都有禁用状态和状态栏原因，不跳过不支持目标后偷偷修改其余目标。没有新增渐变模式或绘制能力。

## 实现与边界

- 复用原生 `NumberEditor`、`Dropdown`、`color_well`、属性参数描述及已有历史事务。常规颜色／宽度修改使用 `SetGroupParameter`，不随每次拖动上传整个路径。启停绘制和填充规则变更使用现有矢量替换命令，保留无关几何、轨道、描边选项和操作器。
- 颜色弹窗的一轮实时预览只占一轮历史；应用保留，取消／Escape／焦点丢失／目标失效／进入浮窗恢复起点。宽度一次连续拖动或数值提交形成一次撤销。
- 控件与弹窗持有输入直到关闭帧，阻止 Space、Delete、Ctrl+A 和外部点击穿透到时间轴。切入原生浮窗先取消主窗工具栏事务。
- 仅缓存一个工程／合成／修订／帧／选区上下文；会话通过 `Arc` 复用快照，选区失效时释放缓存。没有新线程、文件轮询、重试或重启机制。
- 新尺寸先定义在 `dark.json`；复用既有 field、popup、label 和 color-well 样式。窄窗口下工作区按钮通过原有管理入口访问，避免覆盖样式控件。
- 不修改工程格式、渲染／合成／蒙版像素语义、效果包、第三方依赖或资产；既有渲染基线不需要重建。已检查 `PROVENANCE.md`、`ATTRIBUTION.md`、`NOTICE`。

## 验证与证据

定向命令：

```powershell
cargo test -p motion-desktop --bin motion-studio --features ffmpeg --locked --offline toolbar_style -- --nocapture --test-threads=1
```

在 Windows 原生 GPU 上执行，覆盖：混合选区宽度、数值输入与 scrub、颜色通道预览与取消、动画其他关键帧不变、保存重开、单次撤销、填充／描边停用和恢复、锁定／蒙版／观察与失效目标、快捷键与外部点击隔离、修订缓存、失败日志。原生测试启动隔离子进程，附加真实主窗和属性浮窗的 GPU 表面，并调用生产控件和 Present／CaptureUi；没有另写截图绘制实现。

窗口验证为 1600×1000 和逻辑 900×600，DPI 100%／150%／200%。按逻辑坐标点击真实控件，并对合成区域做预览前／后不相同、取消后相同的像素断言。输入由夹具显式提供；不声称模拟了 OS 键盘、IME 或人工切换前台窗口。

截图为本程序测试夹具的实际 GPU 输出；下列弹窗截图仅裁剪空白区，没有重绘控件：

| 状态 | 截图 |
| --- | --- |
| 填充及 RGBA | [实际填充弹窗](desktop-toolbar-style/fill-popup.png) |
| 填充规则下拉菜单 | [实际规则菜单](desktop-toolbar-style/fill-mode-menu.png) |
| 描边弹窗 | [实际描边编辑](desktop-toolbar-style/stroke-popup.png) |
| 多选混合值 | [实际混合选择](desktop-toolbar-style/mixed-selection.png) |
| 最小窗口 | [900×600](desktop-toolbar-style/toolbar-minimum.png) |

结果：定向 **10 passed / 0 failed / 0 ignored**；既有 keyboard 9 项、composition 4 项、真实原生双窗输入路由 1 项全部通过。样式 consumer 检查、改动文件 rustfmt 与 `git diff --check` 通过。

带 FFmpeg 的本地 debug 产品构建通过：`cargo build -p motion-desktop --bin motion-studio --features ffmpeg --no-default-features --locked --offline -j1`。不是 release 验证或正式安装包。现有编译警告未在本改动中批量修正。

可运行开发目录：`artifacts/desktop-toolbar-preview/`。使用既有 runtime manifest 的 SHA-256 和 PE 架构校验，附带 FFmpeg 动态库、来源和许可证；在 PATH 只保留 Windows 系统目录的环境下，`--command-list` 返回成功且 stderr 为空。产品 SHA-256：`912d3a9531d87b3fd6087c9e8c3d4d65afe95bda124094ae58559f9af9dacdf0`。

本地完整截图：`artifacts/pr-ci-followups/desktop-toolbar-native/`；定向日志：`artifacts/pr-ci-followups/desktop-toolbar-tests-final.log`；构建日志：`artifacts/pr-ci-followups/desktop-toolbar-product-build.log`；启动输出：`artifacts/pr-ci-followups/desktop-toolbar-startup.stdout.json`。归档裁剪的来源哈希见 [captures.json](desktop-toolbar-style/captures.json)。远程 CI 尚待当前提交验证，不以本地结果替代其他平台验收。

首轮错误信道夹具曾因变量遮蔽而未保留外层组，未触发预期的 ID 耗尽；修正夹具后保留原断言。首轮截图曾缺少中文字体和合成 Present：改为产品字体配置、真实 Present 后重新截图；未将初次空白画面作为视觉验收通过。

## 错误处理审计

| ID | 类别与覆盖 | 证据／限制 |
| --- | --- | --- |
| TB-E01 | 新观察：矢量快照反序列化失败 | 真实畸形快照重复 120 次，首次 Warn、第 60 次一次 Error。复用现有 panel observation 的进程生命周期累计计数；健康相邻项目不重置，计数饱和。并非连续失败或一秒阈值。 |
| TB-E02 | 新观察：轨道采样错误与禁用回退 | 向内存缓存注入错误携带分离维度的标量轨道，经过生产 render 路径；采样站点同样有界升级。反序列化本身拒绝这种轨道，未放宽该契约。正常无填充／无描边不计为错误。 |
| TB-E03 | 新观察：弹窗采样及命令准备失败 | 真实轨道采样 Err 和组 ID 耗尽；关闭编辑、保留原始 cause，输出 Error 和用户提示，无修改命令流出。 |
| TB-E04 | 既有覆盖：工作线程命令、保存、GPU 呈现、panic／阻塞 | 继续经过既有编辑／历史／Present 通道；本改动未新建同步 worker、panic hook、超时或关闭协议。没有声称重新验收全部原有失败路径。 |
| TB-N01 | 正常边界：空选区、非矢量、锁定、蒙版、观察、分数帧、取消 | 显示禁用／正常取消，不将它们当作错误；未吞掉引擎修改失败或改变协议返回值。 |
| TB-C01 | 默认级别的文件／stderr／stdout | 隔离子进程移除 `MOTION_LOG` 覆盖，核对实际故障 Error 同时在文件和 stderr，stdout 无故障记录；正常缺省分支不产生 toolbar 故障。 |

本机 Vulkan loader 仍报告已失效的 Epic Overlay JSON 路径，属于既有系统环境记录；未修改系统注册表或静默屏蔽。修正字体夹具后的截图不再出现缺字降级。

## DESIGN §0.7 处置

以下基线逐项保留，原因均为本改动只接通工具栏，没有重绘原有属性树、效果面板或时间轴。不能据此宣称这些历史问题已经修复：

| 既有位置／问题 | 处置 |
| --- | --- |
| `path_controls.rs` 六处字面量行步进 | 保留；不修改既有属性树行规划。 |
| `path_controls.rs` 两列硬编码分割 | 保留；不改属性树列宽。 |
| `path_controls.rs` 多种按钮高度 | 保留；工具栏复用数值编辑器，未改旧按钮。 |
| `path_controls.rs` 自绘数值背景 | 保留；新工具栏使用既有 number 样式，不扩散旧实现。 |
| `path_controls.rs` 全宽布尔按钮 | 保留；不涉及该布尔控件。 |
| `effect_controls.rs` 字面量行高与步进 | 保留；未改效果列表。 |
| `effect_controls.rs` 文本布尔按钮 | 保留；未改效果开关。 |
| `effect_controls.rs` 全宽枚举按钮 | 保留；新填充模式使用真正的 Dropdown。 |
| `effect_controls.rs` 手动 hover alpha | 保留；新控件使用已有样式状态。 |
| `effect_controls.rs` 层级缩进 | 保留；本次不重排效果树。 |
| `timeline.rs` 十一列硬编码槽位 | 保留兼容基线；仅验证弹窗不向时间轴漏输入。 |
| `timeline.rs` 吸附线颜色 | 保留兼容基线；不涉及吸附绘制。 |

本次没有对所有面板、五个工作区和全部可访问性状态作整体合规声明。其他平台的原生视觉、真实 OS IME 和人工焦点切换仍需相应平台验收。
