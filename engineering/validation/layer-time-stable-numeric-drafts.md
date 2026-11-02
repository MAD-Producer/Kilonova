# #257：数值草稿绑定连续空间键 ID

接续 `5cdea70d`。开工核对开放 PR 与近期本地分支，跟踪和预渲染工作与本轮没有重叠，没有改动这些分支。

Roving 几何改值会重新分配键时间。第一次更新后继续按播放头写入，会在原时间新增固定键。
本轮为 `TrackEdit` 增加 `SetSpatialKey {key,value}`，命令目录公开该协议，模型只接受显式耦合空间属性与正稳定 ID，沿用完整工程事务及所有值、轨道与项目校验。

属性栏、时间轴共用数值字段及摄影机字段在开始编辑时捕获原 ID。后续数值更新直接修改该 ID，未编辑分量取自该键的作者值，而不是已经随 Roving 重算的播放头采样值。键消失或轨道转换时取消，不重新认领邻近键。无原键的新增操作沿用固定时间插入；锚点伴随补偿和合成画布拖动不在本轮交付范围。

捕获时仅解析目标轨道并准备一次空间时序；预览阶段通过有界键数组查找 ID，不重新构造几何。未增加持久格式、依赖、素材、图标或布局。

## 验证状态

最终源码验证完成：67 项通过，2 项既有手工原生测试未运行；带 FFmpeg 的 workspace all-targets check、样式消费者、改动模块 rustfmt 与 git diff --check 通过。此结果只覆盖本阶段，不据此宣称 #257 完成或整条分支已完成最终集成验收。

分组为 TrackAt 17、空间作者与实际 worker 14、属性 9、锚点 2、画布锚点 2、摄影机 7、空间时间模型 8、空间轨道模型 8。最终程序 SHA-256、逐文件源码哈希、全部原始日志及此前失败见[证据清单](layer-time-stable-numeric-drafts-evidence/manifest.json)和[原始证据包](layer-time-stable-numeric-drafts-evidence/evidence.zip)。

- 新增测试覆盖普通／正放／倒放、固定／Roving、摄影机及全局摄影机、未编辑分量、保存重开与完整工程撤销，非法 ID／值／通道以及重合时刻拒绝。
- 实际 worker 用例连续三次预览并断言始终写 ID 2；取消与迟到松开、一次撤销；摄影机复用主窗／浮窗 chrome 并覆盖中英文及两个方向。
- 默认等级故障用例覆盖属性数值和摄影机的开始／继续／命令构造站点，检查文件、Error stderr、stdout 隔离、首次 Warn／累计第 60 次 Error及恢复。面板观察器的计数是进程生命周期累计，成功不重置；正常删除键及取消不作为故障。
- 初次类型检查暴露新增 JSON 往返测试直接比较未实现 PartialEq 的 Command，已改为比较完整序列化值；没有修改既有测试断言。
- 初次构建中 rustc 以 STATUS_ACCESS_VIOLATION 退出，日志保存为 compiler-access-violation.log。同源码重新构建，不能把该失败算成通过。磁盘空间偏低，但没有证据证明它导致编译器退出。
- 自动审批拒绝清理当前未使用的旧增量缓存，原因是 blocked by policy，目录保持原样。

首轮新增组实际为 9 通过、5 失败（子进程的结果不得冒充父组总数）。既有数值 worker 回归指出 SetSpatialKey 被 composition_transaction 判为结构性事务，正在编辑手势中被拒绝。已将它与 Set 一同归为值预览，并新增分类断言；未修改既有回归断言。首轮日志和程序／源码哈希保存在 before-gesture-classification-fix。最终重建后新增组 14 项全部通过。

摄影机故障用例还发现首次失败时 Begin 尚在本地缓冲，不能取消尚未开始的 worker 手势。命令构造失败现在丢弃该缓冲；已有手势仍发送取消。该失败及源码／程序哈希保存在 before-camera-pending-begin-fix。最终重建后摄影机 7 项通过、1 项既有手工原生测试未运行；故障用例验证三处观察站点及恢复后的一次撤销。

## 剩余边界

本轮完成后仍需验收旧 mini 编辑器、补偿和画布手势、真实媒体设备与高倍速音频以及最终集成。无新原生截图／独立进程重开证据时，不能把 chrome 用例当作原生平台视觉验收。

## DESIGN §0.7 处置

仅修改共享命令适配器，未改变属性行、控件、颜色、尺寸、图标或命中范围。
下列行均按原位置保留，不声称它们已整改。

| 基线位置 | 状态与原因 |
|---|---|
| path_controls.rs 六处行步进 | retained with reason：未改路径行步进。 |
| path_controls.rs 双列分割 | retained with reason：未改路径列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：未改按钮及命中范围。 |
| path_controls.rs 自绘数字背景 | retained with reason：未改数字绘制。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：未改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：未改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：未改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：未改效果树。 |
| timeline.rs 十一列坐标 | retained with reason：保留既有列坐标。 |
| timeline.rs 吸附线颜色 | retained with reason：未改吸附绘制。 |

§5.2 没有新增属性行或缩小命中区域；§12 的全平台、DPI、布局与视觉基线仍需
独立验收。旧 mini 编辑器不在本阶段完成范围，不能把精确数值输入当作它已支持
连续空间键的证据。
