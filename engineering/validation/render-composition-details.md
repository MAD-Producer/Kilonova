# 渲染包装后的原生合成详情（本地完成，远端组合待验收）

继续本人 #138、#135。在 #439 CI 运行期间复用同一空闲工作目录，分支 `fix/render-composition-error-details`，父提交 `1631186c`。开工前只检查分支路径元数据和各工作目录的候选文件未提交状态。`crates/motion-host/src/error.rs` 没有活跃未提交改动，其当前 blob `1c9c654e` 与实际 main 一致；其他分支最后改动来自 10 月 21 日旧错误定义，当前文件已包含后续改动。已有 `failure_boundaries.rs` 存在资源分支交叠，因此不编辑它，新测试单独放在 `tests/render_composition_detail.rs`。没有审阅别人的 PR。

| ID | 分类 | 改变及边界 |
| --- | --- | --- |
| H1 | 功能修正 | Host 的 `composition_detail()` 沿 RenderError 的 Context、Diagnostic、Shared、CompositionPlan::Encoding 寻找实际 Model::Composition，返回原对象的借用；不解析文字，不克隆原因 |
| H2 | 协议保持 | `ok=false` 和错误 Display 不变；已有 `error_detail` 字段从 null 恢复为原生合成详情。没有新增 UI、日志、错误结构、重试或调度改动 |
| H3 | 实际拒绝与原因释放 | 真实 Project::composition 缺失输入产生原生错误；组合渲染包装与宿主 Context 后验证指针身份、60 次借用回复不增持、dispatch 消费后的释放 |
| H4 | 首要原因与负面对照 | Cleanup 只跟随 primary；真实 File::open 的 NotFound 为主而合成错误为 secondary 时，详情仍为空。伪造 composition_error 文字、普通 Invalid/Readback/模型 Missing、非 Encoding 的计划错误不能提升为合成详情 |
| H5 | 既有可观测性边界 | 没有新增日志或计数点。本批只修正结构化回复的类型遍历，不冒充新 worker 阻塞/默认信道验收；#439 已有真实曝光 IO 的默认文件、Error stderr、mpsc 和 stdout 隔离证据，仍不代表本批的组合通过 |
| H6 | 待完成范围 | 不改原生 RenderError::source 包装、资源拒绝、ExposureScenes/provider 更早的文字边界、其他宿主缓存或合集剩余事项。#138、#135 保持开放 |

三项新增测试先在旧生产实现运行，实际 1 通过、2 失败、0 忽略：原生包装与原生 Cleanup 主因均返回 null；文字伪造负面对照通过。原始日志 SHA-256 `5be35c194054287351ec89f973a342dfaedf041b54fd122325c5951c39a9c36b` 保留，没有更改原有断言。

最终冻结的 Rust/Cargo/WGSL/YAML 输入记录于 `artifacts/pr-ci-followups/host-render-detail-final-source-receipt.json`。原边界与新增专项 13 项通过；随后完整宿主 123 项通过，已包含这 13 项，不相加为 136 项；两个阶段均 0 失败、0 忽略。专项日志 SHA-256 `a6dfad2471e1f8bfa3d56f1411456dd3ae608cc57713873914d85f2aac7b75c8`，完整宿主日志 SHA-256 `61cd611151e5a63a16f37d637398199c09bf6318e01f2a58a3937d9bbbbabb1a`。实际回复的字段、文字、原对象身份和释放由失败输入断言验证，不以绿色计数代替边界验证。

全工作区全目标 FFmpeg/diagnostics 检查成功，日志 SHA-256 `bdefb993fb2ff1f890a3c647ebf16201d8f4935feaa758beafd60707a872dad5`。使用原共享 target，没有新增构建目录。已有 dead_code 等警告原样保留；未执行 cargo fix，也未为消除警告改写其他模块。本批没有重新构建可分发程序，不把上一批的启动结果冒充本批产品启动验证。

生产改动只有 getter 的 17 行新增，返回借用不增加文件 IO 或日志。当前验收基于 #439 head，不代表其他资源分支与最新 main 的组合已经通过。远端堆叠 PR 和最终合入 main 仍需独立验证，不提前关闭两个合集。

本记录对应 #440 的首个提交 `9ff10092`。随后同一 PR 合批修正节点运行时恢复的原生错误，不另开 PR，不改写本提交。合批后 getter 与三项原生详情测试逐字节不变，完整宿主 126 项及桌面信道 7 项通过，共 133 项、0 失败、0 忽略，并完成全目标检查、FFmpeg 新产品构建和实际命令列表启动；其中已包含本记录的专项，不累计前批数字。最终冻结输入、实际 worker 信道与新产品证据见 `engineering/validation/node-runtime-native-errors.md`。最终 PR head 和实际 main 仍需各自验收。
