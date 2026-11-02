# #257：路径关键帧导航与增删使用局部身份

接续 `e26abf9d`，交付至草稿 PR #579。开工核对开放 PR 和最近分支：
#575 为原生预渲染，#580 为外部跟踪／相机导入，没有发现路径导航的重叠工作。

## 行为与边界

映射图层的蒙版和矢量参数上一键／下一键按钮、J/K 使用图层属性时钟，将有符号
局部键投影到合成时间。选择存入共享关键帧集合的 LocalKey，不再把分数合成帧
强制转成 u32。3/2 伸缩、offset=2 时，2、6.5、11 合成帧对应局部键
-6、-3、0；倒放时顺序反转。源时间重映射不参与这些属性键寻址。

菱形按钮使用精确 TrackAt 或有符号 LocalKeyframes。整条蒙版姿态作为一批
命令原子提交，部分节点已有键时只删除已有键，不给其他节点补出新键。整条路径
导航仍选择所有节点在同一局部时刻的身份；缺键姿态不能被悄悄复制成部分节点。
新增整数键从精确时间采样；非整数局部时刻禁用整数键插入，锁定、只读和失焦
保留原门禁。共享 Delete 扩展到 Masks／Vector／Properties 的键选择，不改
合成视图的图层删除作用域。新测试通过实际 worker EditorAction 验证此路由。

矢量组参数的连续空间键仍被模型的明确能力检查拒绝，本阶段不删除该检查、
不修改渲染语义，也不声称完成组参数的 Roving 支持。新增夹具最初尝试构造
这种工程，验证失败后依照当前模型将其改为明确拒绝的负对照；生产校验未放宽。
原有连续空间位置轨道的其他入口仍须另行核对，#257 保持打开。

未映射的既有路径轨道行为不变。本阶段没有新的布局、尺寸、颜色、素材或依赖。

## 验证

最终 196 个不同测试通过：映射路径 11、其余路径 97、关键帧导航 10、共享
关键帧选择 77，以及显式运行的原生验收 1。另有 12 项人工验收未运行，不计为
通过。所有组均检查运行前后的同一可执行文件 SHA-256，详见
[清单](layer-time-path-navigation-evidence/manifest.json) 和
[完整证据包](layer-time-path-navigation-evidence/evidence.zip)。

原生验收覆盖 Masks／Vector、正放／倒放、中英文及主窗／浮窗共 16 组场景，
保存 80 张截图，16 份工程在独立进程重开后完整状态一致。按钮和 J/K 导航、
有符号键选择、原子删除与撤销、分数局部时间禁用插入、缺键姿态禁止部分复制
均经实际控件及 worker 验证。它们不是物理鼠标、真实音频设备或全部媒体的验收。

带 FFmpeg 的 workspace all-targets check 通过（49.60 秒），样式消费者检查、
新增模块 rustfmt 与 git diff --check 通过。最终编译后仅将新增测试 Harness
的 EditorAction 分支按 rustfmt 排版；生产代码没有变化。证据包保留编译时源码、
格式化输出及差异，打包脚本确认格式化结果与最终文件逐字节一致；不声称两份
测试源码哈希相同。已有编译警告与 Epic Overlay Vulkan 清单告警不算作本阶段修复。

初次验证证明参数面板导航后的 Delete 未进入共享键删除作用域；修正实际路由，
保留失败工程差异。测试夹具新增 EditorAction 分支时曾错误地提前 unwrap，
导致类型不匹配；只修正新夹具，原始编译日志保留。既有断言和生产模型约束不改。

## 可观测性审计

| ID | 分类 | 处置与限制 |
|---|---|---|
| PATH-N-01 | 功能修正 | 时钟投影、局部身份、原子增删、完整姿态选择及 Delete 作用域；不是只增加日志。 |
| PATH-N-02 | 新观察 | `mask.navigation_time`、`vector.navigation_time`、`path.navigation_selection` 三个固定站点记录对象／参数／阶段与原始原因。 |
| PATH-N-03 | 既有覆盖 | worker Info start／在途状态、原子候选校验、EditorAction 来源校验、Error 文件和 stderr 沿用现有实现。 |
| PATH-N-04 | 正常分支 | 非整数局部时刻、无相邻键、取消、锁定／只读和缺键姿态是正常不可用能力，不伪造帧号或每帧警告。 |
| PATH-N-05 | 观察范围 | 沿用面板进程生命周期累计：首次 Warn、第 60 次 Error，后续抑制；健康调用不复位，新进程重置。固定站点、饱和计数及 4096 字符上下文上限不变。没有每帧 IO、重试、重启或新等待期限。 |
| PATH-N-06 | 实际失败验证 | 三站点在默认日志等级分别注入 120 轮异常并穿插健康调用：文件各保留首次 Warn 和第 60 次 Error，共 6 条；stderr 共 3 条 Error，stdout 无日志污染。后续抑制、进程累计不因健康调用复位、worker 工程不变均通过。原有四个数字编辑观察站点也回归通过，属于既有覆盖。 |
| PATH-N-07 | 未完成范围 | 相机专项手势、连续空间补偿与旧编辑入口，以及真实媒体预热、音频设备和高倍速采样尚未完成，不关闭 #257。 |

## DESIGN §0.7 逐项处置

各项均保留既有排版，独立视觉合规仍 pending verification；retained 不表示原问题已解决。

| 基线位置／原行为 | 本阶段处置 |
|---|---|
| path_controls.rs 六处字面行步进 | retained with reason：不改行步进。 |
| path_controls.rs 双列分割 | retained with reason：不改列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：不改命中尺寸。 |
| path_controls.rs 自绘数字背景 | retained with reason：不改数字绘制。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：不改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：不改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：不改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：不改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：不改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：不改效果树。 |
| timeline.rs 十一列坐标 | retained with reason：按兼容基线保留。 |
| timeline.rs 吸附线颜色 | retained with reason：不改主题和吸附反馈。 |

§5.2 没有增加属性行或压缩命中区域。§12 验收只覆盖具体路径参数场景，
不能证明所有布局、平台和 DPI 合规；路径面板既有数字底部裁切不算作本阶段修复。
