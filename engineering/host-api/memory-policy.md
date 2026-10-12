# 会话纹理预算 API

本接口配置应用允许使用的纹理容量，不查询或声明显卡实际可用显存。配置属于会话运行时状态，不写入工程、不增加撤销步。

Rust 入口为 `motion_host::ops::preview::configure_memory_policy(id, MemoryPolicy)`。已有 Android/桌面插件 JSON 通道也支持以下请求：

```json
{"op":"memory_policy"}
```

查询返回 `policyVersion: 2` 与 `memoryPolicy`。更新时提供完整配置：

```json
{"op":"memory_policy","policy":{"sourceTextureBudgetBytes":268435456,"scratchBudgetBytes":100663296}}
```

`sourceTextureBudgetBytes` 范围为 4 字节至 2 GiB，默认 128 MiB；`scratchBudgetBytes` 范围为 64 至 384 MiB，默认 64 MiB。未知字段、缺失字段和越界值会拒绝整个更新。原 `configure_memory(totalMem, guarded)` 接口仍按原系统内存规则选择 scratch 预算，并保留显式配置的源纹理预算；系统内存数值不视为 GPU 显存容量。

同一源纹理上限用于图片、视频平面、插件资源、矢量及其光栅工作集、蒙版输出、图层快照、嵌套合成与合成中间纹理。scratch 继续使用独立上限。预算降低后，缓存也接受重新检查；超限返回带 `requiredBytes`、`budgetBytes` 和资源类别的既有类型化拒绝。预览可继续按已有有限降级策略重试并保留旧画面。

新建预览/插件编辑器渲染器以及完整质量 PNG 输出继承会话配置。桌面视频导出计划冻结捕获时的完整配置，之后的预览设置修改不会影响该任务。导出不隐式缩小源图片或降低效果分辨率。

解码文件、包大小、设备纹理尺寸等独立安全限制继续有效。本阶段不包含 GPU 可用预算查询、所有资源的统一预留分配器或整帧峰值预算；预览目标、交换链、CPU 缓存、缓冲区、UI 与独立节点预览等仍按现有策略管理。
