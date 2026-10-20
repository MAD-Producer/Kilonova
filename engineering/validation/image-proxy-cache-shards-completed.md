# Image proxy cache directory shards

## Scope and coordination

This batch addresses issue #130 section VIII's process-wide proxy cache IO mutex, against main `d0cfbab1`. Twelve open PR file lists, eighteen recent local branches and dirty intended paths across every worktree were checked before creating `perf/image-proxy-cache-shards`; no overlap was found. The draft sampling work, preferences/shortcuts, FFmpeg source-host repair and the separate media cleanup PRs retain their own files and branches.

The baseline serializes read/header/hash validation and write/scan/eviction for every project's disposable proxy directory under one process-level mutex. This is existing mutual exclusion protecting a per-directory cache budget, not an error in the file checksum or publication format. Unrelated projects consequently wait for the longest cache IO in any project.

## Implementation and limits

The implementation uses a fixed array of 64 mutexes, selecting a shard from the canonical cache file's parent directory. Production cache paths already canonicalize and validate that directory under the project root before use. All filenames in the same directory share a shard so read/write/publication/eviction retain their previous exclusion and budget semantics. Hash collisions conservatively serialize some unrelated directories; this is bounded sharding, not a promise that every pair of projects has a distinct mutex.

No path-key registry or per-project mutex allocations are retained. Hashing a path needs no file IO and no new dependency. Existing proxy byte limits, source hashes, file names, checksum validation, unique temporary files, rollback/cleanup, cancellation and failure-streak resets remain unchanged. The poison observation keeps its existing bounded process-lifetime counter across all shards and reports a bounded shard number after guard release.

## Regression method

Two regressions use actual proxy writes and checksum-validated reads. The first holds one directory's actual cache mutex while another thread reads, overwrites and rereads a different directory. It releases the guard and joins the worker before asserting the result, so a failing baseline cannot strand a blocked worker. A two-second completion bound detects the process-wide lock; it is not a throughput benchmark. Candidate directory selection avoids hash collisions so the case tests actual independence rather than claiming collision-free sharding.

The second checks 128 distinct file keys within one directory cannot acquire its lock concurrently, then runs eight actual writers/readers against that directory and checks every returned pixel buffer and final file count. Existing poisoned-lock, default-Info/Debug threshold, unwind, corrupt-file and cleanup-suppression tests still run. Their lock references target the selected directory shard, while logging callbacks check every shard is free. No existing assertion was weakened or removed.

Parallel testing exposed an existing temporary-collision fixture's assumption that its next process-wide serial cannot be consumed by another directory. Its original `AlreadyExists` and content assertions remain unchanged. That test now acquires all shard locks solely while injecting its exact temporary filename; acquisition uses nonblocking attempts and releases partial locks before retrying, with a five-second test failure bound. This isolates the shared serial without adding a global lock to production or changing temporary-file naming. The new held-lock cases use the real poison-recovering cache-lock helper.

## Acceptance

- Baseline with only a test accessor returning the original global mutex: **1 passed, 1 failed**, exit 101. Actual unrelated-directory IO times out while the first directory is held. Evidence: `artifacts/issue-130-proxy-shards-before.log`.
- Initial post-fix cache group: **7 passed**, exit 0, including original failure/observation cases. Evidence: `artifacts/issue-130-proxy-shards-after.log`.
- An initial incorrect per-file-key mutation survived the original two-filename check because those keys collided. The new case was strengthened to check 128 keys; the same mutation then fails with the wrong lock exclusion, exit 101. Initial and final evidence: `artifacts/issue-130-proxy-shards-initial-key-mutation.log` and `artifacts/issue-130-proxy-shards-file-key-mutation.log`.
- Collapsing every directory to shard zero fails the final cross-directory regression with a real IO timeout: **1 passed, 1 failed**, exit 101. Evidence: `artifacts/issue-130-proxy-shards-global-mutation.log`. Both mutation scripts restore the production file byte-for-byte with SHA-256 verification.
- Initial eight-thread cache-group verification exposes the shared-serial fixture race: **6 passed, 1 failed**, exit 101. After fixture isolation: **7 passed**, exit 0, followed by eight fresh-process eight-thread runs of the same group (**56 successful test executions**, not 56 different tests). Evidence: `artifacts/issue-130-proxy-shards-parallel-before.log`, `artifacts/issue-130-proxy-shards-parallel-after.log` and `artifacts/issue-130-proxy-shards-parallel.log`.
- Final complete `cargo test -p motion-render --locked -j 1 -- --test-threads=1`: **215 passed, 0 failed, 1 ignored**, 53 result summaries including doc tests, exit 0. The existing ignored image probe needs an external fixture and was not claimed as executed. Evidence: `artifacts/issue-130-proxy-shards-complete.log`. An earlier complete run before fixture isolation also passed; the final full suite was rerun after that test change.
- Touched child test/observation files pass `rustfmt --check`; whitespace, the desktop style-consumer guard and the provenance guard (1,470 tracked paths) pass. No panel, UI/style, asset or dependency changes.

Runs use locked dependencies, portable MSVC, one build job and one test thread, with `E:/Dev/aem/target/image-proxy-cache-shards-validation`. Debug symbols and incremental compilation are disabled only through local environment variables; debug assertions and repository profiles remain unchanged. These are local Windows results; remote CI remains required before merge.

## Remaining scope

Issue #130 remains open. Global session registry scope around native/GPU work, per-project blocking IO, decoder/worker lifetime boundaries and UI responsiveness during long serial engine operations are not fixed by this cache change. No UI layout, interaction, assets, dependencies, timeouts, retries or restarts are in scope.

## 主干集成与 Intel 验收预算

旧 head `9e1aaa80` 的 push run 39352494410 中，Windows、Linux、ARM、Rust/GPU 和 libav 均成功；Intel job 118399664195 在 Post rust-cache 清理阶段被取消。check 注释明确为超过 `1h0m0s` 上限，不能把完成主要构建步骤或其他平台成功称作整个 PR 已通过。

同步 main `34d1d7ad862b10205bf16e8a606b839206fd57eb`（含 #360 下载源修正与 #364 Intel 90 分钟预算），合并提交 `e7e305edee3e85862e2ff60b4a476b6a3e1885ea`。相对于新 main，仍只有原六个功能及验收文件；没有改变分片实现、锁范围、错误/日志阈值或既有测试断言。新增主干内容不是本 PR 的重复功能认领。

集成后的工作流策略检查 4 项通过（4.428 秒）；来源检查通过（1,531 个受跟踪路径）、面板样式消费者及差异检查通过。完整渲染回归使用独立的原验证 target 及 locked 依赖，最终结果记录在后续条目；重新推送后的精确 head 仍需完整远程平台验证，合集 #130 继续保持打开。

最终 `cargo test -p motion-render --locked -- --test-threads=1` 退出 0：**291 passed、0 failed、1 项既有外部图片探针 ignored**，含 doctest 共 63 组结果，测试执行时间合计 427.60 秒。证据为 `artifacts/pr-queue/cache-shards-main-integration-render.log`。本次测试覆盖集成后完整渲染代码，而不只是工作流 YAML；计数增加来自主干既有回归，不认领为本 PR 新测试。没有修改忽略条件或断言。

## 原生验收修复堆叠集成（2026-10-20）

接入 #368 `1381cc133c3407d1f4af9eff8a0c2beeaa257eb1`，中文合并提交 `a4b19c6`。实际相对原 head 仅五个上游文件：工作流、采样/PNG 测试、诊断完成记录和工作流测试；本 PR 的六个代理缓存功能文件未变。原 291 通过、1 个既有忽略的完整渲染回归是上一 head 的证据，本次没有重新执行或将其冒充本次全套组合通过。

本次新增工作流组合回归 **5 通过，9.241 秒**，日志 `artifacts/pr-queue/cache-shards-native-ci-workflow.log`；来源守卫（1532 跟踪路径）、面板样式消费者及差异检查通过。没有用户可见界面、素材或依赖改动，不重复截图；生产缓存路径和日志语义仍按本文件原有真实故障与恢复证据评估，最终远程组合 CI 待完成。

预演树已无冲突；第一次将包含空行的命令输出直接传给 `git diff --stat` 时，Git 拒绝空 revision。该命令失败不作为差异审查证据。随后逐项核对实际五文件差异，并重新提取唯一哈希比较：实际树与 `merge-tree` 都为 `6bca64a7eb5cfba0f9e4ef379f05d4b9cd8c1a4c`。没有解决冲突或手工选择缓存代码。

合并顺序为 #368 → #363；父 PR 进入 main 后再核对和调整 base。#130 等合集的其他边界仍开放，不能仅凭此子项关闭。
