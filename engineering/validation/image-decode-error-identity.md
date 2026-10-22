# 图片解码原生错误与取消身份

## 图片独立阶段范围

以下保留图片独立阶段的证据；最新组合结果见文末。

跟进 #138 与 #135。基线为 main `b485344347bbf9701463db30d6c61c5c61795728`，含 #408、#409、#411；开始前检查其他本地分支和开放 PR，图片源码不与 #412 的矢量纹理准入文件重叠。没有 UI 样式、像素算法、依赖或资产变化。

| 编号 | 分类 | 实现、证据与边界 |
| --- | --- | --- |
| IDE-01 | 功能修复 | 图片源检查、元数据读取、同步/异步解码及 caller-buffer 解码保留 IO、模型、image、PNG 原生错误。RenderError 与宿主 Error 显式接入图片错误；渲染器保留原有显示前缀和 source ID 上下文。原实现三个公开 API 用例实际因 String 擦除身份失败，并非产品自身 panic。 |
| IDE-02 | 正常分支与协议边界 | 取消用独立枚举，渲染器不再匹配错误文本。普通文本即使等于取消提示也仍是验证失败。正常缓存未命中、过期结果、Empty 轮询、丢失接收端的 best-effort 回复仍沿用既有行为，不新增日志、重试、重启或等待策略。 |
| IDE-03 | 功能修复与保留不变性 | 预览 reader 用 Arc 保留同一 PNG 原始原因，后续轮询不再转回 String。实际 CRC 损坏文件验证多次查询的 Arc 身份、工程不变与新请求恢复；同时去掉该 reader 中三处既有 IO/模型原因转文本。外部文本回复协议仍显式转成 String。 |
| IDE-04 | 既有观察复验 | 桌面测试实际执行 image_info、prepare_image 的缺失文件、非法路径、PNG CRC 拒绝，经生产 respond 与实际 mpsc 检查默认 Info 文件、Error stderr 和 stdout 隔离；健康文件恢复。Flight 是手动测试上下文，不声称完整 Engine worker 调度、in-flight 发布或受控阻塞验收。没有新增生产观察、阈值或计数。 |
| IDE-05 | 图片独立阶段通过 | 完整宿主/渲染器、桌面失败信道及 FFmpeg adapter 共 613 项通过、0 失败、1 项既有忽略，FFmpeg 产品构建成功。preflight 的 1 项包含于桌面结果，不重复计数；五阶段源哈希不变。远端 CI 尚待最终 head 自身完成，不把本地结果当成远端完成。 |

## 对照与原始失败

`image-errors-before.log` 在未修改的 main 生产代码运行三个公开测试，0 通过、3 失败，断言指出原生身份被 String 擦除。最初夹具把健康资产写在工程根目录，触发现有 assets 目录约束；原输出单独保存在 `image-errors-first-fixture-failure.log`，只纠正本批夹具的目录，原身份断言和五秒期限不变。

首次迁移编译漏掉一处 speculative start 的 String 映射，保留 `image-errors-initial.log` 并补齐调用。新增 GPU 夹具误用不存在的 Scene::default，保留 `image-errors-gpu-initial.log` 后改成真实 Scene::new。没有放宽既有预期，也没有修改生产预算、期限、忽略、日志过滤或取消语义。

第一次完整编译另发现既有取消测试直接把 String 结果与字符串比较，无法编译新的原生错误类型；原始编译失败及源收据保存在 `image-errors-first-full-*`。该调用方检查适配为原生错误的 to_string，精确预期文案保持为 `image decode superseded`，并增加 Cancelled 枚举断言。没有为生产错误实现字符串相等操作，没有放宽原来的取消消息约束；这是 API 类型迁移所需的测试调用适配，而非删改失败条件。

第二次完整执行在宿主回滚测试失败，原始 `image-errors-second-full-*` 保留 68 通过、1 失败及源收据。该用例原先把缺失图片要求为通用 RenderError::Invalid；本批消除该原因擦除后，实际得到 Render(ImageResource(Io(NotFound)))。按新的功能契约将该处适配为精确的原生 IO/NotFound 身份检查；工程、revision、完整 history、不可撤销、renderer 目录、健康提交及 undo 恢复断言全部保留。该轮部分通过不计入最终数量，不用退回通用错误满足旧分类断言。

## 限制

不关闭 #138/#135 合集。其他 CPU 几何/着色器计划文本接口、直接动画采样不变量与旧桌面/导出文本协议仍在范围外。图片 worker 的原有 panic 捕获与断连现在有独立类型，但本批不宣称复现了真实第三方解码 panic、OS 线程创建失败或永久阻塞的 C/文件 IO。DecodeTask 的退出等待/线程 join 未在本批改变；缓存代理的可恢复失败观察沿用既有覆盖。Android 没有新增目标/设备验收。

所有工作区原始日志和收据位于 `artifacts/pr-ci-followups/image-errors-*`。本文件区分新增功能、既有观察、正常分支和剩余边界，不将处置说明当作全部缺陷完成。


## 最终本地验证

本次 613 项相关回归对应上述基线加本批图片错误实现，包含真实 renderer 的配置、同步和异步拒绝、完整旧图片解码与缓存用例、宿主同源 Arc/工程不变/新请求恢复以及默认信道。唯一忽略是既有 optional_local_original_image_probe，需要仓库外的 MOTION_IMAGE_SOURCE；本批未新增忽略。原始文件与 Error stderr 各三条，stdout 隔离，实际公开图片 API 可恢复。完整 Rust/Cargo 源收据见 image-errors-final-source-receipt.json，五阶段退出和计数见 image-errors-final-results.json；初始失败证据均保留，不累加重复执行的定向或前置用例。

来源守卫、样式消费者、差异空白与新文件格式检查通过。PR 中旧取消测试仅作必要的 API 调用适配：精确消息保持且追加类型检查，不缩减旧失败条件。本地基线未含 #410 的时间轴/视频帧增量；editing.rs 的测试分类适配与其修改位于不同代码段，组合主线仍须以本 PR 远端实际 checkout 和 CI 验证。


## 图片与维度修复的最终组合验证

此节取代前面的阶段性待验状态。组合当前主线 `13c5fb9216f3a6c3a6b1b681a5d51b2f4741c390`、图片修复原 head `24d39977f9d94af3dc1b30810d136553d77275db` 与本批维度修复，八阶段原始 Rust/Cargo 字节哈希全程不变。完整模型/宿主、核心维度专项、图片与维度/矢量 GPU 专项、完整桌面错误信道及 FFmpeg adapter 共 **291 项通过、0 失败、1 项既有外部图片探测忽略**，带 FFmpeg 产品构建成功。两项前置门禁均已包含于桌面结果，不重复计数。渲染器运行的是上述相关用例，不冒充此组合的全部渲染器回归。

实际组合程序重新采集默认输出：维度拒绝六条、图片拒绝三条，分别在文件与 Error stderr 保留原原因，stdout 隔离，健康轨道/图片操作恢复。记录在 image-track-default-track-output 和 image-track-default-image-output；Flight 均为手动上下文，不冒充完整 Engine 调度。原图片 613 项及维度独立阶段的完整回归各自保留原收据，不能累加为当前组合数量；原图片 head 的五项 CI 已成功，也不能代替本次最终 head 的远端检查。

实际阶段日志与收据为 image-track-final-results.json、image-track-final-source-receipt.json 和对应 image-track-final-*.log。提交前核对源文件与 Git blob 字节一致，差异空白、来源守卫、样式消费者和三个新轨道测试文件格式检查通过。此前新模块声明行的 CRLF 已在组合构建前规范为仓库要求的 LF，原阶段与规范化收据分别保留。未放宽任何既有断言、期限、预算或忽略；最终远端 head 仍须自身通过检查，主线原生 FFmpeg 任务也单独验证，PR 的 skipped 不代表执行成功。

无可见 UI、依赖、资产或像素语义变化。#138/#135 继续开放，直接通用 Track::sample 不变量、其余 CPU 文本边界、旧协议和 DecodeTask 退出仍在范围外。
