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

覆盖渲染器现有 16 类纹理及预览前台/候选目标。会话查询也包括插件编辑器持有的捕获目标；独立素材预览和其他调用方的捕获目标仍需通过各自的 Rust 台账入口合并

## 提交生命周期诊断

`{"op":"resource_tracking","enabled":true}` 开启计量，false 停止采集新提交。Rust 入口为 `motion_host::ops::preview::resource_tracking`。这是运行时诊断开关，不修改工程、撤销或 revision；通常的 resource_ledger 查询仍保持只读

start_profiling 自动开启计量，stop_profiling 写出 resourceLedger 后恢复显式开关的状态。新建主预览与插件编辑渲染器继承当前状态。默认关闭时 submit_tracked 不枚举纹理，也不调用目标收集闭包；关闭后已经登记的提交仍等完成通知才释放台账

每个被 instrument 的提交记录当时渲染器全部持有纹理以及调用方声明的目标。该集合可能包含本次未采样的闲置资源，因此是已知纹理的保守容量上界，不是精确 GPU 访问集合。快照只持有元数据，不延长实际纹理的生命

resource_ledger 的 submissionTracking 返回主预览和插件编辑渲染器的 enabled、submitted、completed、pendingSubmissions、pendingUntracked、skippedSnapshots、maxSnapshots、knownPendingBytes、peakKnownPendingBytes、knownRetiredBytes、unknownCapacityCount

- pending 的容量按分配 ID 去重，两个未完成提交引用同一纹理只计一次；peakKnownPendingBytes 保留本渲染器累计观测到的已知提交容量高水位，超限缺口和排除项仍然适用
- knownRetiredBytes 是未完成快照中已不在当前应用归属台账的容量，保留到最后一个登记提交的完成通知
- completed 表示 wgpu 队列完成通知，不能代替 shader 校验或设备健康检查，也不能等同于成功呈现
- 最多保留 8 份提交快照，超限记录 skippedSnapshots 与 pendingUntracked，不能把这些缺口当成零容量
- 查询不 poll、不等待 GPU；后台完成通知只更新元数据，各调用方仍按既有方式驱动 device/queue poll

主预览（含保留画面呈现）、Renderer 的直接绘制/捕获/读回以及 Presenter::draw 已接入。原始 queue.submit、独立视频转换、上传暂存、交换链、UI/节点预览自有资源、已解绑的渲染上下文及仅被 View/BindGroup 保留的引用不在此范围。gpuRetirementBytes 与 driverResidentBytes 继续为 null，不能用 knownRetiredBytes 替代全量物理退休显存

启用会产生纹理枚举和元数据快照开销。性能基线对照须采用相同计量开关，不能把仪器本身的 CPU 成本当成无开销

## 生命周期

- retained 表示资源池或素材缓存持有的纹理，不能据此判断当前帧是否采样它
- previewFront 表示预览目标当前前台槽，valid 状态仍由预览接口单独报告
- previewCandidate 表示候选/备用槽，包括提交后换下来的旧前台槽，不表示所有候选都正在生成
- commit_submitted 交换两个槽的归属，不改变分配 ID，也不宣称 GPU 已完成
- 丢弃候选、清空工程或替换纹理后，下一次快照移除对应的应用归属

应用归属消失不等于驱动已经释放。GPU 命令、View、BindGroup 仍可能保留原分配；当前提交台账仅覆盖上述计量路径，因此 gpuRetirementBytes 和 driverResidentBytes 明确为 null。后续整帧预算必须补齐未跟踪的生命周期，不能把当前快照用作 GPU 物理峰值或分配准入保证

## 后续验收

目前 GPU 测试检查既有分类总量与逐资源台账一致、共享引用去重、ID 复用/替换、失败请求保留台账、工程清空和预览候选交换/丢弃

提交计量测试补充共享资源在最后一个提交完成前保留、逆序完成、禁用不丢弃待完成记录、8 快照上限与缺口、实际队列完成回调、失败编码不登记、捕获像素一致及诊断开关与 profiling 的协作

尚未交付全量 GPU 退休分配器、所有 GPU 缓冲/上传暂存/交换链的统一台账、外部解码分配、CPU 缓存归属、P50/P95/P99 基线，以及 Adreno/Mali 真机验收。#60 保持开启
