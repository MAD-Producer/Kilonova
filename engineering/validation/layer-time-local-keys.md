# #257：分数投影关键帧的本地身份编辑

本阶段基于 `da0b3ffb`，在 `feat/layer-time-mapping` 实现编辑协议。
开始前已核对其他分支：打开的 #575 由另一 agent 处理原生预渲染，本阶段不修改其
实现或 PR。音频策略按用户本次确认，采用音高随速度变化；已更新实现契约中的
待答复说明。没有更改依赖、第三方资产、效果包、工程版本或历史渲染基线。

## 功能与协议边界

旧 `u32` 合成帧命令无法寻址变速后的第 6.5 帧，新增复现用例在旧实现实际失败，
原因是未注册 `local_keyframes`。新命令以已发布的 `TimelineKey.local_frame`
加类型化通道作为地址，源身份和时间平移量均为有符号本地整数；目标通过同一时钟
投影后验证合成边界。没有把合成时间取整，没有移除、重置或临时绕过图层时间映射。

支持移动、复制、单值分量偏移、插值、区间曲线、快速缓动、自动 Bezier 和删除。
复用原有通道访问、碰撞计划、区间缓动与自动策略算法。先克隆并求解候选通道，再
安装；完整工程域检查和多通道批量回滚沿用 core 事务。占用未选关键帧、重复/乱序
选择、锁定、缺失、溢出、不支持的能力和越界均拒绝。合法的负向保留键可编辑元数据，
删除最后一键保持其静态值。已有整数合成帧命令的输入与取整规则未改。

缓动侧按递增本地时间定义，倒放时不能直接当作可见的左/右侧。每条命令仅一个
通道；多个通道用原子命令数组。该协议本身不新增并发 revision 语义，桌面交互
接入仍须使用既有来源/revision 校验。连续空间关键帧使用其已有稳定 ID 协议，
本命令显式拒绝混用，不声称解决该协议在新时钟下的映射。

已定位后续桌面接入点：`panels/timeline_key_drag.rs` 当前用 `as_i64` 收集投影帧，
`panels/graph_keys.rs` 用 `as_u64` 查找拖动源键，
`panels/keyframe_clipboard.rs` 仍生成旧 `KeyframeSelection`。
这些入口会遗漏分数投影，本阶段未修改它们；后续要同时传递本地身份与连续显示
坐标，保留来源校验、吸附、释放、撤销和宽容的命中区域。

## 验证记录

首次复现：1 项实际失败（旧实现不认识新命令）；实现后同一预期通过。
扩展测试初次出现测试辅助代码变量遮蔽，之后的三项失败分别是直接解析时忽略
既有合成包装、音频 fixture 的 in 早于 offset、手工插入效果后漏建插件依赖。
修正这些 fixture，使其遵守既有工程约束；没有放宽生产验证或修改既有断言。
原始失败日志保留在证据归档中。

新增用例覆盖 3→5 的本地移动与 6.5→9.5 的投影、相交选择移动、复制数值偏移、
锁定/占用/溢出/越界、失败批量回滚、负帧、倒放缓动归属、自动策略、分离轴、
蒙版几何/羽化、嵌套矢量、效果数值/色彩曲线、真实布尔/枚举参数、电平声道、
显式子合成、保存重开及撤销/重做。连续空间身份拒绝和完整域失败另有负对照。

| 最终执行 | 通过 | ignored（不计通过） |
|---|---:|---:|
| 完整 motion-model | 211 | 0 |
| core 既有关键帧、图层时间、空间 ID、合成与事务回归 | 104 | 0 |
| 新增本地身份编辑用例 | 12 | 0 |
| 原生 serial worker 默认错误信道与恢复 | 1 | 0 |
| 按测试身份去重合计 | 328 | 0 |

原生用例使用实际引擎打开保存的工程，触发占用、缺失和越界三种拒绝；逐次严格比较
完整 project、revision 和 history，之后成功移动同一分数投影键并撤销。子进程取消
`MOTION_LOG` 环境变量，实际文件中每条 Error 都带原请求 ID、edit、对象、通道、
本地操作 phase 和 cause，对应 Info start 在 Error 前；stderr 有同一 cause，
stdout 无诊断泄漏。计数按父测试身份计一次，不重复累计子进程执行。

确切命令、源文件和程序 SHA-256、所有失败与最终执行日志，以及真实文件日志、
stderr/stdout 见[证据清单](layer-time-local-keys-evidence/manifest.json)和
[原始归档](layer-time-local-keys-evidence/evidence.zip)。
全工作区 all-targets 检查（桌面 FFmpeg、宿主 diagnostics，排除退役 Android）
已通过，用时 4 分 10 秒；保留既有 unused/dead-code 警告。编译不计作执行测试，
未声称执行其他平台或 GitHub CI。新增模块和新集成测试的 rustfmt 检查、最终
`git diff --check` 通过。

## 错误与可观测性审计

| ID | 类别 | 证据与边界 |
|---|---|---|
| TIME-K-01 | 功能/协议 | 新增本地寻址和候选通道事务，不是日志修饰；UI 尚未接线。 |
| TIME-K-02 | 复用错误上下文 | `KeyframeTarget.failure` 保留 operation/phase、对象、通道和原始 cause；对象/参数名采用既有长度界限。 |
| TIME-K-03 | 既有 worker 覆盖 | 沿用 serial edit 的 Info start/在途发布和 Error 文件+stderr；真实原生三种拒绝、stdout 隔离、状态不变和随后恢复/撤销已验证。 |
| TIME-K-04 | 正常分支 | 无变更命令、正常空历史和取消语义沿用现有事务；未新增计数器、重试、超时或 Present 日志。 |
| TIME-K-05 | 待完成 | 桌面来源/revision 接线、连续空间时间映射、真实输入与截图验收；本记录不把它们列为修复完成。 |

本地预检错误通过类型化 target 包装；命令之后的完整工程域拒绝保留 core 原有
事务错误及回滚边界。原生用例验证新命令的 start 先于 Error，不把这种快速拒绝
当作新的受控阻塞试验。本阶段未添加阻塞操作或修改在途发布实现，既有 worker
阻塞验收仍属于其原有覆盖。

没有修改 panel。下列是 DESIGN §0.7 的逐项处置，行号引用该文档的基线，不声称
重新定位了当前 UI 代码。所有保留项的共同原因是本阶段只改模型、协议、历史和测试。

| 基线位置 | 内容 | 状态 |
|---|---|---|
| path_controls.rs:860,884,953,1002,1070,1149 | 六处行步进 | retained with reason：不改面板。 |
| path_controls.rs:893 | 双列宽度 | retained with reason：不改面板。 |
| path_controls.rs:894–953,1038 | 按钮尺寸 | retained with reason：不改面板。 |
| path_controls.rs:1124,1130 | 数字输入绘制 | retained with reason：不改面板。 |
| path_controls.rs:1151–1170 | 布尔按钮 | retained with reason：不改面板。 |
| effect_controls.rs:958,1043,1050 | 行高 | retained with reason：不改面板。 |
| effect_controls.rs:998–1013 | 布尔按钮 | retained with reason：不改面板。 |
| effect_controls.rs:1053–1064 | 枚举按钮 | retained with reason：不改面板。 |
| effect_controls.rs:974 | 悬停颜色 | retained with reason：不改面板。 |
| effect_controls.rs 全文件 | 树层级 | retained with reason：不改面板。 |
| timeline.rs:541–547 | 列坐标 | retained with reason：不改面板。 |
| timeline.rs:1432–1436 | 吸附线 | retained with reason：不改面板。 |

§12 的主窗/浮窗、中英文与菜单截图仍待可见入口阶段。#257 保持打开。
