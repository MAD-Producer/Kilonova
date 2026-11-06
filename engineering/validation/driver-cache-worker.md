# 驱动管线缓存后台读写：任务与安装边界

继续 #64 / 草稿 #627。该阶段建立后台文件与驱动任务接口，不代表桌面已经自动恢复/保存缓存。
开始前核对开放 PR 的标题、分支和 SHA：#575 仍是最终帧预渲染，#582 是追踪，#626 是深度；
没有审查或修改它们的实现。工作树清理后只使用自己的独立开发与发布工作树。

## 实现与约束

- Renderer 可创建 restore/save job。构造只克隆设备、缓存句柄和有界身份信息，不读文件、不调用
  `get_data`、不创建原生缓存。任务由调用方的有界 worker 执行，本层不创建线程、队列或重试。
- restore 在 worker 上读入、核对完整封装、创建原生缓存并返回 receipt；只有 owner 显式安装
  后才替换 live cache。Renderer 身份使用 Weak/Arc，任务身份使用 ticket 的 Arc，饱和诊断序号
  不参与授权。外来 Renderer 为类型化错误，过期、取消和目标格式变化为正常 Superseded。
- 只有成功安装一个实际恢复的缓存才取消旧管线准备；缺失、不兼容和不支持均不丢弃有效准备。
  同步 restore 也使此前后台缓存请求过期，避免晚到结果覆盖显式操作。
- save 与管线准备是分开的任务，宿主可先发布可用管线，再排队快照/文件同步。读取快照、
  校验、写临时文件、sync 和 rename 都在 worker 上。已有同步接口保留。
- 开始阻塞操作前发布 Reading/Snapshotting 并写 Info start，随后公布 Creating/Writing。
  失败保留原生 IO/GPU cause，panic 保留有界文本；Error 使用既有 logger，终态轮询不重跑。
  request、operation、路径（最多 384 字符）、耗时和原因（日志最多 2048 字符）可诊断。
- 取消发生在每个可能阻塞阶段之间，原生驱动和文件系统调用不强行中断，不创建替代 worker。
  原子发布前再次检查取消并清理临时文件。rename 为发布点，其后取消不撤销已成功发布的文件。
  取消检查与 rename 之间仍有协作式取消的竞态窗口，不宣称硬取消、事务回滚或目录掉电安全。
- 当前 16 MiB 仍只是快照/读入限制，不约束驱动堆或 `get_data` 在返回前的临时分配。
  调用方仍需限制待执行/待安装任务与同时运行数量；公开任务 API 自身不提供线程资源配额。

恢复入口仍为 unsafe：调用方必须保护应用自己保存的文件在排队和执行期间的来源。
封装 SHA-256 是损坏检测，不是认证。不能向该入口传工程、插件或下载文件。
应用私有目录的创建、权限与自动调度接线尚未交付，不能由这个 API 文档推导为已完成。

## 验证记录

原始记录位于 `artifacts/issue-goal-current/native-pipeline-preparation/`。

- `cache-worker-render-tests`：首轮编译失败，新增饱和序号夹具对实现 Drop 的 TaskState 使用了
  struct-update 初始化，触发 E0509；尚未运行测试。仅改为先 Default 再设置诊断序号，
  所有断言保留。原始失败不覆盖。
- `cache-worker-render-corrected`：实际 Cargo 链接和执行缓存相关 8 项，8 通过、0 失败、
  0 忽略，322 项过滤；含 5 项新任务验证和原有 3 项缓存回归。总耗时 106.938 秒，
  测试本身 24.67 秒。不能当作 330 项完整渲染库回归。
- 真实 Windows / NVIDIA GeForce RTX 4070 Laptop GPU / Vulkan / 591.86：worker 保存
  50,795 字节缓存，独立 device 在 worker 上恢复，安装使用同一个原生句柄；冷、暖实例
  各两次真实效果创建都传入 driver cache，完整 16×16 RGBA 相同。损坏校验和返回精确原因，
  保留工作句柄和像素；身份不符和缺失不清空缓存。该设备证据不覆盖 Metal/DX12 分支。
- 控制阻塞已观察到 Snapshotting，取消后不生成文件；主动拒绝在 owner 上执行，真实 worker
  panic 保留原文本，队列丢弃、Renderer 丢弃、显式恢复和新请求可恢复。跨 Renderer、
  格式变更、更新任务和同步 restore 都拒绝旧 receipt；成功安装才取消此前真实准备票据。
  原子发布的独立取消测试保留旧文件且无临时文件残留。
- `cache-worker-preparation-regression`：执行上述实际 Cargo 生成的同一原生 render EXE，
  原有后台准备 6 项全部通过、0 失败、0 忽略，324 项过滤；总耗时 33.438 秒。
  覆盖 live/retired 复用、旧 owner/代次拒绝、阻塞发布、取消、panic 和真实像素。
- `cache-worker-desktop-default-log`：完整带 FFmpeg 的桌面测试程序编译完成，启动时实际
  `STATUS_DLL_NOT_FOUND`（0xC0000135），尚未执行任何测试。原始失败保留。
  PATH 使用两种分隔形式以及在 runtime 目录启动都仍失败；五个原始 DLL 的独立加载成功。
  在测试 EXE 旁为现有开发 FFmpeg DLL 建立硬链接后，同一 EXE 的测试列表可正常启动。
  记录了原始路径、文件大小和 SHA-256；没有修改 DLL、断言、超时或生产代码。
  这仅修正本地测试运行时布置，没有据此宣称发布打包或操作系统搜索策略已验收。
- `cache-worker-desktop-runtime-fixed`：同一源码、同一 Cargo 命令重新执行，真实生产 logger
  验证 1 项通过、0 失败、0 忽略，1833 项过滤；总耗时 21.406 秒，测试本身 10.24 秒。
  Vulkan 上真实 `open` 目录失败保留 IO source 和拒绝访问（os error 5）；Info start
  先于错误，原始原因及 request/operation/path/elapsed_ms 同时进入文件和 Error stderr，
  stdout 没有产品日志。65 次终态读取不重复报错，新显式 save 在同一设备恢复。
  日志保留机器已有 Epic Vulkan overlay 缺失信息，不将它隐藏或归因于本轮缓存任务。
- `cache-worker-all-targets`：实际 `cargo check --locked -p motion-render -p motion-host
  -p motion-desktop --all-targets --features motion-desktop/ffmpeg,motion-host/diagnostics,
  motion-host/ai-depth` 通过，190.875 秒。34 个编译 warning 来源文件相对本轮基线均未改变，
  不声称全库无 warning。命令实际参数以归档 JSON 为准。
- 六次实际命令均在开始/结束核对 2461 份源文件，命令执行期间源码一致。保留首次夹具编译
  失败和首次 DLL 启动失败，随后验证未修改既有断言、预算、超时或 ignore。

受控阻塞验证来自 render 任务状态测试；桌面默认 sink 验证来自真实 IO 错误测试。
它们不是在桌面 logger 下同时阻塞原生文件读取的单个端到端测试，也不代表完整桌面、
宿主或 330 项 render 库回归。独立进程计时、自动缓存调度与该扩展资格仍需继续。
本轮没有强制诱发支持设备的 `get_data()==None`、原生缓存创建错误、清理同时失败或
桌面缓存任务 panic hook；保留对应类型化分支，不把实际 IO 单项资格扩大为全部故障覆盖。

证据中的 `restore.cancelled=true, phase=installed` 表示该已安装票据后来被新请求发出了取消
信号，不能解释为曾回滚安装；终态保留实际已完成结果。elapsedMs 是票据建立以来的时间，
不能将随后又进行的测试时间当作驱动恢复耗时或启动基准。

## 审计处置

| 项目 | 处置 | 证据 / 尚存边界 |
| --- | --- | --- |
| 64W1 后台读入、原生创建、快照和原子保存 | 新实现，原生窄范围通过 | 实际独立设备与句柄安装、完整像素对照；未接自动调度 |
| 64W2 取消、owner、票据与格式安装检查 | 新实现，原生通过 | 实际阻塞/丢弃/panic/恢复；外来错误与正常过期分开 |
| 64W3 start、阻塞状态、原始 cause、默认输出 | 新观察，按上述边界通过 | 实际状态阻塞与独立默认 sink 验证；保留原始 IO cause |
| 64W4 缺失、不兼容、不支持 | 正常分支 | Vulkan 已验缺失/不兼容；不支持分支待跨后端实测 |
| 64W5 应用私有目录与自动后台调度 | 未完成 | 没有把不可信导入升级为安全缓存 API |
| 64W6 独立进程 cold/warm 与效果重加计时 | 未完成 | 同进程独立 device 不能证明启动/播放收益 |

未新增依赖、第三方素材、工程格式、渲染语义或面板样式。
既有证据包未覆盖。本轮独立归档为
[driver-cache-worker-evidence.zip](driver-cache-worker-evidence.zip)，57 成员、1,543,243 字节，
SHA-256 `505b6c8fe84e55ac8c3d818022ec8419fefa945313e450cb3f9e08c9d28750e2`。
已逐成员核对 SHA-256 和 ZIP CRC，包含六条命令原始结果、首轮失败夹具的精确源码、
基线/当前改动、生产日志三路输出、真实 RGBA、运行时探针、文件与 EXE 身份以及采集脚本。
