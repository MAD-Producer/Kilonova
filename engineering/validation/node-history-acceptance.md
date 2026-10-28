# 原生节点撤销验收的异步边界

## 问题与修正

主分支 run 40525906679 的 Linux 桌面验收在 `undo did not restore the public default in the editable source` 失败。#527 将节点来源与图像准备移入异步 BoundOpen 后，`reconcile_node_target()` 返回只表示任务已排队，原阶段 15 紧接着读取的仍可能是旧编辑器。

将原有 `0.5` 默认值断言移至已有阶段 18，等待来源读取、编译安装完成且预览来源戳一致之后，与既有撤销像素断言共同执行。没有修改断言、期限、生产代码、返回值、重试或重启策略；也没有增加新的日志观察点。

## 验证与边界

Windows x86_64 Vulkan 下使用带 FFmpeg 的 release 构建，中英文 `--node-smoke` 均返回成功。两份报告的全部 layer 布尔检查通过，覆盖撤销恢复像素、来源重开及关联节点流程。源码、报告与原始日志哈希记录在[结果](node-history-acceptance-results.json)，日志归档见[证据](data/node-history-acceptance-evidence.zip)。

首次 debug 启动缺少相邻 FFmpeg DLL，补齐运行目录后 debug 全量 smoke 又在原有期限内超时；均保留原始日志，没有以这些运行作为通过证据。随后采用与 CI 一致的 release 配置完成验证，没有延长验收期限。Linux/macOS 的真实运行仍交由本 PR CI；本机结果不代表已完成跨平台验证。

本次只改验收阶段的等待边界，不改 UI 样式、资源、依赖、工程语义或渲染结果，因此不重新认定 DESIGN §0.7 的现存违规已修复。

## CI 运行预算补查

run 40620825955 的审计、Rust/GPU、Windows、macOS Intel 均通过；Linux 在 job 的 60 分钟总预算耗尽后取消。官方检查注释为 `The job has exceeded the maximum execution time of 1h0m0s`。不是新的断言失败：17 分 21 秒用于桌面测试编译，此后约 31 分钟仍持续输出通过的测试，终止前最后一项 shader 错误信道测试也已通过。

日志中多个隔离失败测试分别耗时约 40 秒。为降低编译及强制回溯解析的成本，Linux 桌面 dev/test profile 使用 `line-tables-only`，保留函数、源文件与行号信息，不生成完整类型调试数据；Windows/macOS 仍用原级别。此处是待 CI 实测的性能修正，尚不能认定全部耗时来自 DWARF。没有延长 job 或产品期限，没有跳过测试、缩减断言、关闭回溯或添加重试。原始 Linux job 日志和注释保存在本机 `artifacts/pr-ci-followups/node-history-acceptance-ci-linux-full.log`，待新运行验证后再冻结结果。

首次配置重复定义已有 job `env`，run 40627617285 / 40627885628 被 GitHub 以重复键拒绝，未执行任何测试；第二次中间修正只更换表达式上下文，没有解决重复块，仍不算有效验证。已将配置合入原 env 块，并使既有工作流策略测试使用拒绝重复键的 BaseLoader，保留 `on`、布尔字符串的原解析语义。新增负面对照确实拒绝两个 env 块。最初本地系统 Python 缺 PyYAML，改用已具备依赖的工具环境后执行；失败输出不作为通过证据。
