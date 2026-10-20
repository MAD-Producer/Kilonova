# 媒体任务生命周期与中毒级联修复

## 范围与分支协调

本 PR 将 #131 第六节的音视频及包暂存资源释放，以及 #130 的媒体任务锁中毒级联、包 worker 发布故障放在同一批修复中。基线为 main `a2bffcf8a4c4f4dd8cf8f4accaa2d88a26a015e7`，分支为 `fix/media-teardown-cleanup`。开始前只读检查开放 PR 文件列表、相关本地分支提交和所有 worktree 在目标文件上的改动。只更新本批自己的 #381，不处理其他人的 PR 合并或 CI 调度。

扩展包清理前再次核对目标文件：没有相关未提交工作树改动。#380 也修改 `core/storage.rs`，但只复用导入校验中已验证的父路径；本批只修改 `PreparedPackage` 发布方法与能力实现，行段互不重叠。视频回收的 renderer 和 decoder 文件仍有其他分支改动，本批避开这些文件，不重复实现 #68 已有的 active-set 回收。

缓存文件对已经有备份/回滚；#359 已处理成功探测结果的暂存清理。这些是既有覆盖，不能重复报告为新增修复。聚合 issue #131、#130 仍有其他事项，本批不关闭它们。

## 功能修改与协议边界

- `AudioJobs::drop`、`VideoJobs::drop` 在锁内设置取消标记并取出资源，解锁后析构 `Prepared`。
- `cancel` 保持取消标记、取出资源、写入取消终态共用同一把锁，释放锁后清理，方法仍在清理返回后才返回。并发状态查询现在可以在清理期间看到取消终态；原有取消未完成 worker 的路径本就可能在终态之后继续清理。取消和提交仍互斥，后来的提交只能读取终态，不能发布图层。
- 两个 `commit_inner` 把资源所有者声明在任务锁之前，使正常返回和 panic 展开都先释放锁，再析构暂存树。发布文件、修改模型与回滚仍在原有取消锁内，元数据、错误与单步历史语义不变。
- 音频缓存完成提取为生产 worker 实际调用的 `finish_cache`，通过借用捕获结果发布缓存，避免闭包提前消费并析构 `Stage`。捕获的错误和 panic 原因继续进入原有完成状态和日志。
- `PublicationCleanup` 在业务锁之前声明，保留缓存备份直至解锁。文件发布及回滚同步执行，只有备份删除与清理日志延后。内部 `Attempt` 确保两次移动之间发生 panic 时，回滚仍先于解锁完成，不把回滚移出取消保护范围。
- 音视频预留任务时改用可失败的活动任务计数。已有任务锁中毒时返回原有 `audio/video task poisoned` 错误，不再次 panic，不创建新任务、不启动解码器、不清除中毒状态。

- 包发布能力改为 `PackagePublication::publish(&mut self)`，仍在取消锁内执行一次原子发布，但资源所有权留在锁外，随后关闭/删除暂存文件。工作区内的生产和测试提供者都适配了该能力；具体 `PreparedPackage::publish(self)` 的调用接口保留。
- 包发布结果确定后，状态保持 `running/finalizing`，清理完成才公布终态。原本在发布期间发起的取消仍需等待终态确认，改用每任务条件变量等待并释放任务锁，不忙等；状态查询、release 拒绝与后续任务准入检查仍可使用。已开始发布的决定不被迟到取消或注册表销毁覆盖，正在清理的任务仍占原有配额。
- 发布与资源析构分别捕获提供者 panic，避免发布 panic 后又在展开中析构 panic 导致进程中止。先前发布的原始错误和后续析构原因分别保留，终态通过现有错误日志信道报告。发布成功后析构失败会报告已发布文件路径，不删除发布结果。正常取消及取消后收到的准备错误保持原有状态语义；新出现的析构故障同时进入取消状态的 error 字段。

删除仍同步执行；本批解除任务锁占用，不宣称降低文件删除本身的耗时或让关闭立即返回。包状态新增 `finalizing` 阶段，未增加序列化字段；这是功能及能力接口调整，不能归为纯日志变更。若提供者清理始终不返回，任务仍停在该阶段并占用原有 worker 配额，不增加超时、重试或重启。

## 报错信道与边界审计

| ID | 分类 | 证据与处置 |
| --- | --- | --- |
| MT-01 音视频任务销毁 | 功能修复；既有观察移位 | 实际 OS 清理失败和阻塞宿主日志器期间，日志回调及另一线程都能获取任务锁；正常删除与空资源路径通过。 |
| MT-02 取消、提交失败、提交 panic、音频缓存完成 | 功能修复；既有观察移位 | 七条生产清理路径回归在修改前全部失败、修改后全部通过；panic 原因和锁中毒状态保留。完整媒体事务验证提交、保存、撤销及取消原有契约。 |
| MT-03 缓存发布/回滚 | 协议边界保留；既有观察延后 | 第二文件失败仍恢复旧文件；受控展开检查证明回滚发生在解锁之前。恢复失败保留旧字节和恢复目录，解锁后输出 Error。备份删除失败仍 Warn，保留外来目录，不删除它。 |
| MT-04 导入回滚的次要失败 | 既有观察合并并延后 | 主错误与所有回滚失败继续拼接返回；每次导入至多一次回滚、最多五个文件。原来每文件一条 Warn 改为每次回滚一条有界 Warn，解锁后输出。原有 Error 记录仍同时包含主因与次因。 |
| MT-05 中毒级联 | 功能修复；原有返回错误信道 | 两条回归修改前因 `unwrap(PoisonError)` 失败；修改后得到明确 Err，注册表未新增任务，其他健康实例仍可预留任务。真实桌面用例通过公开提交接口制造原始 panic，随后申请缓存得到 Err，没有第二次 panic。 |
| MT-06 正常分支与日志热路径 | 既有覆盖 | 取消、空暂存、正常缓存删除不是故障，不新增日志。未新增 start/done 记录、逐帧文件 IO、重试、重启或超时。既有 worker start 在可能阻塞的 handler 之前输出，原有相关回归继续通过。 |
| MT-07 保留范围 | 明确剩余工作 | 解码器退出/峰值、#131 其他边界及 #130 的其他线程/会话问题未完成。新建任务初始化及 spawn 失败处的私有任务锁 unwrap 仍保留：此前没有成功启动 worker，它们不是本批复现的已有任务中毒计数路径；未宣称完成全仓 unwrap 清理。 |
| MT-08 包发布与资源析构 | 功能修复；既有完成信道补齐 | 真实 ZIP 写入、文件发布失败和受控析构阻塞验证先解锁再清理；发布本身继续持有取消锁，取消等待时释放锁。成功、真实 rename 失败、发布 panic、清理 panic、两次独立 panic、取消清理均覆盖，清理完成才允许 release。 |
| MT-09 包故障主次原因 | 新增捕获；沿用原有日志器 | 分别捕获发布与析构故障，panic 文本沿用既有 2048 字符边界，无新增观察键、计数、逐帧日志或业务锁内显式日志。正常取消、NotFound 清理仍不是故障；原有取消错误状态断言保留。 |

新增延后容器每次事务最多持有一个缓存发布、一个导入回滚观察。缓存恢复错误与回滚警告各保留最多 2,048 字符；返回给原有错误信道的完整主次原因不被截断。没有新建无限键集合或跨帧计数器。已有锁中毒观察维持原先生命周期计数及重置规则，本次没有改变阈值。

## 验收与原始证据

证据均在 `E:/Dev/aem/artifacts/cache-publication-cleanup/`，不作为发布素材提交。

1. 上一阶段仅新增销毁回归：2 通过、2 失败，退出 101；修复后完整媒体测试为 91 通过、2 项原有忽略。日志 `teardown-before.log`、`teardown-complete.log`，不与最终测试数相加。
2. 本阶段先提取原有缓存完成逻辑供实际 worker 与测试共用，再运行未修改行为的清理回归：**0 通过、7 失败**，均观察到清理日志调用时占锁。`resources-before.log`；修改后同七条全部通过，`resources-after.log`。
3. 活动任务计数中毒回归：修改前 **0 通过、2 失败**，原始原因是已有任务的 `PoisonError` 再次触发 unwrap panic，`reservation-before.log`。修改后两条通过，包含在最终完整媒体回归内。
4. 负向对照临时把导入回滚 Warn 放回锁内，新的真实 OS 故障测试退出 **101**，`publication-lock-mutant.log`；原样恢复生产源码，SHA-256 `ccdf21858b3b99600b6f3e74d5fe7a04c54f54b7732305af6ba9df53d35b6abe`。这是对新观察顺序的变异验证，不冒充旧 main 的同 API 基线，也不修改测试断言。
5. 最终完整 `cargo test -p motion-media --locked -j 1 -- --test-threads=1`：**103 通过、0 失败、2 项既有忽略**，十二个摘要含 doctest，`lifecycle-final-media.log`。忽略项分别要求 release 大文件验收、外部大视频素材，本次没有运行。
6. 最终 `cargo test -p motion-desktop logging::media_commit_tests:: --locked -j 1 -- --test-threads=1`：**4 通过、0 失败**，`lifecycle-final-native.log`。19 条原有测试构建警告保留。
7. 实际默认 Info 子进程信道证据保留在 `lifecycle-native-output/`。取消/提交/panic 清理 Warn 的文件/stderr/stdout 数量为 **6/0/0**；原始提交 Error 为 **2/2/0**，原始 panic 原因为 **2/2/0**，后者 stderr 来自保留的原始 panic hook；没有第二次 `PoisonError` panic。另一个用例同时触发提供者保存失败和外部目录阻碍，两个主次原因组合各在文件与 stderr 出现一次，stdout 为零。子用例直接重跑仅用于保留原始输出，不额外计数。
8. `cargo test -p motion-model --test dependency_boundaries --locked -j 1`：**1 通过**，`lifecycle-boundaries.log`；新增的工作区测试依赖未破坏生产依赖边界。`cargo build -p motion-desktop --features ffmpeg --locked -j 1` 成功，17.84 秒，保留 21 条原有构建警告，`lifecycle-ffmpeg-build.log`。这是当前分支构建，不作为所有未合并 PR 的集成发行包。
9. 七个修改/新增且原本格式一致的测试与发布实现文件 `rustfmt --check`、差异空白、来源和桌面样式消费校验通过。未全量格式化原本有其他格式差异的 `jobs.rs`、`video.rs`；新增的缓存完成函数单独格式化，避免引入无关改动。

测试开发中曾修正新测试模块可见性和新测试错误类型的 Debug 约束，并纠正锁文件误加到 Android 条目的测试依赖边；首次编译失败分别保留在 `resources-test-compile.log`、`lifecycle-native-test-compile.log`、`lifecycle-native-lock-check.log`。最终锁文件只给桌面测试加入已有工作区 `motion-model`，以使用真实 `EditSink` 故障提供者；没有新增第三方包或版本，没有修改任何既有断言来通过验收。

使用已有便携 MSVC、锁定依赖、目标目录 `E:/Dev/aem/target/model-robustness-audit`，单构建任务、单测试线程。仅通过环境关闭调试符号和增量编译，调试断言仍启用。新增测试使用仓库已有素材及独立生成的故障文件，来源政策、ATTRIBUTION、NOTICE 已核对，不需要新增第三方素材记录。没有 UI、样式或文件格式变更；包能力与状态阶段的变化已单列，远程 CI 和跨平台验收仍由现有审核流程完成。

## 同批补齐包发布与析构边界

最终验收替代上文阶段性测试数，不与它们相加；证据仍在 `artifacts/cache-publication-cleanup/`。

1. 锁顺序负向对照将 `cleanup_package(result)` 临时移回任务锁内，真实 ZIP/rename 失败用例退出 **101**，观察结果为无法取得任务锁；原样恢复后通过，`package-lock-mutant.log`。该用例使用新借用式能力，不冒充旧 main 相同 API 的基线。
2. 七个新增包生命周期用例验证成功、真实目标目录阻碍 rename、发布 panic、发布失败后清理 panic、发布与清理分别 panic、发布成功后清理 panic、取消清理 panic。析构门闩实际阻塞时可查询 `running/finalizing`，release 与新任务准入仍被拒绝；并发 cancel 实际进入条件变量等待且释放任务锁，清理返回后得到相同终态。回收暂存文件和保留原有/已发布文件均使用真实存储提供者。
3. 最终完整 `cargo test -p motion-media --locked -j 1 -- --test-threads=1`：**110 通过、0 失败、2 项既有忽略**，`package-final-media.log`。未修改既有断言；原有发布中取消必须确认 succeeded 的断言促使实现使用条件变量保持确认语义，而不是提前返回 running。原有取消后准备错误仍不写入取消状态 error 的断言也保留，仅新的析构故障写入该字段。
4. `cargo test -p motion-core --test package_transactions -p motion-model --test dependency_boundaries --locked -j 1 -- --test-threads=1`：**5 通过**，`package-final-core.log`；具体存储的公开消费式发布、取消/失败清理与生产依赖边界保持有效。
5. `cargo test -p motion-desktop logging:: --locked -j 1 -- --test-threads=1`：**14 通过**，`package-final-native.log`。包含原有日志回归和六种包真实失败组合。正常的 ZIP 写入/清理、真实 rename 失败、原生清理 IO 失败、发布/清理提供者 panic 都经过实际 worker，不用成功模拟值代替。
6. 默认 Info 原始输出保留在 `package-native-output/verified-0lltiq8h/`，摘要 `package-native-summary.json`：六条完成 Error 在文件/stderr/stdout 为 **6/6/0**；六条预处理前 start 为 **6/0/0**；真实 `remove_file` 被目录阻碍的清理 Warn 为 **1/0/0**。目标目录及清理障碍里的外部内容未被覆盖或删除。子测试直接重跑仅保留信道证据，不重复计数。
7. `cargo build -p motion-desktop --features ffmpeg --locked -j 1` 成功，23.24 秒，21 条既有警告，`package-ffmpeg-build.log`。这是本分支产物，不作为所有尚未合并 PR 的集成版本。
8. 两个新增测试文件及已有包清理测试文件 `rustfmt --check`、暂存差异空白检查通过；来源校验覆盖 **1558 个跟踪路径**，桌面样式消费检查通过。原本格式不一致的生产文件没有全量格式化，避免无关差异。

测试开发中初版在两个原有行为边界上被既有断言拦住：并发取消提前看到 running，以及取消后的准备错误进入状态字段。实现已按原有行为修正，未放宽任何旧断言。新测试也修正了 Windows canonical 路径的前缀差异。初版包用例输出保留在 `package-publication-tests.log`；最终验收以本节列出的日志为准。

后续错误类型与图集边界补入同一个媒体 PR，组合验收见 [媒体故障类型与图集边界验收](media-error-boundaries.md)。上文保留生命周期阶段的原始证据，不与后续完整回归重复累加。

## Windows CI 清理日志与主线集成跟进

Windows job `118624909860`（run `39414379104`、提交 `54b7f4b`）为 636 通过、1 失败、1 忽略；失败的清理日志夹具期望路径匹配数量 1，实际为 0。#384 的同一夹具也失败。导入任务规范化根目录，而夹具用原始路径匹配；同一原生程序使用普通目录成功、使用 `alias/..` 等价目录失败，但实际已有清理 Warn，原始证据在 `artifacts/frozen-reader-isolation/teardown-alias-probe.json`。CI 未留存临时目录原始拼写，不能断言一定由 Windows 短名称造成。

| ID | 分类 | 修复与验证 |
| --- | --- | --- |
| MT-CI-01 清理路径夹具 | 测试修复 | 只在清理测试中构造实际存在的目录别名并规范化，仍以真实 OS 清理失败验证原路径、数量、Warn 原因、外来文件保留和默认信道。保留所有既有断言，仅增加失败上下文；生产清理行为和日志级别不变。 |
| MT-CI-02 主线并发测试 | 接口集成 | 无冲突合入主线 `7bc153a` 后，all-targets 检查发现新增 session_concurrency 的四个媒体方法仍声明旧 String 结果。只将测试提供者及其回调签名适配为 MediaResult，保留会话/表面接口类型及全部并发、panic、资源断言。原失败 `ci-followup-check-before-adapter.log` 留存。 |
| MT-CI-03 CI 触发 | 继承既有修复 | 直接继承主线的单次 PR 校验，覆盖非 main 目标的堆叠 PR；push 仅 main 与版本标签，保留完整 job。无新增 CI 实现、重试、忽略或断言放宽。 |

共同夹具修正先在 #384 的独立构建中完成负向对照：只去掉规范化后同一数量断言退出 101，恢复后通过；真实默认 Warn 文件/stderr/stdout 为 2/0/0。最终夹具 SHA-256 为 `86d1f5256666e3e836d11ea21fff04cde3e7a6ecfec9edf4659614532e1f0cdb`，本分支逐字相同。随后按用户要求将修正集中放回 #381，由堆叠 #384 继承，避免两个分支各维护一份。

本轮验证使用全新分支目标 `target/media-teardown-ci-verified`、便携 MSVC、锁定离线依赖、两构建任务、单测试线程、调试断言启用。最终桌面日志 **15 通过**（`ci-followup-logging.log`），会话并发 **3 通过**（`ci-followup-session.log`），CI 触发规则 **7 通过**（`ci-followup-workflow.log`）；不同用例合计 25，旧阶段结果不重复计数。工作区 all-targets/exclude Android/diagnostics 检查成功，40.91 秒（`ci-followup-check.log`）；FFmpeg 开发构建成功，23.22 秒、保留 21 条既有警告（`ci-followup-build.log`）。这些结果不冒充当前提交的完整 UI 或跨平台验收，远端新提交 CI 仍待完成。
