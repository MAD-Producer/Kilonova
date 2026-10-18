# UI 顶点缓冲区越界修复

基于 #333 的关键帧导航实现，处理用户提供的 `Queue::write_buffer` 报错。生产改动只涉及 UI 绘制器容量和既有桌面错误边界，没有调整面板样式、输入行为、主题、依赖或第三方资产。

## 原因与功能修复

| ID | 分类 | 变更 / 证据 |
| --- | --- | --- |
| BUF-01 | 已定位缺陷 | `SolidVertex` 为 88 字节，原固定容量 65,536 对应日志中的 5,767,168 字节；66,552 个顶点写入 5,856,576 字节会越界。真实 GPU 测试对原尺寸 buffer 执行该写入，校验错误同时包含这两个字节数。 |
| BUF-02 | 功能修复 | 图形与文字分别按实际上传大小检查并扩容，几何增长且不超过 `device.limits().max_buffer_size`。容量跨帧保留，不截断顶点、不忽略尾部绘制，不增加每帧日志或文件 IO。 |
| BUF-03 | 功能修复 | 两种容量都在上传前验证，超出字节或 u32 draw 索引上限返回结构化 `PaintError`，包含通道、顶点数、需求字节数及设备上限；恢复 CPU 缓存，下一次正常绘制可继续。 |
| BUF-04 | 真实像素回归 | 超过原容量的图形末尾绿色四边形、文字末尾红色字形以及图形/文字交错绘制都读取 GPU 像素断言；连续绘制、空帧后保留容量。非二次幂的设备上限内也能完整绘制。 |
| BUF-05 | API 边界 | `Painter::render` / `render_scaled` 现在返回 `Result<(), PaintError>`；仓库桌面、framework 示例及已有 GPU 测试调用点全部处理结果。外部调用方需处理新增返回值。 |
| OBS-01 | 新错误传播 | UI 截图、无合成视图 Present 直接传播错误；有合成视图的 host overlay 保持原有 `()` 协议，回调外保留错误并交给 Present 回复。不得把 `Ok(false)` 覆盖为成功；host 同时失败时保留主错误并追加 overlay 原因。窗口 ID 保留在错误上下文中。 |
| OBS-02 | 既有覆盖 | 复用 engine `respond` 与现有日志设施，保留请求 ID、操作和耗时，Error 写文件及 stderr，stdout 保持协议用途；健康 Present 不新增记录。没有新增重试、线程、超时或吞错分支。 |
| OBS-03 | 实际失败证据 | 限制真实设备容量，生成同一 5,856,576 字节帧并取得实际 `PaintError`；通过 host 成功、`Ok(false)` 和主错误三种结果交给现有回复/日志边界。子进程默认 Info 验证调用方原错误、文件、stderr 及 stdout 隔离。并非以模拟成功返回代替失败测试。 |
| OBS-04 | 正常分支和边界 | 小帧、空帧及容量复用不算故障。保留原有 order/atlas 观测和 GPU 设备错误处理；本次没有解决 CPU OOM、驱动崩溃、设备丢失或所有其他 GPU buffer 的潜在问题。未新增降级计数器或抑制策略。 |

## 验证

证据目录为 `artifacts/ui-vertex-buffer-growth/`。

- 最终 `cargo test --locked -p motion-ui --all-targets -- --test-threads=1` 通过：87 项，0 失败，含 5 个新增容量回归及原有真实像素、绘制观测、路径、样式、停靠和 framework 示例编译。`ui-tests.log`。
- 带 FFmpeg 完整桌面测试通过：469 passed / 0 failed / 1 ignored，124.41 秒，`desktop-tests.log`。包含新增真实 GPU 容量拒绝的默认日志/回复测试；没有改变既有值、像素、撤销或超时断言。
- Python 工具回归运行 54 项、1 项原有跳过，30.573 秒，`tools.log`。
- 带 FFmpeg 的优化构建通过，8 分 36 秒、21 项既有编译警告；构建报告 `release/build.json`、日志 `release-build.log`。本机验证仅将 release package codegen-units 设为 16，保留优化和 thin LTO；不修改仓库构建配置。
- 完整 `tools/ci_desktop.py` 原生验收通过，退出码 0，`release-native.log` 记录 `Desktop languages, styled floating windows, live node rendering and shared MCP passed.`。真实主窗、原生浮窗、中英文、样式覆盖、节点渲染及共享 MCP 的报告与像素捕获见 `release/screens/`；沿用原验收超时预算，未以容量单元测试代替原生窗口检查。
- 最新提交的远端 CI 独立跟踪，不把本机结果算作远端通过。

## DESIGN §0.7 / §12

Path 六处步长、两列分割、混合按钮高度、数值背景、全宽布尔按钮五项均 retained：本次不修改属性行代码。Effect 行高、布尔、枚举、hover alpha 和层级五项均 retained：本次不修改效果控件。Timeline 固定列坐标、吸附线颜色两项均 retained：本次不修改时间轴布局、颜色或命中。没有新增 token 或复制参考资产。

本次为绘制容量及错误传播修复，现有正常帧像素应保持不变；实际 GPU 回归验证尾部绘制不丢失。多 DPI、其他平台和整套 DESIGN 合规不由本记录宣称完成。
