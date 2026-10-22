# 视频帧原生错误身份

关联 #138、#135。开工前检查了开放 PR、主线和本地工作树；#408 已合入，#410 处理时间轴查询，其他工作树负责图层纹理预算和效果库。这里仅修改宿主视频帧结果类型、冻结读取原因及必要调用，不修改视觉、像素混合算法、预算规则、格式或新增依赖/资产。

## VFE-01：真实原实现对照

三个公开宿主 API 回归在原生产实现实际运行，0 通过、3 失败，均为原生错误身份被 String 擦除：真实 headless GPU 的 decoded/prepared 上传预算拒绝，以及直接公开字段构造的非法 YUV 平面转换。见 video-frame-errors-before.log。预算通过公开 AllocationBudget 设置为现有已准入量，未模拟 GPU 成功，也没有修改生产预算算法。

首轮新测试误将数组传给 Vec 字段，编译错误保留于 video-frame-errors-fixture-compile.log；仅修正新夹具类型后执行原实现对照，不放宽断言、期限或忽略。

## VFE-02：原生边界与行为

DecodedFrame 的上传、编码上传和 RGBA 转换，以及 PreparedVideoFrame 的上传和显式 CPU 解码使用宿主 Result，原 RenderError 进入 Error::Render。预算 ResourceRejection 与原 YUV Invalid 不转成文本；原本宿主验证消息仍属于 Message，不从显示前缀猜类型。冻结读取 BlendRead 保留 handle/object/sequence 并将原生原因放入 source，继续原显示文本。

正常 RGBA 借用不分配，双帧线性混合及缓存/上传顺序保持原样。拒绝后预算占用不增加，恢复预算后同一帧上传成功。Android 探测调用将宿主原因放入既有 Media::Backend，未在维护范围之外声称 Android 目标或设备验收。桌面导出供应方仍有明确 String 协议，其转换发生在既有边界，不声称整个导出链已原生化。

## VFE-03：证据分类

- 新功能修复：上述五个宿主入口保留原生错误；冻结读取 source 保留原因。没有新增重试、超时、线程、日志计数或降级策略。
- 实际失败与恢复：三个原实现对照已失败，修复后三项通过。公开真实 GPU 拒绝和恢复、非法 YUV 保持原有错误、健康 RGBA 借用及混合像素 oracle 均有断言。
- 私有状态边界：冻结读取用例直接注入非法 prepared 存储，再调用实际公开读取入口，断言请求身份、原始 source、模型/序列/时间不变和同一 reader 恢复；不冒充真实 codec 或正常 GUI 崩溃复现。
- 默认日志：新增桌面用例使用实际 GPU 和模型帧错误，经生产 respond 与真实 mpsc 回复，检查默认 Info 文件、Error stderr 和 stdout 隔离。Flight 是手动测试上下文，不是完整 Engine worker 调度或 start/in-flight 的新增验证。最终执行及原始输出待后续记录。
- 既有覆盖：PreviewVideoCache 已返回原生 RenderError，本批未改；冻结 reader 的容量、取消、句柄、锁与析构约定保持。正常缓存缺失不新增警告。
- 保留边界：直接 Track 采样、图片 CPU 子模块文本结果、旧桌面响应与导出供应方 String 协议、永久 native 阻塞均不由本批完成。合集继续开放。

没有面板绘制、菜单、样式令牌或命中区修改，不新增可见 UI 或截图验收。

## VFE-04：日志夹具的原始失败与环境边界

首次桌面日志门禁失败：本机默认枚举 Vulkan 时，失效的 Epic Overlay JSON 登记由 wgpu 额外写入四条 Error，实际总数为六，见 video-frame-errors-vulkan-fixture-failure.log、-results.json、-source-receipt.json。目标两条拒绝及其源身份、恢复、回复断言已经成功；错误来自新夹具假设“原生环境不会另报 Error”，不是产品掩盖了该登记。

不修改生产 backend 选择、日志级别、过滤、错误返回、旧断言或期限。仅让新受控子进程显式使用平台原生 backend：Windows DX12（子进程断言实际 adapter），Linux Vulkan，macOS Metal；仍严格要求默认 Info 文件及 Error stderr 各两条完整错误，stdout 无日志。Windows 默认输出的最终证据只覆盖 DX12，不能据此宣称 Vulkan Overlay 配置已修复；原始错误全部保留。公开宿主 GPU 对照仍使用常规 headless 选择，不将两种验证范围混为一谈。


## VFE-05：最终组合验证

基于 #410 衔接 #408/#409 主线后的 4fbce54e，纳入时间轴原生查询与本批视频帧边界，最终完整宿主/渲染器、桌面失败信道及 FFmpeg adapter 共 605 项通过、0 失败、1 项忽略，带 FFmpeg 产品构建成功；preflight 的 1 项已包含在桌面信道，不重复计数。五阶段 Rust/Cargo 源哈希不变，见 video-frame-errors-final-results.json、-source-receipt.json 及原始各阶段日志。

独立默认输出保存于 video-frame-errors-default-final-output：实际 DX12 预算拒绝及 YUV 转换错误经生产回复，在 Info 文件和 Error stderr 各两条，stdout 隔离；实际 backend 在子进程断言。仍保留首轮数组夹具编译错误、三个原实现身份丢失失败、Vulkan 环境四条附加错误导致的夹具失败。仅补齐新夹具类型和指定平台原生 backend，不修改生产过滤、返回、预算、旧断言、期限或忽略。冻结 reader 的 source、不变性及恢复用例包含于最终完整宿主结果。

与时间轴查询修复合并在同一个 #410，减少同类边界拆分的 CI。此前 47dc0922 的完整 1200 项结果仍对应原时间轴提交，不与本轮重复累加，也不称为最终八文件增量的完整桌面回归。最终选定回归包含真实时间轴默认信道和最新纹理预算用例，但不冒称整个 UI 已做视觉验收。来源守卫、样式消费者和差异空白检查通过。Android 仅同步调用，未执行目标/设备验收。

合并过程中主线另有 #411 的八个蒙版纹理文件，已检查与本批 47 个文件无重叠；它们不在本轮本地树中，最终 head 的远端组合 CI 仍须独立完成。

唯一忽略是既有 optional_local_original_image_probe，需要仓库外的 MOTION_IMAGE_SOURCE；本批没有新增忽略。
