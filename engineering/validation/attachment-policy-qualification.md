# 附件候选策略的真实编码与成本对照

关联 #63，接续 `2bc4d32454aeea8ddff7e289508401fccdf69902` 的覆盖/消费者诊断。默认产品策略继续为透明 Clear/Store。本批没有把测试候选开关暴露给应用程序，也没有因小样本计时改用 Load。

## 改动与责任边界

| 编号 | 已验证内容 | 范围与限制 |
| --- | --- | --- |
| 63-B1 真实原生操作 | 仅在 `cfg(test)` 的 `GpuState` 增加每 pass 操作、旧像素污染与实际编码顺序记录。原 `encode_pass` 消费这些操作，未复制另一个渲染器。 | 正常产品构建完全不包含控制结构和污染 pass；默认 API 仍描述 Clear/Store。反事实测试的实际操作另见每策略记录。 |
| 63-B2 像素对照 | 在附件中预写不透明洋红色，然后分别实际编码 Clear、只对完整覆盖候选使用 Load、错误的全 Load、错误的全 Discard。比较完整 RGBA。 | 污染只用于正确性测试，不进入成本采样。后续读者存在时的 Discard 为故意错误的负对照，不能当优化。 |
| 63-B3 原生覆盖矩阵 | 透明 blur/tint、真实 IR Kill、自编蒙版、track matte、Multiply、adjustment accumulator、可见 sprite、完全剔除 sprite，共 8 类；各跑 `(0,96×64),(20,48×32),(7,96×64),(0,96×64)`。 | 共 32 次组合；64 次正确操作运行均与默认像素相同。68 个容量留白附件、8 个 sprite 附件、4 个 Kill 附件实际进入编码，不只是改报告标志。当前默认精度与一台 Vulkan GPU，不能代表所有格式/设备。 |
| 63-B4 故意错误的操作 | 全 Load 在 12/32 次组合中产生旧像素；全 Discard 在 32/32 次组合中损坏后续消费者结果。每个夹具 pass 恰好编码一次；所有策略严格保留 Clear 对照的实际执行顺序。 | matte 合法地先 snapshot 源，再按堆栈合成，实际顺序不等于 pass 编号顺序。图中最后使用点不是 native 时间戳，也不能据此直接删 Store。 |
| 63-B5 成本采样 | 实际原生 640×360 全尺寸 blur/tint 链，15 个 pass。Clear 为 0 个 Load，候选为 15 个 Load，均保留 Store。两个独立进程，各按 Clear/Load/Load/Clear 四块运行。 | 每块 3 个预热 + 8 个保留采样，原始 88 帧记录全部归档；没有硬编码速度门槛、丢掉慢样本或延长产品等待期限。仅测试等待 GPU 完成有 10 秒上限。 |
| 63-B6 产品路径 | 普通非测试 `effect_plan_probe --attachments` 构建并运行成功，17 个 pass 仍报告透明 Clear/Store。 | 原生反事实的开关没有进入普通构建。未新增日志信道、worker、超时恢复或重试；库原有 Result 传播和默认错误输出边界保持不变。 |

Kill fixture 使用已有第一方 schema 创建独立 WGSL，并经真实 `package_directory` / `EffectPackage::from_bytes` 校验后安装。sprite fixture 复用已存档的第一方 SDK 包，只进入单元测试，不恢复已移除的产品粒子预设。没有新增第三方依赖、素材、兼容名称或 UI 样式。

## 桌面 GPU 结果

设备为 NVIDIA GeForce RTX 4070 Laptop GPU / Vulkan。使用已有 `GpuTimer` 与 admitted Presenter，读取真实 composition/presentation 时间戳；表内为各块 8 个保留样本的 composition GPU 时间中位数，单位 μs。

| 进程 | Clear 块 1 | 候选 Load 块 2 | 候选 Load 块 3 | Clear 块 4 |
| --- | ---: | ---: | ---: | ---: |
| final-v3 | 116.224 | 115.200 | 115.200 | 116.736 |
| cost-repeat-v1 | 113.664 | 117.760 | 114.688 | 114.176 |

变化方向未稳定复现，不足以证明 Load 更快，因此保留原策略。CPU wall 同时记录但包含编码、上传、提交、映射及刻意等待 GPU 完成，不能替代 GPU 时间，也不能宣称交互帧率收益。

计时间隔内不做图像 readback，每帧只读 32 字节时间戳；四个块之后各自做完整 RGBA 比较，均与 Clear 参考一致。GPU 时间覆盖离屏合成，单独记录 Presenter 的呈现间隔；并非显存物理带宽、独占 GPU busy 时间、原生窗口 Present 或手机 tile 实测。没有采集硬件带宽计数器；无 tile GPU 证据，不能关闭 #63 的该项要求。

## 验证与原始失败

最终 2458 个源码文件在每个阶段运行前后完全一致，并与提交前源码 SHA-256 一致：

- 新增真实策略矩阵与成本采样 2 项通过，0 忽略；独立进程另重跑成本测试 1 项，计入重复验证，不增加不同测试数。
- 原附件诊断 4 项、scratch 规划回归 32 项、SDK 完整 lib 37 项、预算/缓存/scratch 集成 8 项通过，0 忽略。
- 合计 **83 项不同测试通过**，另 1 项重复成本采样通过；非测试示例构建与执行成功。未声称本机跑完全部 workspace 或替代跨平台 CI。
- 普通构建仍有两个既有 dead_code 警告：`Load::Preserve` 与 `create_owned_texture`；本批未修改它们，原始 stderr 保留。

原始失败全部保留，不通过更改生产约束掩盖：

1. matrix-v1：新夹具误用不存在的 Layer 字段，编译失败；改为真实 `blend.mode` / `Content::Adjustment` API。
2. matrix-v2：新夹具错误要求数字 pass 顺序；实际 matte 源先编码。更正夹具的比较对象为真实 Clear 顺序，同时独立严格检查每个 pass 恰好一次、各策略完整顺序一致；未改任何已有测试断言或生产调度。
3. final-v2：新 Kill 包错误把 shader 放在根目录，被正式 schema 拒绝；移动到 `shaders/` 后通过相同打包/校验流程，没有放宽 schema。

## 证据与剩余工作

本地目录：`artifacts/issue-goal-current/attachment-pass-intents/`。`policy-summary.json` 保存逐阶段终态、设备、覆盖数量和两个进程的块中位数；`verify_policy.py` 核对源码冻结、完整像素 hash、实际编码、负对照、计时原记录、普通构建及原始失败。

`attachment-policy-evidence.zip` 共 65 个成员、3,137,285 字节，已核对 CRC 与逐成员 SHA-256；包含原始 stdout/stderr、命令、白名单环境、源码 hash、各失败和最终改动源码快照，不含 exe、DLL、GPU 驱动缓存或外部图像。

归档 SHA-256：`eee69dacf7cbdc6e8bb26268e889cff8fd562aa8a3756db341f52f917a0a0fd6`。

#63 保持打开：需要扩大精度/格式与外部消费者验证，证明候选策略的完整 admission，并测外部带宽及 tile GPU。当前计时不支持启用 Load；以后若没有可靠收益，继续保留原策略。不为补证恢复 Android CI。
