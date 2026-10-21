# 效果计划的类型化错误边界

关联 #138，基于 main `01183bdd`（已合入 #398、#399）继续实现。不修改另一任务正在接入的资源类别；#399 的交叠预算回收独立验收，不混入本次提交。

## RP-01：原生错误与明确的文本边界

效果计划的构建、预检、序列化、依赖排序及 alpha 同步直接返回 `RenderError`。删除先返回 String 再由 checked 包装器读取可变诊断字段恢复类型的路径；两套公开构建 API 合并为同一套。`last_resource_rejection` 继续用于已有诊断，返回错误的类型身份不再取决于它。

原生效果、图像及模型错误保留原对象；缓存的插件解析失败通过 Arc 保留原始错误，重复构建不重建字符串错误。带图层／效果上下文的错误保留原原因，嵌套序列化原因及资源拒绝可继续沿包装访问。已有只返回 String 的 CPU 子模块在入口明确转为 Invalid，不冒充已完成其内部错误治理。

冻结视频 API 仍为 String 结果，这是既有文本协议边界；新增显式 map_err 位于该边界，不给整个 movie 模块添加错误类型兼容转换。本次不宣称完成全部桌面文本回复迁移。

## RP-02：功能修正与正常分支

图像依赖遍历原先用公开绘制记录的 pass 范围直接切片；现在先验证范围，畸形输入返回含绘制索引、起止范围和 pass 数量的错误。范围之外的依赖顺序、缓存命中、预览绕过及正式输出策略不变。没有新增重试、线程重启、等待期限或静默成功。

预览原本允许的效果绕过继续产生诊断；正式／guarded 构建返回原错误，不能用后续的可变拒绝记录替代其身份。空依赖是正常分支，既有零分配对照继续验收。

## RP-03：观察与验证边界

没有新增日志事件或观察键，现有日志策略、阈值和信道语义保留。初次定向验收 32 项通过：原生原因、缓存身份及恢复、预检上下文、畸形范围、短缓冲写入保护共 6 项；合成 writer 字节／分配对照、预览 GPU 像素、零分配依赖排序及资源拒绝共 17 项；实际默认 logger 的故障路由共 9 项。后者包含新增缓存插件解析失败，两次原始错误经既有回复 sink 返回，检查请求 ID／图层／效果／原始原因、Info 文件、每次一次 Error stderr、stdout 隔离及健康恢复。它不代替完整 worker 的所有在飞／不返回路径验收。

日志为开发工作区 `artifacts/pr-ci-followups/render-plan-errors-{native-unit,plan-regression,default-channels}.log`，退出记录为 `render-plan-errors-targeted-results.json`。全部 target 的 render／host／FFmpeg adapter 编译检查通过。

两项可编译负面对照均在运行时退出 101：将缓存解析错误恢复为字符串 Invalid 时，原生 Effects 身份断言失败；将依赖范围检查恢复为直接切片时，畸形绘制范围触发越界 panic。测试各运行一项，均不是编译失败。每项结束后 finally 恢复原始字节，最终 effect_plan.rs 的 SHA-256 为 `c39d4b3cd868159340bee593164cbd74f350a9f50b9677d967ac4cff3079b251`。证据为 `render-plan-errors-negative-controls.json` 及对应的 `negative-{flattened-source,unchecked-range}.log`。

最终合并树回归及跨平台 CI 仍在验证；只编译或部分通过不构成最终验收。完整回归先清理本任务 motion-render 的构建产物，测试前后核对全部 Rust／Cargo 源文件哈希，避免故障变异产物或中途源码变化混淆结果。

首轮完整回归在 68 组中得到 414 通过、1 失败、1 项既有外部图片忽略后停止，源哈希全程不变。失败是本改造引入的集成缺口：`with_diagnostic` 保留原因后，RGBA 上传仅匹配最外层 Resource，漏加上传阶段上下文。改用递归 `resource_rejection()` 取得原拒绝，再包装上下文并保留原原因；既有的 object／source／PTS／phase 断言不变。修复后 source_allocation_budget 全部 6 项、resource_rejections 全部 3 项通过，另复核原生错误专项 6 项通过。首次失败、源收据及退出记录保存在 `render-plan-errors-initial-full-{render-host.log,source-receipt.json,results.json}`，修复专项为 `render-plan-errors-upload-context-regression.log`。不将第一轮错误说成时序抖动，也不以定向通过替代最终完整回归。

已有断言的 API 适配仅把字符串结果改为检查 Display／原生原因；容量、维度、像素、字节、分配次数和等待条件不放宽。旧合成 writer 保留独立的序列化实现，只在调用已变更的计划 API 时提取原始诊断。

## RP-04：保留任务

模型查询与动画采样的构造不变量、其余文本边界和整个 #138 仍需继续处理，不能据此关闭合集。未新增依赖、第三方素材或渲染语义。

未修改 panels、用户可见样式、菜单及主题值。DESIGN §0.7 十二项全部 retained：此次变更不涉及对应面板，逐项理由沿用 `host-error-contract.md` 的基线表。不以编译代替视觉验收，也不声称新的视觉验收已完成。
