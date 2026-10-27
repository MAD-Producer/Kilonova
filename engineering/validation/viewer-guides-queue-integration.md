# 参考线接续渲染队列的组合验收

#263 范围与[完整功能验收](viewer-guides-completion.md)保持一致。本次不是渲染队列功能实现；#520 已由其他贡献者合入主分支 `2227a195fb21d765b24abdb45f1759d7fe06c0ec`，本分支合并后实际源码提交 `583a954d2fdbf212bd76984b2698611c10efe5ce`。

旧 PR head `02e56f478ee8cbb66fe7c3c76d71e18921559c85` 五项有效 CI 已全成功，实际 checkout `738cf70524f98adabafdf0d2f572cd34520a7639`、树 `7afc86238ee41016ed35aee71cb39c6219cf5b2e`；三个桌面原始日志均确认130项参考线/变换/路径/Composition回归通过。然而新主分支增加了队列、持久输出和窗口入口，完整组合树已不同，因此旧绿灯不能直接用于新组合合并。原日志与结果原样保留，新 head 必须重新通过 CI 并核对完整组合树。

本次按当前工作树重新编译，强制本工作树lib根和main，源码前后哈希不变。程序SHA-256 `9e6bee6eead0dc871efda97caaed57935b7379c6e6276ff68a7781a3a5a2fdbd`。参考线15、变换24、Composition5、Path86，共130项定向回归通过；新增Windows手工原生用例在默认过滤器仍为ignored，随后显式单独执行通过，不重复计数。中英文主窗/原生浮窗四案例、受控1/1.5/2比例、Fit/缩放/平移产生22张PNG；最大线心坐标误差0.199951物理像素，生产PNG导出前后RGBA完全一致。默认日志与实际worker成功/排队失效/错误、文件/Error stderr/stdout隔离同一套断言均再次运行，没有改断言。

当前组合的完整workspace all-targets（既定排除Android）检查通过，启用diagnostics、ai-depth、ffmpeg及GPU测试编译，style consumer检查通过。模型4项和历史15项/1既有忽略沿用其记录的早期提交证据；本次不把它们冒称重新运行。原生仍使用隐藏真实窗口、受控Input和比例，不宣称物理鼠标/系统显示设置操作、其他平台新增原生截图、视频编码或整份DESIGN规范验收。DESIGN0.7逐项责任处置、素材来源与功能/观察分类仍见完整验收文档；本次新增源码变化来自main队列，未替其重新宣称视觉整改。

原始源码冻结、构建/默认日志、22张当前PNG与像素测量、旧CI原始日志和130用例核对见[组合证据包](data/viewer-guides-queue-integration-583a954d.zip)：54项/1088531字节，SHA-256 `7e52badfc0841b688c1339db33e32f2b7c58345e69dc00eff6bcd691fa3e89d7`。结构化[结果](viewer-guides-queue-integration-results.json)。此记录生成时，新head远端CI尚待执行，未合并。
