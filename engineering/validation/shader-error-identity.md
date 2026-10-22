# 着色器原生错误与不可变程序缓存

跟进 #138、#135，基于本人 #413 的 `bfcaf38fa54a4c0bdf3d4bd3937b5ac4329a5d98`。只检查其他活动本地分支的文件归属，避开正在修改的 renderer.rs、effect_plan.rs 和宿主 export.rs；没有审查其他 PR。

| 编号 | 分类 | 实现、证据与限制 |
| --- | --- | --- |
| SEI-01 | 功能修复 | SDK 与宿主 WGSL 解析、验证及 GLSL writer 初始化/输出保留实际 Naga 原因，通过标准 Error::source 可检查 ParseError、带 span 的 ValidationError 和 GLSL Error。解析增加 WGSL parse 前缀及固定 phase，GLSL 保留原前缀并增加 writer 阶段和 shader stage；字段放在原生原因前，避免既有日志长度限制截掉阶段。这是明确的诊断文本变更。正常源码检查、指令/循环预算、编译选项和 shader 数据不变。 |
| SEI-02 | 功能修复 | 合成、平面和蒙版不可变程序缓存返回 RenderError，失败只保留一个 Arc 原因，重复查询不重新编译或转换成文本。缓存错误不会因调用方更换闭包而重试；健康恢复用另一个独立缓存验证，不能冒充同一个失败 OnceLock 可以清空恢复。 |
| SEI-03 | 单独说明的功能边界 | 合成 GPU 构造在创建资源前编译其宿主程序，以 ? 返回实际失败，去掉 expect；这是功能修复，不伪装成日志观察。蒙版 GPU 仍保留原 GPU validation 显示前缀，Diagnostic 内保存真正原因。当前内置常量编译正常，尚不宣称在未修改常量的实际 UI 复现了该 expect 崩溃。 |
| SEI-04 | 既有观察复验通过 | 桌面子用例实际调用公开 shader 编译 API，分别验证解析、验证和 GLSL 拒绝，经既有 From、生产 respond 与实际 mpsc 验证默认 Info 文件、Error stderr、stdout 隔离和健康编译。Flight 为手动上下文，不证明完整 worker 调度或 in-flight 发布，也没有从 UI 注入坏的内置常量。 |
| SEI-05 | 原始失败 | shader-errors-before.log 在原生产编译器上执行新公开 API 测试，1 通过、6 失败：标准 source 缺失，不是六次产品 panic。健康宿主/SDK 的 WGSL、GLSL、反射、标志和 loop_work 已留样，后续逐字节比较。 |
| SEI-06 | 本地验收通过，远端待验 | 完整 effects/宿主、渲染器库与相关合成/蒙版/放大 GPU、桌面失败信道和 FFmpeg adapter 共 397 项通过、0 失败、0 项忽略，带 FFmpeg 构建成功；三个前置门禁不重复计数。远端仍待本次 head 自身验证。没有面板、菜单或样式、依赖版本、第三方资产、shader 源码、工程格式或像素算法变化。 |

第一轮实现保留了 Box<具体 Naga 错误>，但标准 source 暴露的是 Box 包装，六个原断言继续失败，记录在 shader-errors-first-*。修正生产错误容器为保留真实动态错误的 Box，使直接 downcast 看到原始 Naga 对象；没有放宽类型断言。

随后专项 5 通过、2 失败，揭示初稿两个返回类型夹具实际先由 Naga 前端拒绝，不能当作 Validator 复现。替换后的 SDK 非一致采样夹具被当前编译器合法接受，第三轮 6 通过、1 失败，原始完整输出也保留。通过独立编译探针确认 Validator 的真实拒绝输入后，新夹具使用动态数组后仍含字段的结构体，以及 position builtin 的非法类型；仍要求确切的带 span ValidationError。第四轮公开编译器专项 7 项通过，健康程序与原始样本逐字节相同；更正后的原实现对照、缓存及完整验收另行记录，不把旧夹具误分类算成产品故障。

更正后的七项夹具对照在 shader-errors-corrected-before-*，原效果编译器 lib.rs 和 shader.rs 从 bfcaf38f 逐字节恢复，仅运行 effects 专项：1 通过、6 失败，随后恢复修复源码并核对全体 Rust/Cargo 哈希。增加实际 writer.write 拒绝后，shader-errors-output-before-* 扩展为 1 通过、7 失败；同一纹理搭配两个采样器触发真实 ImageMultipleSamplers，不用伪造返回值。宿主编译选项和生产 shader 常量未改变。

四项缓存专项通过。原 effect_plan::error_tests 中的 shader_failure_retains_native_effect_error_and_source 仍要求宽泛 Effects(Invalid)，实际执行以 0 通过、1 失败留档。功能契约如今是 Effects(ShaderParse)，因此将该调用方适配为确切变体并增加原生 source 指针身份断言；原输入和 Effects source 类型断言保留，不退回文本错误来满足旧分类，也不删除恢复/回滚或期限检查。

桌面默认输出专项同时检查解析、验证、GLSL 初始化、输出和超长解析五个实际原因；另有 Naga 本身的一条 Conflicting samplers Error，不能吞掉或过滤后伪称只有五条。目标为文件和 stderr 各六条 Error，其中五条带生产回复操作/请求标识，stdout 隔离。回复保留完整原始原因，日志遵循既有协议：最多 2048 个字符，并将控制字符压平成空格。超长解析检查截断后阶段仍可见。实际默认输出已重新采集并通过，收据见 shader-errors-default-ready-output；子用例核验完整回复，独立采集保留原始文件、stderr、stdout 和五个原因。

第一轮九阶段验证在完成 8 项公开 API 和 10 项渲染错误前置门禁后，于桌面编译阶段主动中止：默认文本还未带出原生 GLSL 阶段字段，需要补齐诊断。只终止本人本地流水线及子进程，未动 #413 的 GitHub CI。中止时源码哈希不变，shader-errors-final-* 与中止记录保留，不能当作最终完整回归；补充诊断后的收据使用独立的 shader-errors-verified-*。

第二轮 shader-errors-verified-* 的公开 API 8 项、渲染错误 10 项前置门禁通过，新增桌面夹具失败：夹具把包含换行的完整回复直接当成了日志预期，忽略既有日志压平和限长协议。修正夹具分别核验完整回复和有界日志，并增加实际超长解析错误；生产日志规则、长度限制和回复语义不变。最终验收另用 shader-errors-accepted-*，不覆盖这次真实失败。

原树 shader-errors-accepted-* 九阶段全部完成：完整 effects、渲染器、宿主、桌面失败信道与 FFmpeg adapter 共 686 项通过、0 失败、1 项既有外部图片探测忽略，FFmpeg 产品构建成功；前置门禁不重复计数，九阶段 Rust/Cargo 字节哈希不变。

随后仅同步主线 b0dfb416 的非交叠文件，本批原有 12 个文件字节不变。shader-errors-integrated-* 的三个前置门禁、完整 effects 54 项、渲染器库加合成/蒙版/放大 GPU 138 项均通过；宿主编译在本人 #413 的 image_error_identity 测试夹具失败，因主线 Platform::attach_surface 新增 AllocationBudget 参数。只给 NoMedia 的未使用 surface 方法补齐形参，原错误、所有断言与期限保持。原编译失败保留，不计为已执行的宿主测试。修复后只重验受影响的完整宿主及尚未执行的桌面/adapter/构建，记录为 shader-errors-ready-*；此前五阶段及其实际原始收据保留，唯一 Rust 差异是未编入这些阶段的宿主集成测试文件。不得据此宣称两个源码树全体哈希相同。

其他 CPU 几何/生成器文本边界、通用 Track::sample、旧桌面/导出文本协议、DecodeTask 退出、内部正则/固定头不变量及不可返回的第三方调用继续开放。没有新增重试、重启、超时、错误清除、日志阈值或 Present 起止记录，也不宣称全部可观测性工作已完成。

工作区原始证据保留于 artifacts/pr-ci-followups/shader-errors-*；不把生成输出或整份日志提交进仓库。


同步前的 shader-errors-accepted-* 完整 effects/渲染器/宿主回归单独保留，不与同步后的数量累加。最终九阶段验收 shader-errors-ready-*：完整 effects/宿主、渲染器库与合成/蒙版/放大 GPU、桌面 engine::failure 与带 FFmpeg adapter 共 397 项通过、0 失败、0 项忽略，带 FFmpeg 产品构建通过。公开 API、渲染错误与桌面默认输出三个前置门禁已包含于完整回归，不重复计数。前五阶段沿用实际原始收据；唯一 Rust 变化是没有编入这些阶段的 NoMedia 宿主集成测试形参。修复后宿主、桌面失败信道、adapter、构建四阶段的全体 Rust/Cargo 字节哈希保持不变。健康编译输出与原始实现留样逐字节相同。此前各轮失败与主动中止独立留档，没有放宽既有断言、期限、忽略或预算。

已在本人 #413 的基础上同步主线 `b0dfb416dd7c403e5ea9c549c09257b36cb56d17`，原有 12 个文件与同步路径无交叠；另适配本人旧图片测试夹具的一处新增预算形参，最终 13 个文件，不改变原断言。同步后重新验证上述组合，渲染器为完整库测试加三组相关 GPU 集成，不冒充此组合全部渲染器集成回归。来源守卫、样式消费者、差异空白和新测试文件格式检查通过。远端 CI 和合入后的原生 FFmpeg 任务需独立验证，不能用 skipped 或旧 head 检查代替。当前记录只验收上述 SEI 条目，不宣称 #138/#135 已全部完成。
