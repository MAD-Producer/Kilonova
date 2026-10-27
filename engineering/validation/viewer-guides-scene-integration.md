# 参考线接续预设与共享深度的组合验证

#263 功能范围与[完整验收](viewer-guides-completion.md)一致。本次同步主分支 `4184f0c8d49db7eb7be6b3ff26cde4a499cd3c6f`，包括其他贡献者的预设、导入动画/摄影机与共享深度通道，实际源码提交 `196fa3b00e96c5b0d80b1645cbc81226f28fbda2`；没有重做其功能。

旧PR head `6587894f3c9f251e570cfcf2226916e4b30afce1` 的五项有效CI全部成功，实际checkout `b591731f09a1930e43711385ac8a9e0e51a4d08e`、树 `027eeac39def9d2359925c8a5606e508305464da`。三个桌面原始日志各确认130项参考线相关回归通过，但最新主分支组合树不同，旧结果原样保留为历史证据，不能用于合并新组合。

切换工作树后强制本工作树lib根与main重新编译，完整Rust/WGSL/Cargo/样式源前后哈希一致。程序SHA-256 `d1396d40829418af8b5ddf1c4d6a55915064746117cee34e587e1f51b0a8a3bc`。130项定向回归通过；手工原生用例在默认过滤器仍忽略，随后显式单独执行1项通过。中英文主窗/原生浮窗四案例、受控1/1.5/2比例产生22张PNG，最大线心误差 0.199951 物理像素，生产PNG导出前后RGBA完全一致。实际worker成功、排队失效与故障，以及默认文件日志/Error stderr/stdout隔离沿用原断言再次运行，未放宽断言。

完整workspace all-targets（既定排除Android）与style consumer检查通过，启用diagnostics、ai-depth、ffmpeg及GPU测试编译。早期模型持久化4项与历史15项/1既有忽略保留原提交证据，不冒称本轮重跑。原生使用隐藏真实窗口与受控Input/比例，没有物理鼠标或系统DPI操作；其他平台新增原生截图、视频编码及整份DESIGN规范不在本轮声明中。12项设计基线处置与来源责任见完整验收；本次未新增依赖或第三方资产。

[原始证据包](data/viewer-guides-scene-integration-196fa3b0.zip)：54项/1111265字节，SHA-256 `8270cd1f13a8afe398a6af907fa84dad44aab7b91a8cc890682543ad29e13026`；[结构化结果](viewer-guides-scene-integration-results.json)。此记录生成时尚未合并，当前head仍须重新通过全部CI。
