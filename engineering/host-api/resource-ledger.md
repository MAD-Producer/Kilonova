# 纹理资源台账

这是 issue #60 的应用纹理归属基础，不是整帧预算分配器或设备性能验收

## API

通过现有桌面/Android 插件 JSON 桥查询 `{"op":"resource_ledger"}`，或调用 Rust `motion_host::ops::preview::resource_ledger(session_id)`。两者只读，不提交 GPU 工作，不修改工程、撤销栈或 revision

返回 version、scope、knownBytes、unknownCapacityCount、resources、graphicsAvailable、editorRendererAvailable、driverResidentBytes、gpuRetirementBytes 和 excludes。resources 每项包含 id、尺寸、format、mipLevels、samples、logicalBytes 与 references（category、owner、state）

会话结果合并主预览渲染器、前台/候选目标与插件编辑渲染器；资源按分配 ID 去重。无图形上下文时 resources 为空，并由 Available 标记说明范围。通常的 preview_info 仅增加 resourceLedgerVersion，完整资源列表按显式查询生成，避免播放快照反复枚举和序列化每个资源

Rust 侧可以用 TextureResourceLedger::merge 合并独立预览和捕获目标的元数据，合并不保留 GPU 句柄

## 范围

每次 motion-render 纹理分配获得进程内唯一 ID。更新同一纹理内容保留 ID；重新分配尺寸获得新 ID。ID 以字符串输出，跨进程、工程文件和设备不具有持久化含义

台账仅保存元数据，不复制纹理、View 或 BindGroup，不延长资源生命。相同分配的多个应用引用按 ID 合并，容量只计一次。引用描述的是应用归属，不是驱动的引用计数，也不是纹理采样次数

容量从纹理描述符计算，包含格式块尺寸、mip、数组/3D 深度和 MSAA 样本数。未知格式容量返回 null，单独计入 unknownCapacityCount，不能把未知当成零显存

覆盖渲染器现有 16 类纹理及预览前台/候选目标。独立素材预览与捕获目标提供各自的 Rust 台账入口；它们不自动包含在会话预览的统计里

## 生命周期

- retained 表示资源池或素材缓存持有的纹理，不能据此判断当前帧是否采样它
- previewFront 表示预览目标当前前台槽，valid 状态仍由预览接口单独报告
- previewCandidate 表示候选/备用槽，包括提交后换下来的旧前台槽，不表示所有候选都正在生成
- commit_submitted 交换两个槽的归属，不改变分配 ID，也不宣称 GPU 已完成
- 丢弃候选、清空工程或替换纹理后，下一次快照移除对应的应用归属

应用归属消失不等于驱动已经释放。GPU 命令、View、BindGroup 仍可能保留原分配；当前尚无提交完成后的退休队列台账，因此 gpuRetirementBytes 和 driverResidentBytes 明确为 null。后续整帧预算必须先补上这部分生命周期，不能把当前快照用作 GPU 物理峰值或分配准入保证

## 后续验收

目前 GPU 测试检查既有分类总量与逐资源台账一致、共享引用去重、ID 复用/替换、失败请求保留台账、工程清空和预览候选交换/丢弃

尚未交付 GPU 退休队列、所有 GPU 缓冲/上传暂存/交换链的统一台账、外部解码分配、CPU 缓存归属、P50/P95/P99 基线，以及 Adreno/Mali 真机验收。#60 保持开启
