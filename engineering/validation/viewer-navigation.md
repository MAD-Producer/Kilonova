# 合成查看器导航验收（#203）

## 功能与边界

- VIEW-01：独立的临时倍率/中心；滚轮以鼠标下的合成点为锚缩放，中键平移，原生数值字段可输入/拖动倍率。Fit 清除手动状态并随窗口适配。手动范围 1–6400%，自动 Fit 可以低于 1%。
- VIEW-02：显示、拾取、钢笔/遮罩投影、变换手柄共用虚拟画布矩形；绘制和命中独立受内容区裁切。GPU 使用窗口内的有效 viewport，以 UV 变换裁切放大画面，避免窗口外 viewport。移动图层先换算到合成尺寸，避免非整数倍率的视口舍入造成漂移。
- VIEW-03：视图状态不进入 Project、工程格式、历史或导出。主窗/浮窗共享既有面板所有权；工程/合成切换重置，Esc/失焦/路由边界取消未完成的平移/倍率拖动。
- VIEW-04：保留右键观察操作；观察视图中 Alt+滚轮缩放观察摄影机。普通/Ctrl+滚轮操作画布，不切换摄影机。Space 保留原有播放路由，编辑倍率时由字段取得键盘输入。
- VIEW-05：控件尺寸先在 dark.json 定义；倍率使用 NumberEditor，Fit 使用按钮。没有修改右键菜单。窄栏依次隐藏时间码、视图数量和空间不足的右侧按钮，避免覆盖倍率/Fit。
- LIMIT-01：放大显示当前预览质量的纹理，不绕过既有质量/内存策略请求额外的全分辨率渲染；导出不继承查看器倍率/中心。
- LIMIT-02：既有无样式 viewport helper 保留默认几何兼容测试；实际主窗/浮窗/截图/编辑手柄使用当前窗口样式。

## 错误与可观测性审计

| ID | 类型 | 处置与证据 |
|---|---|---|
| OBS-01 | 新增观测 | 非空画布映射不可用：本面板连续计数，首次 Debug，第 60 次 Error，随后抑制；计数饱和于 61（已报告标记），恢复/工程切换清零。仅记录固定操作/阶段、哈希标识、尺寸、计数；不推断持续秒数，无用户文本或逐工程注册表。 |
| OBS-02 | 既有覆盖 | 非法 GPU 映射返回原始 invalid composition canvas image，沿既有 Present 错误信道写文件和 stderr；没有清错、重试、重启或修改返回值。受控视频解码阻塞时带导航的保留画面继续绘制编辑器，未就绪仍返回 Ok(false)。 |
| OBS-03 | 正常分支 | 空视口、无附着图形表面、正常预览等待、取消、无位移/编辑对象均非故障。非法数值由原生字段显示错误，不记为运行故障。健康 Present 不新增 Info、文件读取或逐帧告警。 |
| OBS-04 | 功能边界 | 本次新增导航/裁切，不修改工程/历史/导出协议，不引入线程或阻塞操作。Presenter 复用固定 uniform/bind group；纹理复用键不含平移/缩放，视图重绘不重新执行合成效果。 |
| OBS-05 | 既有覆盖 / 待处理 | 默认主题 19 浮窗验收触发既有时间轴 U+2194 缺字 Error（timeline.rs 的箭头文本，本次未改）。原始 stderr 保存在 viewer-final-default-floats.log；本次不把该记录清除或宣称完整字体状态合格。查看器截图文字已逐张复核。 |

## 验证记录

- 坐标测试：鼠标锚定、多种宽高比、平移往返、Fit/手动 resize、非法输入原子拒绝。
- 实际 GPU 像素测试：红/绿图案的放大、平移、缩小边界及裁切；原有颜色空间和 GPU 失败验证保留。3 项通过。
- 实际引擎手柄测试：25.137%、111.7%、313.33% 的投影/拖动、一次撤销还原、屏幕外手柄不能捕获面板外输入。1 项通过。
- 原生输入专项：中英文滚轮、中键、失焦取消、Fit 点击、合成切换、拾取换算；不发出摄影机/工程编辑动作。2 项通过。
- 默认 Info 子进程已通过：覆盖 60 次阈值、抑制、恢复后第二次升级，验证 Error 文件+stderr、stdout 隔离；受控原生解码覆盖非法映射原始信道和阻塞期间带导航呈现。通过。
- 新增错误路径后的桌面全套 418 通过、1 项既有音频设备测试忽略（142.13 秒）；最终边界测试 3 项、输入/日志专项 3 项通过。
- 最终 FFmpeg 优化构建通过（14 分 10 秒，22 项既有编译警告）。从发布 ZIP 解压后的程序运行完整 ci_desktop.py：中英文主窗、节点、共享 MCP、19 类样式覆盖浮窗全部通过；另以默认主题运行中文 19 浮窗，退出 0，确认 overrideActive=false。
- 开发构建触发既有 smoke 总时限；首轮优化验收发现截图辅助代码错误要求外部样式，已修复为正常默认回退。保留原失败记录，未放宽阈值。最终默认配置验收通过，26/22 高的倍率字段/按钮文字完整。
- 真实截图已逐张复核：final/screens/ 下的 smoke-zh.viewer-fit.png、smoke-zh.viewer-zoom.png、smoke-zh.viewer-pan.png，以及 default-panels-zh.panel-3-zoom.png、default-panels-zh.panel-3-pan.png。主窗/浮窗裁切、倍率文字、手柄随图像对齐均通过；本机 OS 非默认 DPI 的人工验收仍待完成。
- 可运行 ZIP：final/package/MotionStudio-validation-windows-x86_64.zip；83,615,994 字节；SHA-256 44f44c43f3bccb4a8c2652b5b3968000b6723ec25da18b013a733d24acc437a1。二进制源码 c792018cd52fb4386ecc8b9bfb9013d5ec0de10c（后续仅更新本验收文档）。包内 FFmpeg 及许可证共 12 个文件逐项哈希通过。
- 本机截图/构建：E:/Dev/aem/artifacts/viewer-navigation/；日志：E:/Dev/aem/artifacts/multiple-camera-layers/viewer-*.log。
- 已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE；无新增第三方依赖/资产，着色器变换和测试图案为独立实现。

## DESIGN §0.7 逐项处置

retained 不代表原缺陷已修复；本次导航不重排属性树。

| ID | 基线 | 状态与原因 |
|---|---|---|
| D01 | path_controls 六处纵向步进 | retained：不改属性行；仅投影/裁切和画布提示接入导航。 |
| D02 | path_controls 双列宽 | retained：未改属性双列。 |
| D03 | path_controls 按钮高度 | retained：保留路径按钮命中；查看器右侧按钮保持既有 22 高。 |
| D04 | path_controls 自绘字段 | retained：不替换路径字段；新倍率为原生 NumberEditor。 |
| D05 | path_controls 布尔按钮 | retained：未改路径布尔属性。 |
| D06 | effect_controls 行高/步进 | retained：未改效果面板。 |
| D07 | effect_controls fx/— | retained：未改效果启用控件。 |
| D08 | effect_controls enum | retained：未改效果枚举。 |
| D09 | effect_controls hover alpha | retained：未改效果悬停。 |
| D10 | effect_controls 层级 | retained：未改效果层级。 |
| D11 | 时间轴 11 列坐标 | retained：保留列、关键帧、时间换算兼容基线。 |
| D12 | 时间轴吸附颜色 | retained：未改吸附或主题。 |

DESIGN §12 只验收本次查看器及相关坐标/输入路径；其余面板视觉基线、完整状态矩阵、OS Reduced Motion 不在本次完成声明内。
