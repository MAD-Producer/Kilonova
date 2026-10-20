# 摄影机镜头设置（#247）

## 范围与协调

分支 `feat/camera-lens-settings` 基于图层导航 #371，继续 #362 / #357 的堆叠关系。开始前检查开放 PR 文件列表及各工作树的目标路径改动。#349 仅在摄影机既有轨道循环增加已修改属性筛选，本批保留其接入位置；未修改其他工作树的偏好设置、采样列或路径实现。来源、NOTICE 和署名规则已核对，没有新增依赖、外部资产、兼容名称或复制产品图标。

## 实现与持久化

新增 `camera_lens` 注册命令，同时设置水平胶片宽度（mm）与已有 FOV 通道；焦距（mm）和像素缩放都是同一投影的派生编辑方式。设合成尺寸为 W×H、垂直视角为 θ，`zoom = H / (2 tan(θ/2))`，`focal = zoom × film_width / W`。胶片宽度范围 0.1–1000mm，现有渲染视角范围仍为 10–120°。可用预设由当前胶片和合成宽高动态判断，非法预设禁用。改变胶片宽度保留当前焦距，焦距/像素缩放编辑保留胶片宽度。

FOV 仍是唯一动画轨道，编辑派生数值会选择该轨道，不引入第二套摄影机或动画状态。沿用现有局部图层时钟、锁定检查、命令注册、错误回复、历史和原子事务；旧版主摄影机与图层摄影机均支持。新增单位标签明确 FOV 是垂直角度。

默认 36mm 胶片字段不写入 JSON，旧项目读取时默认恢复，默认胶片的编辑不提升格式。非默认胶片要求格式 22，命令自动提升版本，保存、重开、打包导出/导入均保留。三个既有“未知未来版本”夹具增加对合法 22 的接受，保留对新未知 23 的拒绝；这是格式契约更新，未删除拒绝断言或降低错误标准。

初验暴露了编辑器 f32 焦距舍入对合法端点的拒绝。换算仅在结果与合法端点具有相同 f32 投影表示时收敛到端点，其余计算保留 f64 精度，显著越界仍拒绝。循环 100 次换算、横屏/竖屏/方形、多种胶片尺寸及实际命令端点均有回归。

当前模型/渲染器没有景深、光圈、焦点距离或散景成像能力。界面显示禁用的景深 Checkbox 和说明，不发送假命令。三种数值编辑使用共享 numeric 手势合同，预设使用透明的原生下拉框；与模式菜单互斥，菜单外点击不会穿透开启另一菜单。所有行尺寸和颜色取自既有 semantic tokens。

## 验证证据

本机证据目录 `artifacts/camera-lens-settings/` 与 `artifacts/pr-queue/camera-lens-*.log`：

- 完整 `motion-core`：258 通过、0 失败、1 项既有跳过，53 组含 doc-test。覆盖实际命令、非法/锁定目标、整批原子性、旧摄影机、偏移局部时间、不同尺寸子合成、撤销/重做、保存重开及打包导出。只改胶片表示、保持 FOV 不变时，真实 Scene 投影矩阵逐值一致。
- 完整桌面：646 通过、0 失败、1 项既有音频跳过，369.70 秒。随后局部增加模式/预设菜单互斥和 FOV 单位标签，最终相关五项全部通过（18.61 秒）；未声称该后续改动再次运行完整桌面集合。
- 最终原生 GPU 验收实际 Present 完成后再 capture_ui，中英参数、预设展开/关闭及键盘选择均有图片。预设选择真实生成一个命令、修改同一 FOV 且一次撤销还原。第一批只读取尚未绘制的 composition scratch，背景黑；已修正截图夹具，最终图像包含实际合成内容，不把旧图当渲染通过。
- 真实 Windows 主窗（英文）和原生浮窗（中文）通过前台焦点、物理数值拖动、一个历史步骤和 Ctrl+Z 还原；两例均正常关闭，`os-focus/summary.json` 及 selected/edited/undone 图片保存。MCP 仅建立并保存夹具和读状态，不代替鼠标编辑。初次驱动未等 cursor-move 事件送达，操作落入旧的时间轴目标；后续显式等待移动送达。第一次正常编辑后关闭又遇到未保存夹具提示，补保存夹具后完成两例。保留失败日志，未将这些驱动失败计作产品成功。
- `cargo build -p motion-desktop --features ffmpeg --locked` 最终成功（38.04 秒），21 项既有警告；最终界面相关测试编译为 19 项既有警告，无新增 warning。来源检查、样式消费者检查、`git diff --check` 通过。

## 错误与降级审计

| ID | 分类 | 处理和证据 |
|---|---|---|
| CAM-LENS-CMD | 既有错误边界 + 新命令 | 非法胶片在真实 engine edit 失败，保留原原因和请求记录；默认 Info 文件日志与 Error stderr 均有原因，stdout 无日志。失败状态及历史不变。 |
| CAM-LENS-SNAPSHOT | 新观测 | 缺失/类型错误的必需数据不能伪装为默认镜头。仅旧项目省略胶片字段是正常默认分支。复用既有 fallback Counter，每个控件实例连续计数，正常快照或目标上下文切换重置；记录有界 object/原始原因。沿用基础设施首个 Warn、60 次 Error 的既有策略，120 次只发两条，成功后新一轮再次发两条。真实渲染调用及子进程检查文件/stderr/stdout；60 次不表示一秒。 |
| CAM-LENS-CANCEL | 正常分支 | Escape、失焦、目标切换/锁定或菜单关闭按既有手势撤销处理，不记录为故障。 |
| CAM-LENS-UNSUPPORTED | 功能边界 | 景深/光圈/焦点距离/散景未实现，明确禁用；不使用重试、后台重启或静默成功替代。 |
| CAM-LENS-WORKER | 既有覆盖 | 本批不新增线程或阻塞操作；非 Present edit 沿用既有开始记录、in-flight、回复和计时，健康 Present 未加日志。 |

## DESIGN §0.7 与 §12

| 基线行 | 状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：本批不改路径面板。 |
| path_controls 两列硬编码 | retained with reason：不改路径几何布局。 |
| path_controls 不等按钮/字段高度 | retained with reason：不改路径动作。 |
| path_controls 自绘数值底色 | retained with reason：新增镜头使用现有 numeric，不改路径控件。 |
| path_controls 布尔宽按钮 | retained with reason：无该路径变更。 |
| effect_controls 行高/步距 | retained with reason：无效果列表变更。 |
| effect_controls 布尔按钮 | retained with reason：新景深为 Checkbox，不改原效果实现。 |
| effect_controls 枚举按钮 | retained with reason：新预设为 Dropdown，不改效果实现。 |
| effect_controls hover alpha | retained with reason：新代码没有自绘 hover 色。 |
| effect_controls 缺少 depth | retained with reason：无效果树结构变更。 |
| timeline 11 列坐标 | retained with reason：保持现有坐标与 hit region。 |
| timeline snap guide 颜色 | retained with reason：保持原提示绘制。 |

§12 中本批相关的单行控件、透明静止数值、共享列/基线、菜单事件优先级、中英和主窗/浮窗焦点、取消与撤销由上述实际交互验证。没有改变 19 个面板的注册、五个布局预设、时间轴列、吸附、Reduced Motion 或其他手势。格式 22 是 #247 持久化功能需求，不是视觉重排附带引入的状态系统。没有声称整个 DESIGN 已整改完成；其他面板基线、最小/默认主窗尺寸的逐面板截图、非本机 DPI 与跨平台原生输入仍未复验。完整远程 CI、最终 PR 合并及 issue 关闭继续待办。

## 父分支与原生 CI 集成复验（2026-10-20）

同步 #371 的 f13dd68，包含 #362 / #357 / #368 以及已合并 #364 的 Intel 验收预算。预演与实际合并树均为 d008102b7ca4706e63ab3aafd52b765302939938；仅九个上游文件变化，没有改镜头算法、数值控件、预设、样式 token 或项目格式。上面的完整回归、FFmpeg 构建和物理输入结果属于原功能版本。

本次实际重建复验：核心镜头 7 通过（0.05 秒）、面板镜头 5 通过（31.68 秒）、采样播放 1 通过（33.70 秒）、中英文 PNG 导出 1 通过（17.41 秒）；工作流 5 通过（5.801 秒），来源 1553 tracked paths、面板样式消费、差异检查通过。定向日志为 artifacts/pr-queue/camera-lens-native-ci-*.log；截图与采样/PNG 结果在 artifacts/camera-lens-settings/ci-integration/。九张原生 PNG 全部解码且具有非均匀像素，目视核对中文参数和英文预设；不扩大为全项 DESIGN 验收。本次没有重新运行全套、FFmpeg 发布构建、OS 物理输入或高 DPI 验收。

首次桌面编译读到共享 target 的旧 core/model 产物，lens 导出及 CameraLens 缺失；失败日志保留 feature.log。核对并解析本次 target 后，仅用 cargo clean -p 清理 core/model 缓存，重建恢复，不修改源代码、断言或等待期限；其他线程在不同 target 的测试和发布构建未被终止。首次来源检查误用不存在的工具路径，改用仓库实际 validate_provenance.py 后通过；不把错误工具调用计为产品失败或成功验收。

错误与降级仍沿用上述 CAM-LENS 条目：这次重跑实际失败日志、阈值/恢复与 stdout 隔离，未新增生产观测点或改变正常取消/未实现景深边界。父链仍须依次合并，最终提交远程 CI、其他平台人工输入和后续父链集成继续待验。

常规 Git 传输连接失败后，本次通过 Git 数据接口发布。仅将两个未发布提交的日期时区表示统一为 UTC，并同步本段集成提交引用；作者、时刻、中文消息、父关系与已验证的产品文件内容保持一致。发布前逐项比较树和对象哈希，远程仅快进，不改已发布历史。网络失败和发布对象核对记录位于 artifacts/pr-queue/。
