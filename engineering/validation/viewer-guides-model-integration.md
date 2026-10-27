# 参考线接续模型图层与深度任务的组合验收

#263 范围与[完整功能验收](viewer-guides-completion.md)一致。本次同步主分支 `06fa8cf533760fea69cbc28533469bc93bb7f6d5`，包括模型图层、深度任务和图层独立开关，实际源码提交 `9dd2a8e6f87f7de94b1ec14f7cf18c6279fd1185`，未重做其他贡献者的功能。中间650d组合的stage12原始记录保留；最终数据来自接续图层开关后的stage13。

旧 PR head `0cb3dbeb67d759945c08b0eacd62547541b58d6a` 五项有效 CI 全成功，实际 checkout `2fc20e1450e554e8d1a142f66784e3066b6b3910`、树 `b4b2cfcb9a9a32e1de592d1598d9b4a64dc0225a`；三个桌面日志各确认130项相关回归通过。最新主分支完整组合发生变化，旧日志保留为历史证据，不能用于批准新组合合并。当前 head 仍须重新通过全部 CI。

本次按当前工作树重新编译（stage12切换工作树强制lib根与main，stage13同工作树按改动依赖并强制main），源码前后哈希一致。程序 SHA-256 `71f89cb7918ac839bdd803e5636cf3de5bb9f86780de94e296c108a68906362f`。参考线15、变换24、Composition5、Path86，共130项定向回归通过；Windows 手工原生用例在默认过滤器仍忽略，随后显式执行1项通过。中英文主窗/原生浮窗四案例、受控1/1.5/2比例产生22张PNG，最大线心坐标误差 0.199951 物理像素，生产PNG导出前后RGBA完全相同。实际worker成功/排队失效/故障、默认文件日志/Error stderr/stdout隔离沿用原断言并再次执行，没有放宽断言。

完整workspace all-targets检查通过（既定排除Android），启用diagnostics、ai-depth、ffmpeg与GPU测试编译；style consumer检查通过。早期模型持久化4项和历史15项/1既有忽略保持其原提交证据，不冒称本轮重新执行。原生使用隐藏真实窗口与受控Input/比例，不宣称物理鼠标或系统DPI操作、其他平台新增原生截图、视频编码或完整DESIGN合规验收。12项设计基线处置、来源和责任边界见完整功能验收。

[原始证据包](data/viewer-guides-model-integration-9dd2a8e6.zip)：95项/1805399字节，SHA-256 `f9550360addf151f3f998f96c25baea3be8fe29cd35eb4bf3fbd51a8adaf0ae3`；[结构化结果](viewer-guides-model-integration-results.json)。此记录生成时尚未合并，新远端CI待运行。
