# #268 蒙版边界拖柄与 XY 联动验收

实现源 `9b7fec95c7c96333d9e013e073b79a4c408d99a0`，基于主分支 `171fc7a7`。本记录取代阶段文档中的 pending 项；阶段原始日志和失败记录保留。未领取其他 agent 的 issue，也未更改它们的分支。

## 行为与数据

选中蒙版后，查看器显示扩展圆环和羽化采样范围方框，沿对应源坐标轴拖动即可修改原属性。圆环与路径切线菱形保持不同语义；拖柄避开可见节点和切线，钢笔专用模式及框选修饰键优先。凹路径、负扩展和空截面使用实际填充/描边三角网格的截面，空截面保留恢复扩展的入口，不用外接矩形冒充轮廓。

羽化是渐变，没有唯一硬边。本实现显示原渲染器在源空间 1 倍密度下的离散核采样范围，不包含采样器 footprint，不宣称是某一 alpha 等值线；查看器缩放只投影这段源距离。扩展继续复用原填充/圆角描边构造，未改像素语义。沿局部轴映射拖动，父级旋转不会把横向移动误当成另一维。

每个蒙版的 `feather_linked` 保存真实编辑约束。属性和时间轴在原羽化行内共享 Checkbox。开启联动不重采样现有动画；编辑时非零基值保留原比例，零基值使用共同增量，并在写入两轴前共同限幅。项目格式 31 为当前唯一接受版本，保留版本字段，没有迁移兼容层。

拖柄复用原数值 Parameter、Command 与 History 事务。释放使用 release 事件坐标；一次连续拖动只产生一轮历史。Escape、失焦、时间/工程/合成变化、锁定、蒙版消失、路径或投影变化等恢复完整轨道。丢失按钮或新按下会结束原手势，之后光标移动不能继续修改旧节点。

## 需求与证据

| #268 要求 | 证据 |
| --- | --- |
| 可见羽化/扩展拖柄、数值联动 | 生产 Composition 路由 + 实际 worker；真实 Present 后捕获图像，断言蒙版蓝色覆盖像素变化，不能只看覆盖层差异。 |
| XY 开关及真实状态 | 原生联动开/关 24 PNG；实际模型只改变约束字段，保存重开和一次撤销一致。 |
| 复用动画/数值手势 | 同一 Parameter/MaskAction；global 33 / local 13 时编辑，邻接关键帧不变，真实保存重开及整轨撤销。 |
| 缩放、反相、父变换 | 数值测试 0.5/1/2 倍与旋转；GPU 主窗 1.25 / 浮窗 1.75 倍、父级 12°、反相开关。渲染器截面按实际 alpha 对照正/负扩展及凹路径。 |
| 完整取消与一轮历史 | 16 类失效/结束状态、合并释放、空截面恢复；原生两 owner 的释放/失焦及一次撤销。 |
| 原生浮窗与主窗、双语一致 | 12 组 GPU 场景 × 两属性 × 前/悬停/后 = 72 PNG；另有 24 组真实 WindowId / WindowEvent 路由，核对输入不泄漏到另一 owner。 |

所有最终检查的准确命令、耗时、退出码和 test result 见 [结果摘要](mask-boundary-viewer-results.json)。过滤组有重叠，不将其相加。冻结脚本每阶段前后核对 HEAD 和 desktop/crates/DESIGN 内容；仅刷新本工作树 crate 根文件修改时间，避免共享 target 复用另一工作树旧产物，不更改字节或删除他人缓存。

| 最终检查 | 结果 |
| --- | --- |
| path_controls 完整过滤组 | 93 通过、3 原生项忽略后单独执行 |
| 查看器 GPU + 独立 owner 显式原生组 | 2 通过，72 PNG、24 owner 案例 |
| 联动显式原生组 | 1 通过，24 PNG |
| 渲染器边界几何与真实 alpha 对照 | 3 通过 |
| 既有渲染器 layer_masks | 3 通过 |
| mask_plan 剩余预算 | 1 通过 |
| core layer_masks 持久化与历史 | 4 通过 |
| 样式消费者 / diff check | 通过 |

模型完整测试及 doctest 的早期结果仍归明确的 `d47cd6e8` 阶段，不冒称本轮重跑全仓测试。Windows 原生组创建真实隐藏主/浮表面；独立 owner 组把明确 WindowEvent 传入生产路由，不等于物理 OS 鼠标注入或改变系统 DPI。未生成独立发布 exe，未宣称其他操作系统的原生交互验收。

实际截图：[主窗羽化悬停](mask-boundary-viewer/zh-main-1x-feather-hover.png)、[主窗羽化修改后](mask-boundary-viewer/zh-main-1x-feather-after.png)、[独立浮窗扩展](mask-boundary-viewer/zh-float-1x-expansion-hover.png)、[反相与 1.5 倍绘制](mask-boundary-viewer/zh-main-1.5x-feather-hover.png)。从 GPU 原图直接复制，未重绘、改字或拼接；原图位置与逐项 SHA-256 见结果摘要。完整原图、源码、脚本、日志和 CRC 验证在 `data/mask-boundary-viewer-evidence.zip`，包 SHA-256 和大小同样记录在摘要。

## 错误、退化与历史记录

| ID | 分类 / 实际证据 / 边界 |
| --- | --- |
| M01 模型命令 | 功能/协议边界：原信道传播锁定、缺失 ID 和无效值，拒绝后快照不变；既有 worker in-flight 与非 Present Info start 保留。未加新 worker、超时、重试或清错。 |
| M02 取消与失效 | 正常分支：完整轨道回滚有实际 worker / owner 验证；取消、空截面、禁用蒙版和空选择不当故障。 |
| M03 测量几何 | 新观测 `mask.boundary_geometry`：实际非法归一化坐标、实际采样越界进入消费者失败分支。原始原因最多 512 字符，单 Viewer 连续计数，有成功/正常缺失/取消重置；首次 Debug、60 次 Error、后续抑制，不将次数等同秒数。 |
| M04 默认输出 | 子进程移除 MOTION_LOG：file 恰有两条阈值 Error、stderr 同两条，stdout 无站点日志；Info 探针仅写文件并显式刷盘。保存真实文件/stderr/stdout；59/60 阈值、120 次抑制和两类恢复均保留断言。 |
| M05 缓存/热路径 | 相同 nodes/expansion/size/density 的几何及错误结果均缓存；没有每帧 IO/警告或 Present start/done。合法空区间为 Ok(empty)。顶点及坐标限制保留 Err，不把拒绝转成假成功。 |
| M06 原有路径回归 | 首轮实际 5 项失败：拖柄抢占旧节点、钢笔和框选；修正生产优先级后原断言保留。后续两项分别是新旧标记不区分及测试未刷盘；圆环/方框/菱形语义区分、显式测试刷盘后通过，未改变生产缓冲策略。 |
| M07 截图验收缺口 | 第一轮标准化截图只读取了尚未呈现的缓存，虽然覆盖层差异检查通过，不能证明蒙版画面。保留原图/日志的历史分类；最终增加真实 Present、可见内容像素数和覆盖变化断言。 |
| M08 既有观测 | 原字体/绘制/worker 拒绝、panic hook 与非 Present 阻塞覆盖保留；本项没有替换这些边界，不把旧覆盖冒称为新站点证据。 |
| M09 环境限制 | 本机原生 Vulkan Loader 的 Error 来自残留 Epic EOS Overlay JSON 路径；原始日志保留，这一环境问题未修复。不清空控制台、不降低日志级别来使验收通过。 |

## DESIGN §0.7 / §12

| 基线行 | 处置 / 理由 |
| --- | --- |
| Path 六处纵向步进 | retained：本轮不改旧步进。 |
| Path 两列拆分 | retained：不改旧参数列宽；联动开关仍在同一行。 |
| Path 混用按钮高度 | retained：未改旧按钮。 |
| Path 数值背景 | retained：继续复用原数值编辑绘制。 |
| Path 布尔宽按钮 | retained：旧实例未改，新增联动使用 Checkbox。 |
| Effect 行高与步进 | retained：不改效果面板。 |
| Effect fx/— 布尔按钮 | retained：不改效果面板。 |
| Effect 枚举宽按钮 | retained：不改效果枚举。 |
| Effect hover alpha | retained：不改效果状态。 |
| Effect 层级缩进 | retained：不改效果树。 |
| Timeline 列坐标 | retained：复用旧行，不重排其余列。 |
| Timeline snap guide 色值 | retained：不改时间轴吸附。 |

新增标记、线、间距、角半径和命中范围使用现有语义 token，没有新资产、依赖或复制资源；未缩小旧命中容差。§12 本功能的双语、绘制比例、主/浮窗、hover、释放/失焦、取消、动画与 Undo 有以上证据，不宣称全仓视觉整改已完成。

最终证据包：142 项、10,644,967 字节，SHA-256 6d5b67c69f226f606769b05b0ddca9042019a1424d1494812f4a1ca83e708cc9。已逐项重读并验证 ZIP CRC。
