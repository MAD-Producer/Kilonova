# Vulkan 效果管线编译缓存：首批实现与验收边界

对应 #64，基于主分支 `daf806b5`。本批不关闭该 issue。

## 归属核对

开始前读取开放 PR 的范围和文件名：#575 已包含最终合成帧缓存及后台预渲染，
因此不另做 #65。#582 为追踪任务，#623 为深度任务。旧
`perf/pipeline-preflight` 工作树仅有 2026-10-17 的两份未提交文件，原样保留。
新实现位于独立分支，不修改其他工作树。

## 已实现

- 按 adapter feature 启用 `PIPELINE_CACHE`，使用官方 `pipeline_cache_key`。
  wgpu 30.0.1 当前仅为 Vulkan 提供应用管线缓存；DX12/Metal 正常走无此缓存的路径。
- 实际效果创建描述符引用 driver cache。成功创建计数采用饱和累加，明确不是驱动命中率。
  原有按 WGSL、sprite、blend 和目标格式证明的 live pipeline 复用保持原语义。
- 提供显式保存/恢复接口。普通绘制和 Present 不进行文件操作，没有增加线程或重试。
  驱动提取、文件写入和同步可能阻塞，接口必须由后续宿主在非 Present 阶段调度。
- 文件封装包含官方 adapter key、driver/driver_info、应用/wgpu 接口命名空间的摘要，
  原生数据长度及 SHA-256；最多读取 16 MiB 数据加头部和一个越界检测字节。
  此上限只约束快照与读入数据，不代表限制驱动内部缓存或 `get_data` 的临时分配。
- 读取损坏或创建失败时保留已有缓存与 live pipelines。缺失、版本/驱动命名空间不符、
  后端不支持为明确的正常结果；其他失败保留原生 IO/GPU cause 并通过 Result 返回。
  GPU error scopes 归本次创建所有，全部弹出，并保留同时出现的附带错误。
- 保存使用同目录独占临时文件、文件同步与重命名；失败会清理临时文件。
  清理也失败时保留主因与附带 IO cause。未宣称目录同步、掉电安全或磁盘配额验收。

`restore_driver_pipeline_cache` 为 unsafe 接口：仅接受应用自己保存、受本地目录保护的
快照，不能用于工程、插件或网络导入。校验和处理意外损坏，不认证攻击者构造的数据。
这遵守 wgpu 的原始 driver 数据来源约束，不把任意字节变成安全的公开导入协议。

## 已有原生证据

真实设备：Windows，NVIDIA GeForce RTX 4070 Laptop GPU，Vulkan，驱动 591.86。

- 首批三个测试通过，无忽略：封装损坏/截断/超限、原子发布与原生 IO cause、实际设备恢复。
- 两个独立 device 在同一进程内创建真实 invert+tint 效果管线；保存快照 50,795 字节。
  冷实例两次成功创建传入缓存，恢复实例在正常构建与故障恢复后累计三次。
- 对比完整 16×16 RGBA 数据，共 1,024 字节；恢复、损坏拒绝及命名空间失效后像素完全一致。
- 在已经恢复的缓存上注入真实非法 WGSL，设备仍报告
  `effect program pointwise-v1-0-invert-tint` 的解析错误；失败未计为成功管线。
  下一次显式合法请求恢复像素。它不是伪造返回值或放宽原有断言。
- 最终编译与三个测试前后 2,450 份源码摘要一致。
- 缓存阶段 `5a31ac63` 的完整锁定渲染回归已结束：
  `cargo test -p motion-render --locked -- --test-threads=1`，123 组结果共
  978 通过、0 失败、1 项原有忽略，耗时 3765.094 秒（含编译）。前后 2,450 份源码一致。
  此结果不冒充随后增加后台任务代码的全量资格；后续验证见
  [后台任务记录](native-pipeline-preparation.md)。

首次编译的新测试误引用 RenderStats 字段，第二次误将未启用 serde 的 AdapterInfo
直接序列化，均保留原始编译失败；只修正新测试的 API 使用。首次运行因 Windows
默认临时目录权限失败，未进入 GPU；改用已有可写临时目录后同一 EXE 通过。
未修改任何既有测试断言、预算、等待期限或 ignore。

## 仍未完成

- 宿主应用缓存目录接入、后台读写、默认错误信道及卡住操作的状态发布验证。
- 渲染层已有独立后台任务、取消/代次安装边界及实际 GPU 验证，详见上述记录；
  宿主队列与近期区间采样尚未接入。
- 插件/package 与精度/格式变更的完整失效矩阵；当前原生用例仅覆盖一组效果和一个格式。
- 独立进程 cold/warm、首次/再次添加效果的配对阻塞时间测量。本批同进程像素控制
  不可用来宣称启动提速；桌面驱动本身也可能已有缓存。
- 完整渲染、宿主集成及本提交精确头 CI 资格；当前三个测试不能代替这些范围。

## 来源

没有新增依赖、第三方素材、UI 或渲染语义。已阅读 PROVENANCE.md、ATTRIBUTION.md
和 NOTICE；实现独立编写，只使用已有 wgpu、sha2 和标准库 API。

- [wgpu 30.0.1 PipelineCache](https://docs.rs/wgpu/30.0.1/wgpu/struct.PipelineCache.html)
- [官方 adapter key](https://docs.rs/wgpu/30.0.1/wgpu/util/fn.pipeline_cache_key.html)
- [Device 创建数据的安全约束](https://docs.rs/wgpu/30.0.1/wgpu/struct.Device.html#method.create_pipeline_cache)

原始命令、stdout/stderr、源码摘要、EXE SHA-256、设备记录和失败记录暂存于
`artifacts/issue-goal-current/driver-pipeline-cache/`，尚未打包为可复核发布证据。
