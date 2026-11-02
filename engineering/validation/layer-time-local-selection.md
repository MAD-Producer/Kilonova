# #257：桌面选择集保留本地关键帧身份

基于 `a5458993`，工作分支 `feat/layer-time-mapping`。开始前再次检查打开 PR，
仍只有另一 agent 的 #575（`feat/82-native-prerender-runtime`），没有审查或修改它。
音频首版按用户确认采用音高随速度变化。未新增依赖、资产、工程版本和效果包变更。

## 本阶段边界

扩展现有 `AddressedKey`、`Channel`、`Set`，增加类型化通道和有符号本地整数地址。
没有增加第二套选择/历史状态，也没有在旧 `u32` 字段编码标记或将显示时间取整。
例如本地 3 对应合成 6.5，选择仍保存 3；负向保留键和裁切外的键也保持其身份。
作者轨道按完整有序 i32 键集合检查；正常缺失返回 stale，不算故障。来源、代次、
revision 缓存、锁定和只读检查沿用现有选择系统。主选择与菜单缓存包含本地身份。
同一通道混入两套帧坐标会在修改旧选区前拒绝，字节预算计入 i32 的符号位。

删除和快速缓动计划接到 `LocalKeyframes`，与未映射通道经同一 `EditorAction`
共同提交和撤销。UI 入/出侧按合成时间定义，倒放时交换一次传给存储协议。
主选择只投影对象和轨道，不伪造一个旧整数关键帧。旧整数剪贴板不能完整表示本地
或空间身份，保持选择所有权并禁用整个混合操作，避免图层回退和部分成功。

**这仍是内部接线阶段，不是可见功能交付。** 现有绘制入口尚不生成本地地址；
整数拖动/微调不能执行本地选区，显式禁用。时钟感知的拖动/吸附、真实输入、
变速/倒放/重映射入口、剪贴板扩展、插值和自动类型汇总、Roving 映射、各媒体余项
与截图仍待完成，#257 保持打开。新增本地 mark 分支只维持类型穷尽和共用 token
绘制边界，没有改已有面板的样式、行数、颜色、尺寸、命中区域或菜单位置。

## 验证记录

| 最终执行 | passed | ignored（不计通过） |
|---|---:|---:|
| 现有 key_selection 默认测试集（含新增 7 项） | 48 | 4 |
| animation_menu | 29 | 3 |
| timeline_shortcuts | 55 | 1 |
| keyframe_clipboard | 6 | 0 |
| curve_comparison | 15 | 0 |
| serial worker 的关键帧错误/恢复边界 | 2 | 0 |
| 按测试身份去重合计 | 155 | 8 |

新增 7 项不是重复加在上述总数上。被忽略的 8 项为既有手动原生截图测试，本阶段
没有执行它们，不能计作截图验收。最终默认测试程序的 SHA-256 在扩展执行前后相同。
工作区 all-targets 检查通过（FFmpeg、host diagnostics，排除退役 Android），用时
2 分 31 秒；保留既有 unused/dead-code 警告。编译检查不计入执行测试数。
样式消费者检查、三个新增/主要修改模块的 rustfmt check、git diff check 均通过。

首轮编译失败来自新增测试辅助代码对不实现 Debug 的 `EaseUnavailable` 调用
`unwrap`；改为测试内明确失败消息，没有改变生产错误类型。扩展执行第一次忘记
继承已有便携构建的 TEMP/TMP，11 项在系统临时目录返回 PermissionDenied。
同一程序只修正启动环境后全部通过；没有修改生产代码或既有断言、预算和期限。
原始失败、首轮 7 项、最终所有测试和检查日志保留，首轮结果不再次累计。

新增用例覆盖：分数投影/负帧/裁切、去重切换、来源变化/缺失/锁定/只读、坐标类型
混用原子拒绝及符号预算、正放/倒放缓动和混合未映射通道、混合删除/保存重开/
整次撤销、主选择/剪贴板/弹出菜单来源防回退，以及默认错误输出的真实子进程。
快捷键与菜单计划提交真实 core 引擎事务；它们不是原生鼠标输入或截图验收。

默认级别的真实子进程取消 `MOTION_LOG` 覆盖：先执行健康/正常缺失各 120 次，再
交错执行损坏和健康轨道，时钟损坏另执行 120 次。两个固定站点各只保留第 1 次
Warn 和第 60 次 Error；文件有对象、phase、原始原因，stderr 只有相应 Error，
stdout 没有 panel 诊断。健康检查按本功能诊断筛选，不误认日志初始化 Info。
随后两项既有原生 worker 用例实际重跑，保留默认 Info start、三种本地拒绝的
文件/stderr、stdout 隔离、完整状态不变及成功恢复/撤销的原始输出。

确切命令、完整测试身份、程序/源文件 SHA-256 和日志清单见
[证据清单](layer-time-local-selection-evidence/manifest.json)，原始输出见
[归档](layer-time-local-selection-evidence/evidence.zip)。没有 GitHub CI 或其他平台
执行结果；没有把本阶段内部状态测试当成用户可见入口完成。

## 错误处理审计

| ID | 类别 | 证据与边界 |
|---|---|---|
| TIME-S-01 | 功能/协议 | 本地地址加入既有选择集，删除和缓动转换为既有原子动作。不是单纯增加日志。 |
| TIME-S-02 | 新观察 | 本地作者轨道或时钟格式损坏通过既有 panel observer 保留对象、phase、原始 cause。计数为固定调用点的进程生命周期累计：第 1 次 Warn、第 60 次 Error，此后抑制；正常相邻轨道不清零。这是复用既有 counter 的范围，不等同 60 帧或一秒。 |
| TIME-S-03 | 正常分支 | 缺失键、裁切外保留键、旧剪贴板/整数手势不支持的坐标、锁定、只读、过期来源属于 stale/禁用能力；不清错，不重试，不改成成功。 |
| TIME-S-04 | 既有覆盖重跑 | 编辑仍走 `EditorAction` 的 serial worker，在途和 Info start/Error 文件+stderr 保持原协议；本阶段重跑两项既有真实拒绝/恢复测试，原实现与先前边界见 `layer-time-local-keys.md`，不是新增在途或受控阻塞实现。 |
| TIME-S-05 | 待验证 | 时钟感知鼠标手势、主窗/浮窗、中英文完整入口、阻塞输入和截图，不能用本阶段内部计划测试替代。 |

## DESIGN §0.7 逐项处置

以下保留项均因本阶段修改选择和动作计划，没有整改相应绘制或行布局。位置引用
DESIGN 的基线，不把当前行号偏移当成修复证据。§12 不声称整体符合。

| 基线位置 | 内容 | 处置 |
|---|---|---|
| path_controls.rs:860,884,953,1002,1070,1149 | 六处行步进 | retained with reason：不修改该布局。 |
| path_controls.rs:893 | 双列宽度 | retained with reason：不修改该布局。 |
| path_controls.rs:894–953,1038 | 按钮尺寸 | retained with reason：不修改该控件。 |
| path_controls.rs:1124,1130 | 数字输入绘制 | retained with reason：不修改该控件。 |
| path_controls.rs:1151–1170 | 布尔按钮 | retained with reason：不修改该控件。 |
| effect_controls.rs:958,1043,1050 | 行高 | retained with reason：不修改该布局。 |
| effect_controls.rs:998–1013 | 布尔按钮 | retained with reason：不修改该控件。 |
| effect_controls.rs:1053–1064 | 枚举按钮 | retained with reason：不修改该控件。 |
| effect_controls.rs:974 | 悬停颜色 | retained with reason：不修改该绘制。 |
| effect_controls.rs 全文件 | 树层级 | retained with reason：不修改该层级。 |
| timeline.rs:541–547 | 列坐标 | retained with reason：不修改该布局。 |
| timeline.rs:1432–1436 | 吸附线 | retained with reason：不修改该绘制。 |
