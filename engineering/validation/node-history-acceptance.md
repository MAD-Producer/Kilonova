# 原生节点撤销验收的异步边界

## 问题与修正

主分支 run 40525906679 的 Linux 桌面验收在 `undo did not restore the public default in the editable source` 失败。#527 将节点来源与图像准备移入异步 BoundOpen 后，`reconcile_node_target()` 返回只表示任务已排队，原阶段 15 紧接着读取的仍可能是旧编辑器。

将原有 `0.5` 默认值断言移至已有阶段 18，等待来源读取、编译安装完成且预览来源戳一致之后，与既有撤销像素断言共同执行。没有修改断言、期限、生产代码、返回值、重试或重启策略；也没有增加新的日志观察点。

## 验证与边界

Windows x86_64 Vulkan 下使用带 FFmpeg 的 release 构建，中英文 `--node-smoke` 均返回成功。两份报告的全部 layer 布尔检查通过，覆盖撤销恢复像素、来源重开及关联节点流程。源码、报告与原始日志哈希记录在[结果](node-history-acceptance-results.json)，日志归档见[证据](data/node-history-acceptance-evidence.zip)。

首次 debug 启动缺少相邻 FFmpeg DLL，补齐运行目录后 debug 全量 smoke 又在原有期限内超时；均保留原始日志，没有以这些运行作为通过证据。随后采用与 CI 一致的 release 配置完成验证，没有延长验收期限。Linux/macOS 的真实运行仍交由本 PR CI；本机结果不代表已完成跨平台验证。

本次只改验收阶段的等待边界，不改 UI 样式、资源、依赖、工程语义或渲染结果，因此不重新认定 DESIGN §0.7 的现存违规已修复。
