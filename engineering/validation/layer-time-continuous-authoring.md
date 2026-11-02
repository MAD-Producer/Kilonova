# #257：连续空间轨道的精确属性输入

接续 `5475cbee`，提交到 PR #579。开工核对开放 PR 与本地 worktree，
#575 在做预渲染，#580／#581／#582 在做跟踪，没有发现重复实现；没有修改这些分支。
本记录是阶段交付，#257 保持打开。

## 功能边界

`property_authoring::precise` 原来只根据图层时间映射判断是否使用精确时间。
但连续空间轨道在恒等时钟下也有小数时间：例如图层 offset 为 2、稳定键 ID 2
的局部时间为 4.125，合成播放头就是 6.125。旧数值入口先取整为 6，模型
`SetVector` 在局部 4 插入一个新固定键，原来的 ID 2 没有被编辑。

现在图层位置／锚点、摄影机位置／目标以及全局摄影机存在连续存储时，共享入口
选择 `TrackAt`，保留原始 f64 合成时间；模型沿用精确投影与稳定 ID 解析。
数值草稿同时捕获文档、合成、generation、播放头、图层时钟和 offset。
上下文变化取消预览，自己的 revision 变化继续有效。未变速、没有连续轨道的
普通对象以及已有部分静态视图维持原入口。未新增持久字段、依赖、素材或观察站点。

同一对象的普通动画属性也不得借此取整；其小数作者地址沿用模型的明确拒绝，
不会悄悄落到相邻整数键。静态属性仍由模型按现有语义处理。

## 验证记录

本阶段 42 个不同测试通过：新增适配器／实际 worker 8、既有属性输入 9、
锚点 2、画布锚点 2、摄影机 5、连续空间模型 8、既有空间模型 8。
两个手工原生验收项保持 ignored，清单记录名称，不计为通过。最终带 FFmpeg 的
workspace all-targets check 通过（77 秒），修改模块格式、样式消费者和
`git diff --check` 通过。原有未使用项等编译警告保留，未新增 shutdown Result 警告。
执行结果、程序与源文件 SHA-256、默认日志及初次失败记录见
[证据清单](layer-time-continuous-authoring-evidence/manifest.json)和
[原始证据包](layer-time-continuous-authoring-evidence/evidence.zip)。

新增模型／适配器用例覆盖六种通道、普通／正放／倒放时钟，以及固定／Roving
两种状态，共 28 个成功组合。完整工程与直接按稳定 ID 编辑的独立期望比较，
并验证撤销重做和实际目录保存重开。12 个恒等时钟负对照直接执行旧命令，
必须产生不同工程和第五个键，证明测试确实捕获取整插入问题。

这里的 Roving 组合只证明一次精确写入。几何编辑会重新分配 Roving 时间，
同一次手势的多次预览还需要将目标稳定 ID 绑定到草稿，而不能每次重新按
播放头查找。当前真实 worker 的拖动夹具使用固定小数键，不代表该场景已完成；
这项仍须补齐后才能宣称连续空间数值手势全部可用。

真实串行 worker 用例通过属性栏和时间轴共用的数值字段开始、拖动和松开，
核对精确时间命令、完整工程及只有一次撤销。播放头、offset、时钟、文档代次、
选择和焦点变化取消预览；迟到的松开不再提交命令。
本阶段没有新增布局或截图程序；这些用例不冒充原生窗口／DPI／独立进程重开验收。

初次新增负对照误将 `SetVector` 当成稳定 ID 操作的拒绝接口，断言 `unwrap_err`
失败；它实际返回成功并插入固定键。修正该新增测试为完整差异及第五键断言，
没有修改既有测试断言或生产校验。初次程序哈希、输出与源码哈希独立保留。
同时处理新增测试的 shutdown Result，不能静默丢弃。

## 错误处理审计

| ID | 分类 | 处置与证据边界 |
|---|---|---|
| CONT-T-01 | 功能修复 | 只改变精确属性时间的识别；稳定 ID、碰撞检查、回滚及命令错误由现有模型／worker 执行。 |
| CONT-T-02 | 既有观察 | 损坏 offset 经过 `properties.numeric_time_begin`／`properties.numeric_time_continue`，不回退整数入口；默认文件记录首次 Warn 和累计第 60 次 Error，Error 同时到 stderr。 |
| CONT-T-03 | 阈值与恢复 | 两个站点各 180 次失败，期间交替健康开始；仍各仅两条文件记录和一条 stderr Error，stdout 无应用日志。观察器为进程生命周期累计计数，成功不重置。恢复后的真实编辑和取消仍能工作。 |
| CONT-T-04 | 正常交互 | 上下文变化、取消、普通静态视图和空撤销不是故障；没有新增逐帧日志、文件 IO、超时、重试或重启。 |
| CONT-T-05 | 既有基础设施 | worker 的 Info start／在途状态、panic hook、阻塞处理不改；本阶段不声称重新验证所有处理器的 panic 与阻塞路径。 |
| CONT-T-06 | 待完成 | 连续空间轨道旧 mini 编辑入口、伴随补偿边界、真实媒体设备／高倍速音频与最终集成验收仍需推进；CI 结果单独记录。 |

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
