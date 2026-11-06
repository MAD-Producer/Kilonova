# 查看器截图与快照保存的原生错误

关联 #138，接续桌面错误信道类型化。稳定源码上的真实 worker 故障测试和三项既有
截图回归均已通过；远端当前提交 CI 尚待执行。本项不关闭 #138。

## 变更

快照捕获与保存改为返回已有 `motion_host::Result`。捕获保留宿主/渲染错误，
保存各阶段使用已有 `Error::with_context`，保留原阶段文案与原生 RenderError / ImageError。
UI 截图的 Presenter 创建、通道设置、编码也保留原生 RenderError。
不新增依赖、资产、日志热路径、重试、预算调整或用户可见布局。

开始前核对 open PR 及全部工作树中这两个生产文件的未提交修改；旧草稿保持原样，
没有活跃的同项错误转换修复。本分支以桌面错误信道的主线整合源码为基础，
在独立工作树开发，不修改正在运行全量回归的源文件。

| ID | 处理与验收范围 | 状态 |
| --- | --- | --- |
| VC-CAPTURE | 查看器复制图像的原生宿主/渲染错误保持类型；失败不消费快照 generation。 | 真实 worker 预算拒绝通过；恢复后 generation 从 1 递增至 2。 |
| VC-SAVE-TARGET | 保存 PNG 的目标分配、色彩输出、编码、提交、读取保留 RenderError 与阶段。 | 已实现；新测试用真实预算拒绝覆盖目标分配，其他阶段不冒称全部故障注入。 |
| VC-SAVE-IO | 保存路径被同名目录占用时保留 ImageError::IoError 与 OS cause。 | 实际文件系统拒绝通过，检查 OS 错误码、原目录保留和恢复保存。 |
| VC-PRESENTER | UI 截图 Presenter 的三个原生 RenderError 转换不再变成字符串。 | 稳定源码编译及既有截图、精度、多视图回归通过；不冒称已注入每个 Presenter 故障。 |
| VC-CHANNELS | 沿用 worker 的 Info 开始和 Error 文件/stderr 输出；stdout 隔离。 | 默认 Info 下三个真实错误均检查请求、操作、开始顺序、两路错误和 stdout 隔离；恢复后的像素/工程不变。 |
| VC-TEXT | configure_capture 的剩余 String 是现有验证提供方边界；UI Painter、atlas、字体等仍有未迁移位置。 | 保留并继续跟踪 #138，不宣称全应用迁移完成。 |

## 执行边界

新用例以合法最小 1 字节预算触发真实分配拒绝，保持生产的 1 byte..4 GiB 边界。
最初草案的 0 字节在读到接口约束后更正，没有改动生产预算；首轮构建期间发生该修订，
因此只保留为构建过程记录，不作为稳定源码验收。随后稳定源码重新构建，四项实际
测试均成功；各阶段源码清单相同，测试可执行文件 SHA-256 在各次执行后均保持一致。

## 本地运行结果

Windows 原生窗口与 GPU，开启 FFmpeg，未设置 `MOTION_LOG`。新测试及以下三个原有
回归各 1 passed、0 failed、0 ignored：

- `engine::viewer_capture_error_tests::native_viewer_capture_and_save_failures_keep_types_default_sinks_and_recover`
- `engine::capture_context_tests::`
- `engine::precision_capture_tests::`
- `engine::viewer_frame::native::inspection::`

新测试实际执行两次预算拒绝和一次文件系统拒绝，拒绝保存不创建文件，失败捕获不覆盖
已有快照。恢复 PNG 与原 PNG 像素完全相同，且显式验证 16×16 全绿像素，工程和 revision
保持不变。同文案字符串不能伪造原生错误的对照是附加检查，并未替代上述实际错误。

证据包 `viewer-capture-errors-evidence.zip` 保留首轮非稳定源码构建、最终稳定编译、
四项执行的原始输出/源码哈希、默认日志/子进程输出及前后 PNG。归档中逐成员 SHA-256
清单已逐成员回读验证。归档共 44 个成员、1,841,269 字节，SHA-256：
`eaef4bdad05b2597ed6c9b1a439112467b088bd005c1edf7442dd21dab0b6c55`。

本项与诊断原生错误合批进入 #619，最终源码同一二进制又执行四项查看器回归，均通过；
见 [诊断合批验收](diagnostics-native-errors-current.md)。剩余边界为最终提交 CI，以及
表中 VC-TEXT 的提供方迁移。没有改变原有测试的
断言、忽略或期限，也未改变生产预算、错误返回行为或重试策略。
