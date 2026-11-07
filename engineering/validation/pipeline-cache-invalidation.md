# 管线缓存的源码与工作格式失效验收

## 范围

本批为 #64 补充跨缓存层的真实 GPU 验收，不修改生产缓存策略、渲染语义、
日志、线程调度或用户可见 UI。其他工作树的 UI、深度、跟踪与预渲染改动未纳入。

新增用例位于 `pipeline_preparation/tests/invalidation.rs`，使用实际打包、重新读取的
两份独立测试效果包。两包插件 ID、版本、效果定义及入口一致，WGSL 分别输出红色
与绿色，实际包 hash 不同。用例执行：

1. 冷设备后台预热并验证全部 256 个红色像素，保存真实应用私有驱动缓存。
2. 独立设备恢复缓存，后台预热并验证全部像素与冷设备相同。
3. 完成旧包的准备任务后更换注册表，断言旧票据取消且安装返回 Superseded。
4. 更换源码后要求实际原生创建计数增加；全部绿色像素与独立冷设备相同。
5. 完成旧格式准备任务后通过正式 API 切换 Float16，断言旧票据不能安装；
   要求新格式原生创建计数增加，并与独立 Float16 冷设备比较全部输出像素。
6. 每次预热后的实际捕获均不能增加原生创建或前台创建计数；所有设备健康。

Float16 使用现有显式 SDR_CLIP 输出转换，不把浮点目标交给仅接受 RGBA8 的
PNG 捕获接口。该测试验证显式转换后的像素，不宣称验证任意 HDR 数值误差。
Vulkan 支持时必须实际保存并恢复驱动数据；不支持的后端必须明确返回 Unsupported，
仍执行全部源码、格式、取消与像素验证，不以提前返回跳过它们。

## 缓存失效边界

驱动二进制缓存按官方 adapter key、驱动信息、应用版本、wgpu 与效果接口版本
隔离；wgpu 还校验原生缓存头。一个兼容驱动 blob 可包含多个源码和目标格式，
不因为新增一个格式就丢弃整个 blob。活管线复用另行严格核对完整 WGSL、
sprite、additive 与目标格式。包 hash 参与注册表解析，改变包会取消旧准备票据。
本批验证的是这些层级不会把旧原生管线安装给新源码或新目标格式。

既有低层 proof 用例仍覆盖 blend/vertex 标志与上限，未被新用例替换。
这些测试不是更换物理 GPU 或驱动的实验，也不是驱动报告的命中率测量。

## #64 要求与既有证据

| 要求 | 实际覆盖与边界 |
|---|---|
| 当前和近期区间的实际 program/format/blend，避免同步全库预热 | `host-pipeline-preparation.md`、`preparation-reuse-and-surface.md`、`multiview-pipeline-preparation.md`；有界当前及未来实际场景，不枚举全部效果排列 |
| 后台创建、error scope、session 和 generation 所有权 | `native-pipeline-preparation.md`、`driver-cache-worker.md`；本批增加已完成旧包/旧格式结果拒绝安装 |
| 官方缓存与兼容隔离 | `driver-pipeline-cache.md` 的实际保存/恢复、损坏/不兼容包络拒绝；本批补源码及格式跨层像素证据 |
| 不支持或损坏时回退，首帧之前报告 preparing | `host-pipeline-preparation.md`、`driver-cache-worker.md`、`multiview-pipeline-preparation.md`；实际状态与原始失败保留 |
| cold/warm、首次/再次添加的阻塞测量 | `normal-desktop-cross-process-cache.md` 的独立普通程序 PID、原始耗时与完整 RGBA；`pipeline-precision-native.md` 的精度覆盖。数据没有证明所有热启动都更快 |
| 非法源码精确诊断，取消不得安装旧结果 | `pipeline-preparation-terminal.md` 的普通程序包解析/缺包故障；`driver-pipeline-cache.md` 的真实 native 创建失败；二者不是同一故障边界 |
| DX12/Vulkan 能力明确 | Vulkan 可持久化官方缓存；DX12 仍执行后台创建和内存复用，持久化能力为 Unsupported。没有全平台零同步编译保证 |

文档中的历史待办须结合后续记录阅读，不能据早期未实现状态判断当前主线，
也不能据单个新测试宣布整个 issue 已关闭。最终提交的 CI 与合入状态单独核验。

## 原始证据与失败处置

原始命令、stdout/stderr、每轮源码摘要、设备及 RGBA 位于
`artifacts/issue-goal-current/pipeline-cache-invalidation/`。最初新夹具误用了
Registry 的键类型、遗漏 AllocationBudget 参数；随后误用浮点 PNG 捕获接口。
原始失败保留，只修正本批新夹具，未修改既有断言、生产预算、等待期限或 ignore。

本批没有新增观察点。正常 Unsupported、取消和 supersession 仍是正常分支；
实际创建与存储失败仍由原有 typed error、worker 和宿主信道处理。
测试不冒充普通桌面程序注入 native GPU 故障，也不宣称完成其他 issue。
