# demo 工程构造错误传播

关联 #138、#135。本批在本人 `f04ad7ca` 源码上开始，已与命令目录提交 `f0a97236` 合为工作分支 `refactor/model-command-invariants`，计划作为一个后续 PR 验收，不重复启动两套远端 CI。

开工前逐个核对 25 个候选路径在其他工作目录中的脏状态和近期提交元数据：只有旧 egui 分支的 device_failure 测试有同路径修改；没有改它，近期记录属于已被当前主线取代的历史提交。没有审阅他人的 PR。上一批命令目录的真实进程已完成，才在该独立工作目录整合并准备新的冻结输入。

`Project::demo()` 改为 `Result<Project>`，沿用 `Project::new` 的真实校验并通过 `?` 返回原始错误，替代内部 unwrap；宿主 template 和 render_probe 示例传递原生错误。22 个现有测试文件只补齐成功夹具的 unwrap，原断言、期限、预算和忽略保持。没有复制一份绕过校验的构造，没有兼容适配或恢复默认值。

这是一个固定构造不变量清理，不是已复现的用户输入故障。固定参数目前合法；没有改常量制造产品故障，也没有为不可达的错误添加日志。宿主原生错误路径、日志分层和 stdout 协议保持。其他私有判别分支、DecodeTask、MaskCache、粒子历史与旧文本协议仍开放。

修改前以真实 Model 库调用 demo、validate 并序列化完整工程，stdout 2229 字节、stderr 空、退出 0，SHA `7e3f8c39d0c8b9d69379326336d90cf3fb0b96b8647e4422b0a5015ef750d92c`，三层均保持 3D。`demo-construction-baseline/` 保存源片段、原始输出、实际库 compiler-artifact 与输入哈希。第一次收集器因父构建 rlib 的旧哈希已被后续合法构建替换，在编译和执行探针前拒绝；修正为从成功产品的实际 compiler-artifact 解析当前库，确认两目录 demo 源文件相同，再留样。不把前次收集器拒绝算成产品失败或成功执行。

最终组合七阶段已全部实际完成：模型/核心 334、宿主 120、音视频格式 3、device_failure 与 wgpu_upgrade 两个实际渲染故障目标 3、MCP 6 项，共 466 通过、0 失败、1 项既有人工测量忽略；所有目标 FFmpeg/diagnostics 检查与 FFmpeg 产品构建退出 0。所有阶段核对冻结 Rust/Cargo/CI 字节一致，原始日志、执行命令、时间和 SHA 在 `model-command-invariants-results.json`。既有 unused/dead_code 警告保留，不宣称零警告，也不声称执行了全部渲染器集成或全套像素基线。命令目录旧批 277 项属于不同源码，不累加。

`model-command-final-product/` 根据最终成功构建的实际 compiler-artifact、manifest、feature 选择产品和原生 Model/Host 库，产品 SHA `a0b1f81b9dd0a9d4f86f62fc2cbaa73783d7d63e34261370d777378e7207cbb4`。真实 CLI stdout 24475 字节、MCP stdout 97630 字节，与修改前原始产品逐字节一致；55 条命令与三个响应核对通过，两个进程退出 0、stderr 空、stdout 仅 JSON。实际新库的 demo、validate 和宿主 template(0) 成功，template(1) 保持原“unknown project template”拒绝；新 demo JSON 2229 字节与原始旧库完全相同，SHA 保持上述值。源码、二进制和实际探针输入在整个执行中未变。此处是原生模板函数和真实 MCP/CLI，不冒充 GUI、完整 worker 故障注入或受控阻塞。

本地验收完成，远端 CI 与合入等待对应最终提交的真实结果；#138/#135 保持开放。无 UI、样式、菜单、依赖、资产、工程字段或渲染语义改动。

| 项目 | 分类与处置 | 剩余边界 |
| --- | --- | --- |
| D1 | 新增构造清理：模型 demo 返回原生 Result，宿主与渲染示例继续传播 | 固定参数合法，未声称复现正常用户输入使其失败；原生校验保留，无新故障观察 |
| D2 | 既有覆盖适配：22 个成功夹具文件只对 Result 解包，模型/核心、宿主及受影响集成目标已执行通过 | 不修改原断言、期限、预算或忽略；限定为已记录的实际目标，不冒充全部渲染验收 |
| D3 | 保留并待独立核验：audio_levels、masks、vector/groups 中的私有判别 panic 表达 | 包含提前返回、穷尽 action 和包裹构造假设；只读分类不是公开可达性证明，不算本批已修复 |
| D4 | 保留并待后续实施：DecodeTask、MaskCache、粒子历史缓存与旧文本响应 | 与此固定构造不同；候选渲染路径另有活跃分支，不能因本批通过而关闭合集 |
