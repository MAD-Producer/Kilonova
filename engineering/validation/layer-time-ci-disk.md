# 图层时间映射：Linux CI 磁盘准备

## 失败证据

PR #579 的提交 `2bbb5bc2` 在 [Linux job 125163671849](https://github.com/MAD-Producer/Kilonova/actions/runs/41410961603/job/125163671849) 编译 `motion-studio` 测试程序时失败。日志原文：

```text
2026-11-02T08:12:58.3907076Z rustc-LLVM ERROR: IO failure on output stream: No space left on device
2026-11-02T08:12:58.5064241Z error: could not compile `motion-desktop` (bin "motion-studio" test); 36 warnings emitted
2026-11-02T08:12:58.5385787Z ##[error]No space left on device
```

该 job 此前恢复了约 1746 MB 的压缩 Rust 缓存。日志没有提供文件系统容量或展开后的缓存大小，不能据此把根因归咎于某个具体缓存文件；可以确认失败发生在测试执行前。

## 改动与验证

Linux desktop job 在恢复缓存之前移除托管 runner 预装、桌面构建不使用的 Android SDK 和 .NET SDK，输出清理前后的磁盘容量。删除目标是两个固定 SDK 目录；不删除源码、Cargo 缓存、测试产物、故障证据或运行所需的原生依赖。

既有 `tools/test_ci_workflow.py` 的 14 项工作流策略检查通过；现有 actionlint 通过（本地未运行 shellcheck）。首次本地 Python 缺少 PyYAML，随后使用仓库已有的 PyYAML 6.0.3 路径和可写临时目录运行成功；没有添加依赖或修改断言。

这是 CI 环境准备修正，不改生产逻辑、测试集合、断言、预算或重试行为。实际 runner 的剩余容量与完整构建是否通过，仍需由后续 CI 证明；本地静态检查不等于 Linux 验收通过。
