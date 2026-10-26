# 字符／段落真实编辑与文字重排（#239）

实现与使用说明见 [desktop/TEXT.md](../../desktop/TEXT.md)。字体、字号、行距、字距、外描边、段落对齐、文字框宽度和缩进保存到必填 `TextStyle`；工程版本改为 27，没有旧工程迁移。未增加依赖、字体、图标或外部资产。测试截图为本程序实际 GPU 输出；原生中文夹具读取本机已安装的 Microsoft YaHei，不将字体二进制纳入仓库或证据包。

字符／段落使用同一草稿；数值拖动和文字局部撤销只修改草稿。显式应用经过现有串行引擎线程、字体栅格和资源预检后，复用 `apply_command_batch` 一次提交。字体缺失、字形缺失、无效样式和旧工程凭据不允许部分修改文字、字体或历史。开始任务停止播放；窗口事件处理不等待重排。文字布局仍是已有 LTR 字形、字偶距、逐字符换行语义，没有引入复杂文字塑形。外描边使用填色与方角，不是独立颜色描边。

无附加排版的样式调用原栅格路径，测试逐字节比较旧输出；新增样式仅改变明确请求这些样式的文字。既有填色事务和颜色弹窗保留；排版草稿需应用或取消后再修改填色，避免用颜色修订隐式重基准未完成草稿。

## 需求与证据

已同步主线 `da2fd4af`；生产实现提交为 `c75d06a9`。最终结果见 [结果 JSON](text-style-editing-results.json)，原始日志、23 张原生截图、独立进程回复、PNG／MP4 和重现脚本见 [证据包](text-style-editing-evidence.zip)（SHA-256：`84bb56a9eecdd315142051297edf62117178c9f5b4301b228f77d465435264a1`）。包内保留首轮失败，不含字体二进制和效果包。

最终模型／核心 85 组共 461 项通过，媒体像素 4 项、桌面文字 6 项、辅助面板 4 项通过；原生夹具先通过 20 图版本，再通过增加中文完整样式后的 23 图版本。默认错误信道在独立进程核验 3 条真实失败的文件／stderr／stdout 边界。以上均没有失败。

带 FFmpeg 的最终构建成功；仅将程序目录及 Windows System32 放入 PATH 后，命令目录 JSON、真实视频解码及中英文完整桌面冒烟通过。七个 DLL 的 SHA-256 与固定发行清单逐项相同。独立 MCP 进程重开中文完整样式，样式逐字段相同、640×360 PNG 逐像素相同。便携英文夹具的 MP4 两帧实际解码后与正确样式 IoU 均为 `0.994840`，与原样式仅为 `0.003815`；撤销前冻结的电影仍保留已应用的排版样式。

实际样式：[字符](text-style-editing/zh-styled-character.png)、[段落](text-style-editing/zh-styled-paragraph.png)、[原生浮窗](text-style-editing/zh-float-applied.png)、[字体菜单](text-style-editing/zh-font-menu.png)、[真实缺字错误](text-style-editing/zh-font-error.png)、[英文小窗口](text-style-editing/en-character-min.png)、[英文 2 倍比例段落](text-style-editing/en-paragraph-dpi2.png)、[英文浮窗](text-style-editing/en-float-applied.png)。

复现命令（构建环境须准备仓库规定的 FFmpeg 和图形驱动；原生中文夹具还需本机已安装 Microsoft YaHei）：

```powershell
cargo test -p motion-model -p motion-core --tests -- --test-threads=1
cargo test -p motion-media --test text_styles -- --test-threads=1
cargo test -p motion-desktop --bin motion-studio --features ffmpeg text_style -- --test-threads=1
cargo test -p motion-desktop --bin motion-studio --features ffmpeg panels::auxiliary::tests -- --test-threads=1
cargo test -p motion-desktop --bin motion-studio --features ffmpeg text_style_native_controls_pixels_transactions_and_floating_input -- --ignored --test-threads=1
cargo build -p motion-desktop --bin motion-studio --features ffmpeg
```

可设置 `MOTION_TEXT_STYLE_NATIVE_ARTIFACTS`、`MOTION_TEXT_STYLE_EXPORT_ARTIFACTS`、`MOTION_TEXT_STYLE_ERROR_ARTIFACTS` 留存夹具输出。编译保留既有其他模块的警告，没有通过全局压制警告或无关改动改变检查结果。跨平台 CI 由本 PR 最终提交另行验证。

| 需求 | 验证入口与范围 |
|---|---|
| 字符与段落真实编辑 | `panels::auxiliary::text_style::tests` 通过生产主窗／浮窗 chrome 编辑同一草稿，比较中文／英文发出的命令；每个属性一行、原生 Dropdown、被动数值无字段底色。 |
| 源像素和确定排版 | `motion-media/tests/text_styles.rs` 比较无样式旧像素、对齐平移、字距／缩进、描边覆盖、CRLF、Tab、负字距及重开字体缓存。布局模型没有窗口比例字段。 |
| 一次撤销与保存 | `engine::text_style_tests::text_styles_are_one_transaction_preserve_transform_reopen_and_cancel_stale_sources` 使用真实宿主验证样式、资源、变换、一次撤销／重做、存储重开及旧修订取消。 |
| 错误原因和回滚 | `text_layout_failures_keep_model_history_and_default_file_stderr_stdout_boundaries` 在独立默认日志子进程触发无效字号、真实缺字和缺失字体文件，保留完整模型、修订和历史，之后成功应用并撤销。 |
| 最终导出 | `export::text_style_png_movie_and_independent_reopen_preserve_authored_pixels` 导出真实 PNG、独立子进程重开后逐像素相等；撤销模型后冻结的 MP4 仍保存排版，实际解码两帧并与正确／错误样式作像素负对照。 |
| 原生窗口和 DPI | 原生 ignored 夹具实际创建主窗／浮窗、运行生产 Present、等待正常图片解码后捕获 GPU 输出。覆盖中文／英文、900×600 与 1600×1000、1/1.5/2 比例、字体菜单、实际应用／撤销和缺字错误恢复。 |
| 输入和焦点 | 同一面板回归覆盖非法整数保留焦点和草稿、局部文本撤销／重做、组合期间 Ctrl+Enter 隔离、窗口失焦释放字段但保留草稿。 |

原生夹具使用受控 Input，通过真实 Shell／窗口呈现与动作队列；没有声称自动注入前台操作系统键盘或输入法候选窗口。macOS/Linux 原生交互和前台 IME 人工体验不在本次 Windows 自动证据中，不把该边界伪装为额外的 issue 完成门禁。

## 错误处理与观测审计

| ID | 处置 | 证据／边界 |
|---|---|---|
| TS01 | 既有观察复用 | `TextLayout` 接入封闭命令枚举、panic 回复和 `text_layout` 诊断名。复用现有串行 worker 在 handler 前发布 in-flight、Info start 和 Debug completion；没有新增健康 Present 日志。通用阻塞／panic 契约沿用原基础设施，未单独注入字体解析器永久阻塞。 |
| TS02 | 新增真实失败验证 | 三种实际失败逐一核验默认文件 Error、stderr Error、stdout 隔离、Info start、完整工程和历史不变、恢复后可再次编辑。日志不含文字草稿 sentinel。没有吞错、放宽预算或重启线程。 |
| TS03 | 新增失败上下文 | 字体导入、栅格、资源发布分别附对象、阶段、字体／路径和原始 typed cause；引擎边界继续携带请求编号和耗时。原因输出经过既有受限 error cause 机制。 |
| TS04 | 正常分支 | 旧路径／文档世代／合成／修订正常取消，不当作后台故障。取消字体选择、取消草稿、字体和栅格缓存命中／未命中、图片异步加载 pending 都不是失败。 |
| TS05 | 正常校验 | 数值、整数、字符数量和布局范围错误显示在草稿中，不记录重复 Error；非法整数保留原输入、选区和局部历史，不能悄悄应用其他字段。 |
| TS06 | 既有协议观察复用 | Shell 非阻塞轮询回复，断连使用 `Engine::poll_reply` 的既有 Error；任务结束后更新真实结果。较晚的成功替换旧 UI 状态，原始 Error 日志仍保留。 |
| TS07 | 新增快照故障观察 | 面板发现无效／缺失必填样式时显示错误，并按实例只报一次 Error、成功读取后复位；不写每帧文件 IO。对象和原因受限，不记录全文。此防御分支不等同于已注入宿主崩溃。 |
| TS08 | 功能和资源边界 | 排版使用显式单任务事务；项目关闭和其他写动作沿现有 busy guard 串行化。宿主批量预检／回滚和图片加载故障沿用既有机制，本次不宣称重做这些机制。 |

## 首轮失败及修正

原始失败日志均保留。初期编译发现测试错误依赖 Raster 的 Debug、按钮 clicked 字段与方法混用、颜色数组缺少固定长度注解，按实际 API 修正。像素断言发现对齐时分数像素叠加改变字形宽度，修正定位算法并保留原宽度断言。

取消草稿测试发现错误提示会推动按钮，修正为稳定提示行并校准滚动高度；整数校验保留输入和焦点，不丢弃错误值。失焦测试发现多行控件默认继续持有字段状态，为这两个面板增加保留草稿的字段释放。

原生第一轮只改逻辑尺寸，实际 surface 尺寸未变化；第二、三轮发现没有等待首次异步图片解码就断言呈现完成。夹具现在调用真实 resize／Present，正常 pending 有界轮询，Err 和 renderError 立即失败，保留实际文字像素和物理输出尺寸断言。截图检查还发现失败后成功时旧状态仍显示错误，修正为显示最新成功结果，原始失败日志保留。

本机既有 Epic Vulkan overlay JSON 缺失、字体库扫描和物理内存探测日志原样保留，没有通过过滤这些日志制造通过。

## DESIGN §0.7 逐项处置

已核对当前面板源码与主线表达式验收记录；原基线行号已变化。下列均保留既有实现，不将其他提交的整改当作本轮修复。

| 原基线项 | 本轮处置 |
|---|---|
| Path 六处字面步进 | retained with reason：本轮不修改 Path 行计划；字符／段落使用现有 row/control token。 |
| Path 双列分割 | retained with reason：不改 Path 列布局；新字段使用统一 label/value 列。 |
| Path 混合按钮高度 | retained with reason：保留 Path 命中范围；新建文字按钮保留已有 control 高度。 |
| Path 自绘数值字段 | retained with reason：不修改 Path 渲染；新数字复用 NumberEditor 与 passive_with_precision。 |
| Path 全宽布尔按钮 | retained with reason：本轮没有新增布尔控制，不宣称完成 Path 整改。 |
| Effect 行高及步进 | retained with reason：当前行计划使用 effect/group/row token，未修改。 |
| Effect fx／— 布尔 | retained with reason：保留当前 Checkbox 路径。 |
| Effect 全宽枚举按钮 | retained with reason：保留当前 Dropdown 路径；新字体和对齐同样使用 Dropdown。 |
| Effect hover alpha | retained with reason：不改效果 hover；新增字段使用语义样式。 |
| Effect 层级缩进 | retained with reason：保留当前 tree.indent 实现。 |
| Timeline 列与 gutter | retained with reason：不修改时间轴坐标、列和命中容差。 |
| Timeline snap guide 颜色 | retained with reason：保留现有兼容基线，不改吸附视觉反馈。 |

§12 本轮验收限于改动涉及的字符／段落布局、语言、窗口、DPI、输入、错误、持久化和最终输出；不宣称整个 DESIGN 的所有整改均已完成。
