# 插件编辑器 M2 验收

范围：`feat/plugin-editor` 的原生布局运行时、独立示例参数与真实着色器预览。
这不是 M3 的拖拽作者界面，也不宣称 M4/M5 已完成。
实现/入口见 [运行时说明](../host-api/plugin-editor-runtime.md)。

## 实际检查

| ID | 检查 | 证据与结果 |
| --- | --- | --- |
| M2-V01 | 参数手势、共享绑定、取消与撤销 | 6 项 CPU/原生控件测试通过；连续更新一次撤销，失焦恢复原值，重复绑定不增加参数槽，无效值拒绝 |
| M2-V02 | 真实着色器输出 | 显式 Windows GPU 验收通过：原生拖动改变 PNG 指纹，撤销恢复，错误值拒绝后像素保持；公共参数更新不重编译管线 |
| M2-V03 | 图与 WGSL 两种来源 | WGSL 示例实际编译/安装/调参；节点来源保存为 `.msplugin` 后重新读取、实际编译、安装并更新公共颜色参数，像素变化 |
| M2-V04 | 窄窗与控件层次 | 中文宽版、中英文宿主文本、窄版 200% 绘制、颜色弹层、下拉框、作者预览槽截图已检查；控件按行排布，不压缩数值命中区域 |
| M2-V05 | 弹层与隐藏预览 | 弹层覆盖 GPU 区域的像素为弹层背景；切到无预览的标签页时 `node_preview=None`，不在参数区回退绘图 |
| M2-V06 | 既有节点预览 | 6 项真实 GPU 回归通过，涵盖数值/字节资源、公共参数、时钟/alpha、管线缓存、预算拒绝、临时输出与保存/导出 |
| M2-V07 | 源文件错误信道 | 既有 M1 默认级别错误测试重跑通过；新准备阶段受控阻塞时已发布 resource_decode 状态和 Info start，真实 PNG 解码失败与受控 panic 进入日志/stderr，stdout 无产品日志 |
| M2-V08 | 产品构建与启动 | 带 FFmpeg 构建通过；实际启动 GUI 后取得窗口客户区截图，正常关闭退出 0；缺失源文件显示错误提示，关闭后退出 1，两个模式 stdout 均为空 |
| M2-V09 | 风格与格式 | 现有 `verify_style_consumers()`、新增 Rust 文件格式检查和 Git diff whitespace 检查通过。没有改变主题 token 的校验上限或放宽任何验收断言 |

原生 GPU 验收显式 `#[ignore]`，由专门命令运行，不把截图/成本采样加入默认 CI 门禁。
产品构建保留原有 29 条桌面警告，测试构建 26 条；新增插件模块未引入警告。

```powershell
cargo build -p motion-desktop --no-default-features --features ffmpeg --locked --offline
cargo test -p motion-desktop --no-default-features --features ffmpeg --locked --offline --bin motion-studio --no-run
motion-studio-tests.exe plugin_editor::tests:: --nocapture
motion-studio-tests.exe plugin_editor::window::acceptance::native_layout_controls_render_and_update_shader_without_recompilation --exact --ignored --nocapture
motion-studio-tests.exe node_preview::tests:: --ignored --test-threads=1 --nocapture
```

## 原始失败与修正

- 首次主题运行测试拒绝超出 512 的窗口尺寸 token；删除这两个 token，窗口尺寸改由既有面板/画布尺寸组合，没有增加例外或修改样式校验。
- 首次 GPU 验收被示例 WGSL 的保留字 `target` 拒绝；修改自行编写的示例变量名，保留编译器断言。
- 早期启动脚本捕获到白色客户区，保留该 PNG、当时的 Debug 日志及运行器超时。
  日志显示首个 Present 已返回成功，不能把白图等同于 GPU 调用阻塞。
  早期脚本只按 PID 选首个可见窗口，未校验标题和有效客户区，还反复恢复窗口；
  后续确实遇到零尺寸辅助窗口。验收脚本改为同时核对 PID、产品标题、有效客户区，
  捕获时仅将本次子进程的产品窗口临时置顶，防止其他验收窗口遮挡，随后立即还原层级。
  修正后的正常/文件缺失窗口检查通过；无法追溯证明每次旧白图的成因，不宣称修复驱动问题。
- 窗口遵循 `RedrawRequested` 提交，`about_to_wait` 仅轮询和请求重绘；正常未呈现回复
  保留绘制需求。首帧增加一次性的开始/状态与延迟观测。这些是窗口调度与诊断变更，
  与验收脚本修正分别记录，没有靠持续无条件绘制或重启掩盖失败。
- 第一次样式失败、WGSL 失败及早期窗口脚本失败的原始输出保留在证据目录。

## 观测与协议审计

| ID | 类型 | 处置 |
| --- | --- | --- |
| M2-O01 | 新观测 | 源准备阶段原子状态 0/1/2；进入读取/编译及资源解码之前记录 Info start；完成 Debug，作用域清理；受控阻塞、真实失败与 panic 验证 |
| M2-O02 | 已有覆盖复用 | 源检查沿用 M1 bounded Display/Debug、原始错误 source 链、panic 钩子、默认文件/stderr/stdout 规则；引擎 GPU 命令使用既有 request/in-flight 边界 |
| M2-O03 | 新观测 | 首次 surface 提交前一次性 Info；成功后状态清除、Debug 完成；延迟记录附带安装、尺寸、待呈现及引擎在途状态。健康 Present 不逐帧记录 start/done |
| M2-O04 | 正常分支 | 空撤销栈、未打开弹层、未连接时间上下文、过期 stamp、窗口关闭后接收方退出均不是故障；不增加错误日志 |
| M2-O05 | 功能/协议边界 | 校验参数后更新 GPU uniform；拒绝时保留旧效果；颜色编辑内部手势整体合并，取消恢复预览快照；原生弹层独占输入；没有吞掉 panic 或修改成功返回值 |
| M2-O06 | 有界状态 | 契约最多 32 参数/256 布局组件；128 历史快照；独立窗口只加载一个源工程；每类参数/尺寸更新最多一个在途请求；不打印 WGSL 原文、图像字节或无限上下文 |
| M2-O07 | 现存环境问题 | 本机 Epic Vulkan overlay 缺失 JSON、物理内存探测回退及一个 fontdb 字体告警仍存在；不归入本补丁已修复项，也不屏蔽这些日志 |
| M2-O08 | 未覆盖/限制 | 原生解码/编译不返回时只能保留阶段和开始信号；本阶段没有超时重启/线程终止。真实 OS DPI 切换、IME、键盘遍历、长名称矩阵、macOS/Linux 与宿主断连尚待验收 |

## DESIGN §0.7 处置

以下条目在本补丁均为 **保留**：本次不改变这些面板的布局/绘制/交互，不能计为修复。
唯一面板模块变化是让已有颜色预览工具对本 crate 可见。

| 基线位置 | 本次状态/理由 |
| --- | --- |
| path_controls 六处行步进 | 保留：未改动该文件 |
| path_controls 两列分割 | 保留：未改动该文件 |
| path_controls 按钮高度 | 保留：未改动该文件 |
| path_controls 数值背景 | 保留：未改动该文件 |
| path_controls 布尔按钮 | 保留：未改动该文件 |
| effect_controls 行高 | 保留：未改动该文件 |
| effect_controls 布尔 | 保留：未改动该文件 |
| effect_controls 枚举 | 保留：未改动该文件 |
| effect_controls 手绘悬停色 | 保留：未改动该文件 |
| effect_controls 层级 | 保留：未改动该文件 |
| timeline 11 列坐标 | 保留：兼容基线，未改动该文件 |
| timeline 吸附线颜色 | 保留：未改动该文件 |

§12 本轮验证了宽窄绘制、行对齐、原生手势/取消/撤销、裁剪命中、弹层、GPU 输出与真实入口。
200% 截图调整的是逻辑尺寸和字形 atlas 密度，不是更改 OS 显示缩放；真实 DPI、IME、
主/浮动宿主窗口和其他平台仍为待验证，没有用编译成功或绿色数量代替验收。

## 证据索引

`plugin-editor-m2/` 保存原始日志、JSON 报告与自行渲染的截图。
所有截图均来自本仓库原生控件及 GPU，不是生成式参考图。
证据目录使用局部 Git 属性保留原始换行和日志末尾空行；`manifest.json` 记录文件字节数与 SHA-256，
不通过裁剪失败日志或清理输出改变验收证据。这些属性仅作用于该证据目录。

- `wide-zh.png`、`narrow-2x.png`：宽/窄原生渲染。
- `color-popup.png`、`enum-popup.png`、`popup-over-preview.png`：实际控件弹层及图层顺序。
- `runtime-normal.png`、`runtime-missing.png`：独立产品进程的真实窗口客户区。
- `shader-before.png`、`shader-after.png`、`shader-undo.png`：调参/撤销像素证据。
- `graph-before.png`、`graph-after.png`：节点来源参数证据。
- `preparation.log`、`preparation.stderr.txt`：受控阻塞、真实解码错误及 panic 的默认级别证据。
- `startup-white-observed.png`、`startup-white-debug.log`：早期未核对标题的窗口捕获及原始日志。
- `native.log`、`cpu.log`、`node-preview.log`、`source-check.log`、`runtime-report.json`：验收原始输出。
