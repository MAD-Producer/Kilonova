# 合成帧打包：分配与错误边界验收

基线：`44a5a0d`。本批将 #126 的帧级临时分配与 #138 的 `composition_plan` 错误边界合并处理，不关闭两个聚合 issue。开始前核对了本地 worktree 和开放 PR；`perf/effect-input-order-allocations` 正在修改 `effect_plan.rs`，本批不修改该文件，也不处理别人负责的 PR 队列、CI 或 UI。

## 条目处置

| 条目 | 类别 | 结果与证据 | 边界 |
| --- | --- | --- | --- |
| B126-01：每个父合成临时收集子节点 | 新的性能改动 | 在后序节点表中保存兄弟索引，再逐个回填直接父节点；每条链接只走一次。7 节点、8 视频实例的预热样例中，总分配由每次 16 次降到 9 次 | 节点表仍按每次调用分配；每个节点新增一个 `usize` 索引，并非零分配或整帧延迟保证 |
| B126-02：每个合成临时收集视频列表 | 新的性能改动 | 先计数，再直接遍历层写入视频记录；没有临时视频 `Vec` | 多一次线性计数遍历，未改变计划和资产解析策略 |
| B138-01：打包错误统一到 `RenderError` | 新的类型化边界 | 限制、丢失引用、丢失视频资产与写入校验由 `CompositionPlanError` 携带；嵌入 `RenderError`。合成诊断用 `Context` 保留原始 `Box<RenderError>`；`source()` 与 `resource_rejection()` 可检查同一个预算拒绝对象 | `effect_plan` 内部仍有 String 与分类器；宿主 `Result` 仍为 String，仅在已有宿主协议边界显式转为文本，未完成 #138 全部迁移 |
| B138-02：丢失采样引用时 `expect` panic | 功能修复 | 返回包含合成 ID、引用图层 ID 的类型化错误；没有日志替代错误、吞掉 panic 或自动重试。修正输入后同一构建器可以恢复 | 对外可变 `Scene` 的损坏输入验证；没有声称正常项目已复现该状态 |
| B138-03：宿主打包失败可诊断 | 新的观察 | `frame_bundle` 进入会话之前记录 Info start，返回后 Debug done / Error failure。失败带 session、合成、帧、阶段、耗时、原始显示原因；日志发生在 `with_session` 返回、释放锁后 | 直接宿主函数没有新增工作线程或重启机制；调用方既有 worker 的在途状态职责不变。未修改 Present 热路径 |
| B-ABI：成功打包与旧协议 | 保留并验证 | 测试专用旧写入器来自同仓基线；混合帧率、同源多引用、分数帧、随机回退时逐字节一致，并独立检查父节点、纹理槽、视频 PTS、旋转后的显示尺寸、保留位 | MAGIC、VERSION=1、32/80/40 字节记录不变；失败输出仍可能不完整，调用者必须丢弃，不改变既有失败语义 |
| B-NORMAL：正常与资源不足分支 | 既有边界 | 有效输入继续成功；深度/实例/32 MiB 上限的历史文本不变；预算不足有结构化 scope/required/budget，恢复后没有遗留拒绝状态 | 正常缓存命中、取消、空撤销、媒体准备语义均未改动；没有新降级计数器或阈值 |

## 验证

本机 Windows，原生 MSVC、锁定离线依赖，debug assertions 开启；使用现有 FFmpeg 共享运行时。最终命令与完整输出保存在工作区 `artifacts/composition-bundle-boundaries/`。

- `render-final.log`：新打包契约 6 项，以及 `composition_frame_boundaries` 2、`compositions` 5、`instance_aliases` 3、`multiple_camera_compositions` 3、`resource_rejections` 3，共 22 项通过。包含真实 GPU 像素对照和资源恢复。`render-restored.log` 是负对照还原后的新契约 6 项复验，不重复计数。
- `render-resource-unit.log`：已有嵌套错误与资源压力恢复单元测试 2 项通过。
- `logging-final.log`：带 FFmpeg 的桌面日志测试 11 项通过，包含真实宿主 scratch 预算失败、增加合法预算后恢复、超长上下文拒绝。
- `native-output.json`、`native-*/logs/motion-studio.log`、`stderr.txt`、`stdout.txt`：移除 `MOTION_LOG` 后，2 次实际错误分别在文件/stderr/stdout 中记录 2/2/0 次；3 次 start 为 3/0/0 次。默认级别没有 Debug done，8192 字符输入上下文截为 128 字符；错误原因上限 2048 字符。该子进程复验不另计测试数。
- `mutant-video-vector.log`：暂时恢复视频临时集合后分配变为 13 次，性能契约失败，退出码 101；`mutant-flattened-source.log`：仅保留显示文本、丢弃原始拒绝类型后，类型化契约失败，退出码 101。`mutants.json` 记录还原时的 SHA-256；两种改动均已还原。现有测试断言未改动。
- `host.log`：`motion-host --features diagnostics` 的完整单元、集成与文档测试 86 项通过。以上独立用例合计 121 项；还原复验与原生日志子进程不重复计数，见 `test-summary.json`。
- `workspace-check.jsonl` / `.stderr`：`cargo check --workspace --exclude motion-android --all-targets --features motion-host/diagnostics --locked --offline -j1 --message-format=json`，最终 `build-finished success=true`。
- `build.log`：`cargo build -p motion-desktop --features ffmpeg --bin motion-studio --locked --offline -j1` 成功，29.81 秒，21 条已有警告。只证明当前分支可编译，不代表其他未合并 PR 的集成结果。
- 新增/改写的小型 Rust 文件通过 rustfmt 检查；差异空白、provenance 与桌面样式消费守卫通过。没有全量格式化已有的大型文件，也没有用编译结果替代上面的实际失败与输出验证。

新增测试初次运行的夹具 ID、项目目录和 boxed source 下转型假设错误保留在 `focused.log`、`focused-corrected.log`、`render-regression.log`、`logging.log`；这些失败不作为通过证据，修正仅涉及新增测试。

## 剩余范围

#126 的矢量顶点、分组 uniform、蒙版 bind group、资产集合、presenter 等站点仍需按所有权分批处理；这里没有声称它们已修复。#138 的宿主整体类型迁移、完整效果计划迁移及其余审计条目不在本批闭环内。没有引入依赖、第三方资产或用户可见 UI 改动；Android 已退出当前桌面验证范围，未运行其构建。真实原生日志证据仅限上述打包调用，不替代其他 worker 的受控阻塞、崩溃或持续降级验收。
