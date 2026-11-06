# 宿主效果管线准备：有界调度与当前帧边界

本记录对应初始宿主接线阶段；后续管线复用、实际窗口与更广回归结果见
[后台准备的管线复用与原生窗口验证](preparation-reuse-and-surface.md)。

继续 #64、草稿 PR #627。实现和验证均位于独立工作树。
开始前核对开放 PR：#575 为最终帧缓存/预渲染，#582 为追踪，#626 为深度；
不修改它们的工作树，不做这些 PR 的代码审查。#625 已不在开放列表中。

## 行为

- 每个 Session 懒创建一个 worker，跨 surface detach/attach 和工程替换保留；
  不因原生创建未返回而创建替代线程，不自动重启已断开的 worker。
  一个排队请求、一个最新待入队请求、一个完成通道槽；工作线程本身还可持有
  一个正在运行或等待投递的结果。总保留量按这些槽位约束，而不是无限追加队列。
- `try_send` 的队列满是正常背压，只替换尚未入队的旧请求；不会重跑已失败的
  CPU/GPU 操作。新请求由 Renderer ticket、document generation、revision、
  surface/view、尺寸和预览模式约束。饱和序号不充当身份。
- 当前帧未准备好时继续呈现 chrome/上一画面并返回 pending。已经安装过同一
  修订、surface/view、尺寸与预览模式后，后续时间窗口的预热不再次拦截当前画面。
  `preview_info.pipelinePreparation` 报告 ticket、preparing、队列状态、限制和错误。
  没有新增可见面板或样式；准备状态的最终可见提示仍需桌面接线和原生窗口验收。
- worker 在已发布 Planning 与 Info start 的边界内准备表达式并采样未来场景。
  每次最多包含当前场景和两个未来样本，优先取两秒内根图层的实际起止边界。
  没有根边界时取窗口末尾；考虑浮点 clip interval。嵌套/时间重映射专属的
  未来边界尚未完整枚举；周期性任务会更新已采样的当前嵌套场景。
- 单独保留原生编译器的严格多场景 API。宿主显式选择 best-effort lookahead：
  当前场景失败仍返回原始错误；未来采样或规划/创建失败写 Error 并保留原始
  RenderError、帧号及有界诊断结果，不因未来错误拒绝当前合法画面。
  未来真实进入该图层时仍走正常错误路径，没有把失败当作成功管线安装。
  每次最多两个未来错误，不通过截断原生 cause 来制造成功。
- 失败结果使用 Arc 保留原始宿主/渲染原因，正常轮询返回同一错误身份，不反复
  入队；明确的新修订可以提交新请求，仍复用同一个 worker。工程替换和 detach
  显式取消任务；关闭时放弃回复属于正常协议分支，不在 owner 上等待驱动线程退出。
- 没有持有 Session 或观察互斥锁调用 worker 的日志。新 Host 错误包装仍支持
  `composition_detail()` 提取原始结构化错误。
- alpha 快照超出准备专用容量时明确报告 limited 并保留原有渲染路径；不放大或
  缩小生产渲染预算来通过预热资格，也不把准备容量限制变成工程不可渲染。

## 实际证据

`cargo check --locked -p motion-host --all-targets --features diagnostics,ai-depth`
在初始宿主接线阶段通过。随后增加未来错误保留策略并执行实际原生链接的
`cargo test --locked -p motion-host --lib session::pipeline_preparation::tests:: --features diagnostics,ai-depth -- --test-threads=1 --nocapture`：

- 初始三项通过，0 失败、0 忽略；追加负向条件后五项通过，0 失败、0 忽略。
- 当前 tint 和未来进入的 invert 均由实际 worker 准备；前台 capture 不增加
  native pipeline 创建，未来画面与独立冷 Renderer 的完整 RGBA 一致。
- 实际阻塞采样阶段已能读取 Planning；连续替换 32 个修订请求未等待阻塞调用，
  保持同一个线程及有界 pending 槽，释放后只安装最新修订。
- 未来插件 hash 缺失时，保留与实际冷 preflight 完全一致的失败原因及未来帧；
  当前画面可显示，真实进入未来帧仍失败，随后当前帧仍可显示。
- 真实 worker 受控 panic 被原有任务边界捕获，保留原因；65 次轮询均返回同一
  Arc 错误和 ticket，创建计数仍为零；明确新修订后真实创建恢复、线程身份不变。
- 浮点根 clip 起止边界的采样列表有序、去重、最多两个。

当前五项不是完整宿主回归或原生窗口资格；测试进程未初始化桌面的生产日志 sink，
因而不能据此声称默认文件/Error stderr/stdout 隔离已验收。

## 仍需完成

- 完整宿主、桌面与渲染回归，实际窗口 pending→首帧及保留画面行为。
- 生产日志 sink 下的 Info start、阻塞状态、原生错误/Error stderr、stdout 隔离。
- 工作精度/package/格式及实际换工程、换 surface 的完整失效矩阵。
- 嵌套/时间映射的近期边界、exposure 预览路径接线。
- 复用已存在的 native pipeline，减少每个后续窗口的重复后台创建；当前实现
  尚未证明后台窗口开销相对于收益足够小。
- 应用私有缓存目录、后台恢复/保存与冷/暖独立进程、首次/再次添加效果计时。

原始输出及源码摘要：`artifacts/issue-goal-current/native-pipeline-preparation/`
中的 `host-worker-check`、`host-worker-native`、`host-lookahead-native`。
不关闭 #64，也不以这五项窄测试代替上述剩余条件。
