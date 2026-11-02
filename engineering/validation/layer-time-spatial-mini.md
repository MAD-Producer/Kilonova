# #257：小曲线编辑器的连续空间键寻址

接续 `35d4238c`。开工核对开放 PR 和近期本地分支，跟踪与预渲染分支没有占用本轮工作；未修改或审查其他人的 PR。

## 行为与边界

关键帧身份改为独立的组合帧、作者局部整数帧或空间稳定 ID；用于绘图和读数的局部时间保持 f64。空间 ID 不写入帧字段，也不通过取整寻找邻近键。

小编辑器从现有 timeline spatial_keys 读取有界、唯一且按作者顺序排列的 ID、连续局部时间和精确投影。键导航、移到播放头和删除复用空间命令；加键沿用精确 TrackAt，由 worker 分配 ID。图形范围菜单捕获独立身份，读数按连续局部区间投影一次。当前有效空间选区可进入小编辑器。

固定空间区间新增按 ID 修改缓动的协议。模型先验证缓动，再在候选轨道中修改并验证完整工程；该值预览可加入现有手势。Roving 区间继续受恒速距离约束，需先固定区间端点才能编辑缓动，不静默移除约束。没有新增布局、尺寸、颜色、图标或命中范围。

## 验证状态

最终同一二进制的 48 个不同用例通过：新空间入口 6、小编辑器原回归 11、变速回归 9、基础控件 1、mini 范围菜单 6、稳定数值回归 14、显式原生验收 1。新原生项在普通测试组中标为 ignored，随后显式运行通过，仅计一次；另有两个既有手工原生用例未运行。最终带 FFmpeg 的 workspace all-targets check、样式消费者、改动小模块 rustfmt 与 git diff --check 通过。

普通/正放/倒放覆盖加键、导航、移动、删除和曲线。独立数值期望证明局部 7.0625 帧的二次曲线样本为 x=322.125，所有键时间不变。真实 worker 覆盖两种 chrome 所有者、中英文、连续三次预览、目标键消失时取消、迟到松开、完整工程保存及一次撤销重做。实际 GPU 子进程覆盖中文主窗正放和英文浮窗倒放，共六张截图；另一独立进程重开后完整工程一致。仅验证这些窗口和用例，不据此宣称全平台验收。

源码与程序哈希、逐组结果和原始记录见[证据清单](layer-time-spatial-mini-evidence/manifest.json)与[证据包](layer-time-spatial-mini-evidence/evidence.zip)。

首轮问题与处置：第一次检查暴露新协议缺少 Easing 导入，第一次测试构建暴露新方法可见范围不足，均修正生产声明。首轮新增用例为 4 通过、2 失败：本轮测试误把既有“二次”预设控制点写成另一夹具的值，并忽略解析器的显式 InComposition 包装。修正的是新增测试的期望：仍比较完整工程、完整内部命令及合成地址，并加强具体空间协议 schema 的断言；既有断言未放宽。失败源码、程序哈希和日志保留在 before-fixture-corrections。

额外整组 graph_ranges 压力回归被外层脚本 300 秒期限中断。原子进程 PID 30840 随后继续运行，已追踪至退出 101；虽写出了自身检查结果，父组最终断言未执行，不能算作整组通过。保留外层中断记录、原子进程退出、两轮日志和内部结果，不推定 101 的原因。未因观察超时重启该进程。最终定向运行明确只覆盖 mini 范围菜单，不代替这项整组压力验收。

## 错误与观测处置

| 项目 | 分类、证据与限制 |
|---|---|
| MINI-ID | 现有 curve.controls.keys 观察站点扩展到空间快照：重复 ID 等错误经真实渲染入口进入默认日志；180 次交错正常帧验证首次 Warn、累计第 60 次 Error、抑制与恢复。 |
| MINI-EASING | 新增 curve.controls.spatial_easing：真实损坏缓动 JSON 验证默认文件、Error stderr 和 stdout 隔离。计数为进程生命周期累计，成功不重置。 |
| MINI-SELECTION | 新增空间选区发布错误沿用面板观察基础设施，正常认领经 chrome/worker 用例覆盖；畸形提供者导致该发布失败的独立注入仍待补齐，不声称已验证。 |
| MINI-WORKER | 复用现有串行 worker 的请求号、非 Present Info start、在途状态及默认错误信道；模型拒绝非法键、非法缓动、末键及 Roving 区间时完整工程和历史不变。本轮没有改变 worker 阻塞/超时/重启或 panic hook。 |
| MINI-NORMAL | 删除目标、换轨道、取消与迟到松开是正常失效，不将它们记为错误或重新认领邻近键。 |

## 剩余工作

#257 保持打开。伴随锚点补偿与合成画布手势、真实媒体预热/设备/高倍速音频和最终集成仍需完成。空间曲线复制粘贴与空间值方向拖动保留既有不可用边界，不能宣称本轮已支持。没有新依赖或外部素材。

## DESIGN §0.7 处置

本阶段仅接入小曲线编辑的地址、命令与选区，未改变属性行、控件、颜色、尺寸、图标或命中范围。
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
独立验收。全平台、DPI 和所有面板的视觉整改不在本阶段验收范围。
