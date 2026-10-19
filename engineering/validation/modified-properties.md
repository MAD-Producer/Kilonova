# UU 已修改属性筛选：实施与验收记录

关联 #213。实现及本地验收如下；合并前保持 issue 开放，由 PR 的最终 CI 继续验证。

## 分支协作

2026-10-19 从主线 `015df56d` 建立 `feat/modified-properties`，独立工作区为 `.worktrees/modified-properties`。检查开放 PR：#344 负责渲染采样，#345 负责文字边界，#346 负责图标几何，后续 #347 负责借用输入；本分支避开这些库与节点控件文件。检查本地分支与提交，没有已有 UU/已修改属性筛选实现。node-gestures 有未提交节点输入和 shell 路由改动，本分支避开对应文件。根工作区的用户未跟踪文件保持原样。

## 判定契约

“已修改”对照新建对象的默认值，与当前播放时间、上次保存时间无关。只要保留关键帧，哪怕关键帧数值等于默认值，仍属于已修改。保留的表达式也属于已修改，禁用表达式不能让它从筛选结果消失。分离轴逐轴判定，不由另一个轴的静态值或关键帧决定本轴的可见性。

| 属性族 | 默认值/规则 | 实施状态 |
| --- | --- | --- |
| 图层锚点 | 源尺寸归一化中心 `[0.5, 0.5]` | 判定代码及定向回归通过 |
| 图层位置 | 当前合成中心，Z=0 | 同上 |
| 缩放、旋转、不透明度 | `[100,100,100]`、`[0,0,0]`、1 | 同上 |
| 摄影机轨道 | 复用 `Camera::new(width,height)` 的默认位置、目标、视场角、半径及角度 | 同上 |
| 效果参数 | 实例保存的实际效果默认值；曲线比较 `CurveObject::default()`；包含关键帧与表达式 | 时间轴与内嵌属性效果共用判定，定向回归通过 |
| 蒙版 | opacity=100、feather=[0,0]、expansion=0；显式路径属于新增内容 | 两处共用逐通道判定；保留模式/反转行作为蒙版上下文 |
| 矢量组 | 组变换模型默认；填充、描边、修剪与中继器复用当前创建入口；形状复用 `VectorContent::shape`，新增组内几何大小=100、位置=0 | 两处使用同一棵筛选树；比较 authored 值，不比较当前帧采样值；新增算子/路径保留标题以便定位 |
| 音频 | 增益=1、未静音、左右声道=0dB；任意关键帧保留 | 既有时间轴音频行按同一 filter 筛选，行计划与实际绘制一致；未在属性面板另造原本不存在的音频编辑器 |

缺少整个不适用通道是正常分支。已有通道的非有限/不可读数值通过现有有界观察机制记录，并保留该行可见，避免错误响应把问题字段藏掉；没有清除错误或更改工程。沿用 panel observation 的固定调用点、进程存续期失败累计计数：第一次 Warn，第 60 次 Error，随后抑制；计数饱和且上下文限 4096 字符，进程重启时重置，穿插正常行不重置，以免列表中坏字段的计数永远被好字段清除。60 次不表示一秒。新增进程隔离回归已检查默认文件/Error stderr/stdout 隔离、阈值与重复抑制。此处沿用既有进程存续期计数，不能宣称成功后重置或新增连续失败计数器。

## 输入与共享状态

第一个 U 立即执行既有动画筛选，不等待第二次按键。350ms 内同一工程、合成、选择、窗口及属性上下文中的第二次新按键切换为已修改筛选；处理完一对后清除序列。操作系统自动重复、IME、文本输入、失焦和中间的其他操作不能拼成 UU。上下文改变时使未完成序列失效。

已接入键盘路由。共享筛选仍使用既有 TimelineState，供时间轴与属性面板消费；摄影机、分离轴、效果、蒙版、矢量均读取同一筛选值。独立效果控制面板及矢量面板保持其完整编辑入口。UU 展开相关已有组；不修改文档格式。按键序列只存放在 Panels，不放进可持久化的工程。

## 验收状态

- 已写真实 Engine 夹具：新层默认、静态编辑与恢复、等于默认值的关键帧、分离轴独立判断、禁用表达式保存重开及移除后的恢复。
- 首轮 `modified-properties-classification.log` 为 1 通过、5 失败：直接创建摄影机的夹具缺少格式 19 声明，尚未进入这些判定断言。仅补齐合法夹具版本，保留所有断言；`modified-properties-classification-final.log` 为 6 通过、0 失败（26.83 秒增量编译，执行 0.07 秒），包含独立子进程日志验证。构建使用独立于 #344 release 的 target，并以单编译任务运行；首次等待该已有 target 的其他任务释放锁，未终止其他分支进程。
- 路由回归覆盖中英文与 Timeline / Properties owner，第一按立即进入 Animated，第二按进入 Modified；有界时间测试不依赖 sleep。失焦、IME、系统重复、滚轮、菜单/文本输入和图层切换打断未完成序列。真实 Engine 的保存重开覆盖禁用表达式，筛选不产生工程命令或历史记录。
- 旧单 U 收起测试现在显式中断上一对按键序列，以表达“稍后独立按 U”；保留原收起断言。新测试单独验证即时 UU，没有删除或放宽原断言。
- 首次完整 FFmpeg 桌面回归 `modified-properties-full.log`：602 通过、2 失败、1 既有忽略。两项失败是效果库双击和图层名称双击的墙钟测试；随后 `modified-properties-panels-serial.log` 的全部 295 项面板测试通过（100.87 秒），含原两项断言。未将初轮失败抹掉，也没有调整双击时限。
- 新错误信道与全属性族定向结果：`modified-properties-all-families-final.log`，13 通过、0 失败（9.42 秒），含矢量默认值/恢复、音频真实布局、全部变换与两项进程隔离错误测试。音频夹具首轮没有设置输入焦点，尚未触发 U；仅补齐 Timeline 焦点，保留全部行数及状态断言。最终面板集成与编译记录继续见下文。
- 最终生产代码（含滚轮中断及无多余路径克隆的默认矢量树）面板集成：`modified-properties-panels-final.log` **297 通过、0 失败**（79.86 秒）。`modified-properties-native-final-build.log` FFmpeg dev 构建成功（22.77 秒），21 条既有警告；新增 private-interface / dead-code 警告已消除。随后加强菜单/搜索中断断言的 `modified-properties-final.log` **13 通过、0 失败**（5.80 秒），未重复累计数量。
- 一次同 target 的测试重链接在前一测试 EXE 仍运行时被 Windows 以 LNK1104 拒绝，见 `modified-properties-final-targeted.log` 的前次版本；未停止其他任务，等待原测试终止后串行重新运行通过。后续同 target 的测试与编译保持顺序执行。
- 原生 FFmpeg dev 构建与 `--property-smoke <report>`：`modified-properties-native-verified.log` / `modified-native-verified/summary.json` 确认中英文各 33 次主窗呈现、两个原生浮窗各 4/2 次呈现，designCaptureProjectUntouched=true。新增 U / UU 各阶段、Timeline / Properties、main / floating 面板入口，共 16 张真实 GPU 截图。截图通过实际 chrome / floating_chrome 输入路由绘制到 attached native surface，不将其描述为操作系统物理键盘、跨屏焦点或多档 DPI 自动化验证。首次脚本遗漏 --property-smoke 的路径参数，程序正常报错退出；记录 `modified-properties-native.log`，没有进入本轮 UI 验收。
- 已目视检查中文 `uu-timeline-main-modified`、`uu-properties-floating-modified`：静态旋转 12° 与带关键帧的默认不透明度均保留；默认位置/缩放/锚点不显示。其余截图与既有窄宽/最小时间轴捕获保留在产物目录。没有新增右键菜单。新的 OS 级跨屏 DPI / IME 实机矩阵、长时间复杂工程性能基准仍未验证。

## 错误与正常边界审计

| 项目 | 分类与证据 | 剩余边界 |
| --- | --- | --- |
| MP-OBS-01 `properties.modified_value` | 新增变换 authored 数值读取观察；坏字段保持可见；子进程实际验证默认文件、60 次 Error stderr、stdout 隔离、后续抑制 | 进程存续期累计，采用上文已说明的既有 counter；不宣称连续失败或成功重置 |
| MP-OBS-02 `properties.modified_effect_value` / `properties.modified_curve` | 新增效果默认/值和曲线解码观察；真实畸形 JSON 的分类路径触发；同样验证 59/60 阈值、交错正常值、抑制及原始原因 | 不等同于从合法工程导入畸形模型，模型校验仍由现有引擎负责 |
| MP-EXIST-01 `audio.level_track` | 既有观察调用点提取为返回 Result 的 helper；绘制保留原 zero fallback，筛选遇到 Err 保留行 | 未增加重试、清错或新日志策略；音频错误信道已有回归随面板集运行 |
| MP-NORMAL-01 | 不适用/缺少通道、默认值、空关键帧、没有选择、IME 与取消是正常分支，不记录故障 | 无逐帧文件 IO 或正常 Present start/done |
| MP-BOUND-01 | 筛选和按键识别为同步本地读取，没有新增 worker、阻塞 IO、协议回复或重启；工程快照与历史不变 | 源工程读取/渲染仍由现有 worker 观测负责；本功能不声称修复它们 |

## DESIGN §0.7 逐项处置

本次改变可见行集合，复用原有行布局与交互；没有新增颜色、尺寸、token 或命中容差，没有新依赖/素材。`validate_provenance.py` 与 `verify_style_consumers()` 通过。以下均是保留既有样式行为的处置，不能算成修复对应遗留缺陷：

| 基线项 | 处置 |
| --- | --- |
| path_controls 的六组 y 步进 | retained：未改步进或行高，筛选仅跳过不匹配行 |
| path_controls 两列分割 | retained：复用现有 property_row/layout |
| path_controls 混合按钮高度 | retained：未改按钮实现 |
| path_controls SUNKEN/ACCENT 数值背景 | retained：未涉及其绘制分支 |
| path_controls 布尔全宽按钮 | retained：未新增或替换该遗留入口 |
| effect_controls 行高/步进 | retained：复用当前 token 与布局，不以本功能顺便重排 |
| effect_controls 布尔 fx/横杠 | retained：筛选不改变控件语义 |
| effect_controls 枚举全宽按钮 | retained：未新增循环按钮或改下拉 |
| effect_controls hover alpha | retained：未改悬停颜色 |
| effect_controls 层级缩进 | retained：保留已有层级布局；无新增缩进数值 |
| timeline 11 列坐标 | retained：兼容基线不变；行范围与 key lane 共用筛选计划 |
| timeline snap guide 颜色 | retained：未改 snap guide |

DESIGN §12：已有行的颜色、字段边缘、缩进与命中区保持；原生 U/UU 截图和既有面板交互回归验证筛选后的行对应关系。没有将本次局部验证描述为全部设计规范合规。

这份记录区分实现、实际失败尝试、观察信道和待验证的 OS 边界；不将纯判定测试或编译成功等同于全平台验收。
