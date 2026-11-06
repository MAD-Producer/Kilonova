# 效果管线后台准备：任务所有权与原生验证

对应 #64，继续保留该 issue 的完整实施和验收范围。
本批不改变可见 UI，不新增依赖、资产或工作线程。

## 实现边界

- 从前台 GPU prepare 中提取单次原生编译器。描述符仍使用原有完整 WGSL、
  顶点布局、混合特化、目标格式和驱动缓存；每次调用的三个 error scope
  均在调用线程创建、弹出。校验、内部错误和 OOM 的原生原因与附带 poll
  失败保留在类型中，不先转换为字符串。新增 OOM/internal 捕获是功能变化，
  不是单纯增加日志；这两类真实设备故障仍未注入验收。
- 准备任务持有独立 CPU PlanBuilder 和必要 GPU 句柄，不持有 Session 或
  Renderer 的可变引用。渲染 owner 上直接运行返回错误；宿主须提供有界 worker。
  不隐式启动线程，不在 Present 做文件操作，不添加重试或重启。
- 只规划调用者提供的 1～3 个已采样 scene。遍历限制为 256 个嵌套 scene、
  32 层深度；最多保留 128 条管线、8 MiB 用于比较的 WGSL。
  alpha 快照复制前检查 16 MiB，运行时转移其所有权而不再复制一遍。
  这些限制不是 driver、PlanBuilder 临时分配或输入 Scene 总内存的限额。
- 实际严格 preflight 与启用平面覆盖后的绘制计划分别求出需要的管线，
  按完整 WGSL、sprite、additive 和格式去重；可选择 guarded preview 计划。
  不遍历整个效果库。达到准备上限明确返回 limited，余下工作保持现有绘制行为。
- 每个 Renderer 只接受当前 ticket 的 Arc 身份、相同 document generation、
  project revision 与输出格式。请求序号是饱和的诊断计数，不充当身份。
  新请求、registry、资源目录、工作精度或融合策略改变会取消旧任务。
  成功恢复驱动缓存也取消旧任务。
- 安装将一批结果替换到已有的有界复用库，不累积历史批次。
  接口重新计算真实 WGSL 字节数，不能依赖结果的声明计数越过上限。
  编译成功即计入 Renderer 生命周期计数，包括随后取消/过期的工作；安装不重复计数。
- ticket 在 CPU 规划与原生创建前发布阶段并写 Info start；完成写 Debug，
  失败写 Error，正常取消不算故障。状态只使用原子量，无业务/观察锁包围日志。
  丢弃未运行任务或未安装结果会结束其准备状态。不能中断一次尚未返回的
  CPU build、驱动创建或日志 sink 调用；没有声称 driver 调用具有硬超时。

## 新增证据

实际 Windows Vulkan，NVIDIA GeForce RTX 4070 Laptop GPU，驱动 591.86。

1. 独立 worker 上创建真实非法 WGSL，保留 `wgpu::Error::Validation` 原始 cause
   身份、程序 key 和解析位置；同线程下一次合法创建恢复成功。同时保持渲染
   owner 的 validation scope 活跃，该 scope 不捕获 worker 的错误。此用例
   已在编译器提取阶段通过；新增任务之后的完整单元复跑尚待记录。
2. 最终任务代码五项实际测试通过，0 失败、0 忽略：
   - worker 预热实际 invert/tint 工程，安装后首次 capture 不增加前台原生创建，
     复用计数增长；完整 16×16 RGBA 与独立冷 Renderer 一致。
   - owner 线程拒绝、旧请求结果、revision 不匹配、foreign Renderer、
     未运行任务丢弃和准备结果丢弃都不能安装。
   - 受控阻塞时已经能读取 Planning、generation/revision 和零完成数；
     取消后未进入 GPU。受控 planning panic 保留原因、状态 Failed，随后新的
     显式请求仍执行真实创建并能安装。
   - scene 数量与 alpha 超限拒绝不取消已有有效任务；伪造小字节计数仍不能
     将超过真实 WGSL 容量的结果装入复用库。
   - 饱和序号不使新旧 ticket 混淆，Renderer 状态销毁会取消当前请求。
3. 上述最终五项使用真实 render crate、Cargo 产生的锁定依赖与原生链接。
   源码前后 2454 个文件一致，依赖字节前后相同；EXE SHA-256：
   `b3b44d3c14ad02ea4fd470786a93ce0d7c657183d3f3b96042c8b8b37215b087`。
   这是直接 rustc 验证，不能替代最后的 Cargo workspace/all-targets 检查。

## 失败记录与修正

- 首次新测试用结构更新语法构造有 Drop 的 PreparationState，编译拒绝；
  改为 Default 后设置诊断计数。没有改变断言。
- alpha 边界夹具最初还保留 Renderer 自带白色 fallback 的一个 alpha 字节，
  因而不是声明的“上限加一”。隔离新夹具后仍严格断言上限加一的原生错误。
- 严格的“前台不再创建”断言两次失败，保持原样。预热读取上一帧的融合 report，
  尚未绘制时 report 不反映当前开关。新增当前策略 getter，快照读取该值，
  现在同一断言通过。另将真实绘制的平面覆盖计划纳入准备，不以只有 preflight
  成功来推断首次 draw 已预热。
- 保留首次编译错误、两轮失败、最终成功的 stdout/stderr、源码摘要及命令。
  未修改旧断言、生产预算、等待期限、重试策略或既有 ignore。

## 观察审计与剩余工作

| 项目 | 处置及证据 | 尚缺的资格 |
| --- | --- | --- |
| F64-NATIVE | 线程独立原生校验错误、cause 身份和合法创建恢复已验证 | 实际 OOM/internal/poll 联合故障 |
| F64-STATE | 受控阻塞前发布阶段、取消和新请求恢复已验证 | 宿主 worker 的默认文件日志、Error stderr、stdout 隔离 |
| F64-INSTALL | 身份、revision、foreign 和实际字节上限拒绝已验证 | 宿主换工程及完整格式/package 失效矩阵 |
| F64-NORMAL | 缺失缓存、取消、superseded、limited 作为正常分支 | 宿主状态展示与首帧准备提示 |
| F64-SCOPE | 按实际 scene 预热并验证首次 capture 没有新创建 | 近期时间区间采样、带界队列与实际应用接线 |

issue 仍需要：应用私有缓存路径及后台读写；宿主调度与首帧状态；独立进程
cold/warm 和首次/再次添加效果的阻塞时间对照；DX12 的原生回退资格；
最终提交的宿主/桌面编译和全量回归。当前仍不能宣称用户播放已无同步编译。

原始证据：`artifacts/issue-goal-current/native-pipeline-preparation/`；
缓存阶段完整回归另见 `driver-pipeline-cache/render-full-summary.json`。
