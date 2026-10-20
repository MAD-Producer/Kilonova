# #237 图层边界对齐验收

## 范围与实现

开始前核对工作树、开放 PR 和文件范围；避开快捷键、媒体清理、渲染采样与已修改属性分支。新增依赖、第三方素材及兼容名称均为零，已核对 PROVENANCE、ATTRIBUTION 和 NOTICE。

六个方向（左、水平中心、右、顶、垂直中心、底）使用同一求值场景的现有 selection_geometry 边界，而不是图层位置或未旋转宽高。目标可选合成或全部所选图层的联合边界；主选不固定。边界沿用现有可选轮廓的保守范围，不宣称逐像素透明度包围盒。旋转、非中心锚点、缩放、父级变换及同时选中父子均补偿；父子顺序不影响结果。

通过既有异步引擎信道执行，先完成整组规划，再一次批量提交。位置分离时只写改变的 X/Y；保留 Z、其他变换、时间偏移及无关关键帧，一步撤销/重做。播放先停止；属性写入使用整数帧，因此小数帧停止后在向下取整的同一帧求值和写入，成功后显示该帧，拒绝时保留原时钟和场景。已对齐时不添加历史。

空、重复、缺失、过量选择，锁定、3D、不可见、不活动及非画面图层、位置表达式、屏幕叠加或不可逆父级等会明确拒绝整组，不部分移动。选区目标至少需要两层。面板提前显示常见禁用原因；工作线程仍独立验证最新工程。仅实现 #237，不包含 #238 分布功能。

## 错误处理与正常边界

| 编号 | 处置与证据 |
| --- | --- |
| ALIGN-01 现有观测 | 新命令名称 align_layers 接入原有 serial worker：进入 handler 前发布 in-flight 和 Info start，完成计时及清除状态沿用现有协议，Present 热路径不变。请求有界为 MAX_LAYERS；不记录用户文本或整个工程。 |
| ALIGN-02 真实失败 | 实际 worker 请求包含不存在的图层 999，原始原因包含对象 ID，经回复进入既有 UI 错误通道；默认级别文件和 Error stderr 均含 operation 与原因，stdout 无日志。随后有效请求成功，失败前后的工程、revision、undo、redo、frame 均相同。 |
| ALIGN-03 正常分支 | 弹层外点击取消、单层选区禁用、已对齐无命令属于正常输入/无操作，不增加错误或重试。没有新增计数器、重启、超时策略或 panic 吞没。 |
| ALIGN-04 功能边界 | 真实批量提交和已有原子历史处理承载功能错误；未增加另一个回滚协议。原生截图夹具结束时撤销自己的临时批次并恢复呈现，清理错误与主失败同时发生时保留两者。 |

## 已验证证据

- host 定向 5 项通过：24 个方向/目标/分离位置组合，实际重新求值轮廓；父子不同选择顺序；拒绝和已对齐；有时间偏移的既有动画键；表达式与零缩放拒绝。实际工程保存重开、一次撤销和重做均比较完整数据。日志 `artifacts/layer-edge-alignment/host-verified-tests.log`。
- desktop 定向 3 项通过：中英文、主窗/原生浮窗 chrome 的真实输入路由、一次 typed action、禁用及下拉取消；真实 worker 错误和恢复、小数帧、日志分流、保存重开；原有位置分量回归保留。日志 `artifacts/layer-edge-alignment/desktop-verified-tests.log`。
- 带 FFmpeg 原生构建成功，21 条既有警告；中英文真实 GPU 截图通过：合成目标、实际目标下拉、选区对齐前和对齐后。每次截图先实际 present，比较工程和历史，夹具结束恢复原场景。证据 `artifacts/layer-edge-alignment/native/` 及 `capture-verified.log`。没有新增右键菜单。
- 初次新增夹具错误使用 Engine::new_project 前的根目录重开，已改读实际 snapshot.root；初次截图还暴露夹具撤销后的 revision 未同步，以及只更新控制柄未 present 新画面，均修复后重跑。失败日志保留；没有放宽既有工程/历史断言，旧图不计作证据。
- 整合主线 bc5c8b66 后，完整 host 库回归 40 通过、0 失败（58.39 秒，host-complete.log）；完整桌面回归 598 通过、0 失败、1 个既有音频设备忽略（207.78 秒，desktop-complete-verified.log）。桌面首条命令误加 --lib，在编译前被拒绝，已按实际二进制 target 运行。该轮不包含随后合入的 #344，不能冒充采样功能的组合回归。
- 新增测试最初直接读取 default_stylesheet，被面板样式守卫拒绝；改为从实际测试 PaintList 读取 styles 后通过。来源守卫检查 1,484 个 tracked paths 通过，差异检查通过。没有修改守卫、断言或实际主题值。
- 随后整合主线 2a9cd061（#344 图层采样），定向重跑 host 5 项（7.08 秒）和 desktop 3 项（9.88 秒）全部通过；没有把定向重跑称为整套回归。带 FFmpeg 的组合原生构建成功（48.60 秒，21 条既有警告）；来源守卫 1,528 tracked paths、面板样式消费者及 diff 检查通过。证据分别为 host-sampling-integrated.log、desktop-sampling-integrated.log、native-sampling-integrated-build.log。
- 组合构建的中英文真实 GPU 截图再次通过，包含各四幅对齐图和原有 property-followups 验收；夹具工程与历史恢复断言保留。证据 native-sampling-integrated/ 与 capture-sampling-integrated.log。已目视复核英文对齐后、中文对齐前后。新增 Rust 模块 rustfmt 检查通过。

## DESIGN §0.7 逐项处置及 §12 限制

| 原基线 | 处置 |
| --- | --- |
| path_controls 六处行推进 | retained with reason：不改路径行排版。 |
| path_controls 双列分配 | retained with reason：不改路径字段几何。 |
| path_controls 混合按钮高度 | retained with reason：不缩小既有命中区。 |
| path_controls 数值背景 | retained with reason：不改路径数值绘制。 |
| path_controls 全宽布尔按钮 | retained with reason：不改路径布尔控件。 |
| effect_controls 行高与推进 | retained with reason：不改效果行。 |
| effect_controls 布尔文本按钮 | retained with reason：不改效果控件类型。 |
| effect_controls 全宽枚举 | retained with reason：不改效果枚举；新对齐目标使用现有 Dropdown。 |
| effect_controls with_alpha 悬停 | retained with reason：不改该分支。 |
| effect_controls 树缩进 | retained with reason：不重写效果树。 |
| timeline 列坐标 | retained with reason：保持既有列坐标与交互。 |
| timeline 吸附线颜色 | retained with reason：不改拖动反馈。 |

对齐面板所有颜色和尺寸从现有 semantic token 读取，使用原生动作按钮及 Dropdown；未新增虚构 token。已验证截图 DPI 1，中英文主窗及浮窗逻辑输入；高 DPI、其他 OS 的人工交互仍待验证，不声称 DESIGN 全项合规。六方向实际图像语义以重新求值边界测试为证，原生截图另验证可见呈现。

## 原生验收修复与主干集成（2026-10-20）

接入 #368 `1381cc133c3407d1f4af9eff8a0c2beeaa257eb1`，集成提交 `28bc31c`。相对原 head 引入 16 个上游文件，包含已经合并的媒体锁外清理、渲染像素验收、固定 FFmpeg 下载源、Intel 预算及 #368 的测试夹具/诊断收集修复；对齐规划、UI、命令路由和 token 未改。没有放宽原断言或等待时间。

本次真实组合定向验收：host 对齐 **5 通过，10.27 秒**；桌面对齐交互与真实 worker 故障/恢复 **2 通过，17.80 秒**；原生四模式采样 **1 通过，31.87 秒**；中英文 PNG 导出 **1 通过，26.23 秒**；工作流 **5 通过，6.389 秒**。日志为 `artifacts/pr-queue/alignment-native-ci-{host,desktop,sampling,frame-export,workflow}.log`，来源守卫（1535 跟踪路径）、面板样式消费者及差异检查也通过。未重跑完整桌面/渲染或 FFmpeg 发布构建，不能将旧 head 的全套结果当作本次新结果。

本次没有用户可见界面变化，原生界面截图仍对应本文件前述正常对齐功能，不声称重拍或补齐高 DPI/其他 OS 人工交互。最终组合 CI 待完成。堆叠顺序为 #368 → #365 → #366，父 PR 合入 main 后再检查/调整子 PR 的 base。
