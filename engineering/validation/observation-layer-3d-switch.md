# 观察视图中的图层 3D 开关

## 问题和功能边界

用户报告 GitHub 预览版时间轴图层行的 3D 方框点击后连勾选状态都不变。只读获取正在运行的 `E:/Dev/test/motion-studio.exe` 窗口，确认合成视图选择器为“俯仰图”，3D 方框呈禁用状态。没有操作或关闭用户进程，也没有改写其工程。

`timeline.rs` 原本使用 `!row.locked && state["observing"] != true` 启用方框，因而所有非合成观察视图都拒绝这一操作。观察相机不改变图层归属，3D 开关也不依赖视口坐标；本次仅移除该开关的观察视图限制，保留锁定、目标类型和命令路由。没有放开其他属性字段、视口拖动、蒙版或观察模式中的关键帧写入，也不在点击时自动切回合成视图。

这是一项功能修复，不是添加日志。正常的禁用点击不是错误，不增加警告、重试、状态重置或错误吞噬。实际编辑仍走已有 Action → 异步 engine 信道，沿用 Info 开始记录及失败返回；捕获或恢复失败保留原始原因。

## 可复核证据

证据根目录：`E:/Dev/aem/artifacts/property-selection-context/`。

| 项目 | 证据和处置 |
| --- | --- |
| UI-3D-01 | `3d-user-preview-window.png`、`3d-user-preview-detail.png`：用户正在运行的预览版窗口，俯仰观察视图下的禁用状态。 |
| UI-3D-02 | `3d-observation-before-fix.log`：使用真实 `Action::ViewKind` 和 engine 回复进入观察视图；首个自由视图、主窗、360 宽点击返回空动作，原始断言失败已保留。 |
| UI-3D-03 | `3d-observation-after-fix.log`：2 项定向测试通过。覆盖三种观察视图、双语、主窗/浮窗绘制路由、360/1280 宽、开关双向、实际异步编辑回复、保存重开、撤销重做、锁定保护及视角保留；另覆盖完整工作区/单时间轴、普通/曲线模式、滚动后切换焦点。 |
| UI-3D-04 | 原有两个测试的观察模式只读夹具改为真实 engine 锁定夹具：保留“无编辑动作”和“工程不变”断言，新增确认撤销锁定恢复原工程。此调整对应上面的明确功能边界；不再把观察相机等同于该开关的只读权限。其他观察只读断言保留。 |
| UI-3D-05 | 新增原生捕获场景 `timeline-3d-observation-off/on`。从真实绘制区域点击，只接受唯一、指定图层的 `SetLayer3d(true)`，执行实际 engine 编辑后再截图；校验 `observing=true`、`observationView=top`。捕获成功或失败都分别恢复编辑历史和观察相机；原有截图场景的空动作断言不变。构建、完整原生运行及目视验收待验证。 |

隔离预览程序的额外探针脚本只操作独立临时工程。早期脚本夹具出现 MCP 初始化通知缺失、Win32 句柄参数类型错误和不完整工作区回退；这些不是产品缺陷证据。修正后普通合成视图可切换成功。最终观察视图复现以上述真实 engine 回归测试为准，不能把合成视图成功记为用户问题已解决。

完整 FFmpeg 串行桌面回归通过：`typed-and-3d-full-tests.log`，501 passed、0 failed、1 项原有忽略，220.46 秒。工具回归 `typed-tools-tests.log` 为 54 项、1 项原有跳过，30.328 秒。新增文件暂存后来源检查通过（1356 tracked paths），样式消费者及暂存差异检查通过。优化构建和新增原生捕获仍待验证。

## DESIGN §0.7

| 基线项 | 处置 |
| --- | --- |
| Path 六处纵向步长 | retained：本项未修改。 |
| Path 两列分割 | retained：未修改尺寸。 |
| Path 混合按钮高度 | retained：未修改命中容差。 |
| Path 数值背景 | retained：未修改绘制。 |
| Path 全宽布尔按钮 | retained：未修改控件。 |
| Effect 行高/步进 | retained：本项未修改。 |
| Effect 布尔按钮 | retained：本项未修改。 |
| Effect 枚举按钮 | retained：本项未修改。 |
| Effect hover alpha | retained：本项未修改。 |
| Effect 层级 | retained：本项未修改。 |
| Timeline 固定列坐标 | retained：沿用共享列计划和完整 icon.slot 命中区域。 |
| Timeline 吸附线颜色 | retained：未修改。 |

最终优化构建及完整原生验收通过：`preview-log-release-build-final.log`、`preview-log-native-acceptance-final.log`。双语及覆盖样式的 `release/native-log-fixes-final/property-*.timeline-3d-observation-off/on.png` 已生成；目视检查中文默认样式两图，实际点击后第二层的 3D 从空框变为蓝色勾选，视角保留由捕获流程检查。UI-3D-05 的构建/原生/目视待验证状态由此完成。

无新增依赖、资产、颜色、尺寸 token 或右键菜单；这里的验收仅覆盖所述功能边界。
