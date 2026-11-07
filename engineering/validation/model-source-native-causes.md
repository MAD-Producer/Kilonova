# 模型源验证与冻结导入的原生错误

关联 #138，基于 `8bd6d245`。开始前核对 UI、深度会话、跟踪和预渲染分支的
merge-base 独有代码及当前未提交改动。它们未修改本批模型目录验证/导入范围，
motion-scene 的错误枚举也无重叠。预渲染在 renderer.rs/lib.rs 新增入口和导出，
与本批新增错误类型的位置分离。没有改动这些分支或审查其 PR。

## 实现与行为边界

原 ModelSourceCatalog 把 ModelAsset::validate 和 import_frozen 的失败转换成
RenderError::Invalid(String)。新增 ModelSourceError 的 Validation/Import 变体，
保存 source_id 和原模型/场景错误，再由 RenderError::ModelSource 传播。
保留 `GPU validation:` 前缀和原始原因；新增 source ID 与 validate/import 阶段
上下文。这是错误呈现与原因链变化，不改变导入能力、校验规则或原子替换行为。

motion-scene 的 Cause::Io/Json 原来使用 thiserror 的 transparent 包装，source()
会转发到底层错误自己的 source()，跳过包装中持有的原始 IO/JSON 对象。现在
保持相同 Display，显式把原生对象作为 source；不从文本恢复类型或错误位置。
没有依赖、资产、UI、缓存策略、GPU/像素算法、重试、重启或等待期限改动。

## 实际输入与恢复

- ModelAsset 字节数为零：公开 CPU 目录准备及实际 Renderer::configure_assets
  返回 Validation，source_id=7，原生模型 Invalid 的对象地址可沿 source() 找到。
- 真实冻结 OBJ 包的 options 被改成非法 JSON，同时重算 options digest 及工程
  记录的包 digest/字节数，确保实际进入 JSON 解析，而不是先被 SHA 校验拒绝。
  保留真实 serde_json::Error 的 Syntax、line=1、column=1 及同一对象地址。
- 真实冻结包移除内嵌 MTL，更新资源计数及包 digest/字节数；实际导入器访问
  OwnedResolver，返回原生 NotFound，保留 `resource absent from frozen bundle`。
  不伪造 Err，也不调用私有转换器冒充公开失败。
- 三种失败后，完整输入工程 JSON、文件字节、旧目录 Arc、源数量及保留字节数
  不变。原生 GPU 配置用例继续渲染同一旧目录，96×64 全部像素必须与已验证
  包含不透明红色三角形的基准完全一致；renderer 健康检查不能清错或跳过。
  恢复原包后进行下一次显式准备/配置，仍共享原有解析资源。

## 默认观察与限制

没有新增 log 调用、观察键、计数器或 worker。既有 catalog/importer Info start
和 Error 记录继续使用原 log 基础设施；目录 Error 现在自然包含原生阶段与源 ID。
新子进程验收在没有 MOTION_LOG 覆盖的情况下，用 Info 捕获器接收实际生产日志，
验证文件中每条 Error 都进入 stderr、Info 不进入 stderr、stdout 不含日志，且
catalog Info start 先于错误。记录实际 JSON 位置与缺失资源原因，不只验证返回值。

捕获器是测试端 log sink，不是实际桌面 logger 或完整引擎 worker。因此不能把
上述结果声称为新的桌面端到端默认输出、请求在途发布或受控阻塞验收；相应已有
覆盖见 desktop-error-transport-current.md。冻结包内未找到资源是实际故障；正常
缓存复用、空目录、显式修复后的成功均不额外记作错误。

| 项目 | 处置与证据边界 |
|---|---|
| MS-VALIDATE | 原生模型原因与 source ID；实际公开 CPU/GPU 配置失败 |
| MS-IMPORT-JSON | 实际冻结包 JSON 解析；完整 source 链、位置、同一对象 |
| MS-IMPORT-IO | 实际冻结资源读取；NotFound、原始原因与同一对象 |
| MS-ATOMIC | 旧目录所有权、完整输入与全部像素保持；无新增恢复策略 |
| MS-LOG | 既有生产日志在默认 Info 的测试 sink 中复验；非桌面 worker 端到端 |
| MS-PENDING | 其他 CPU/摄像机文本错误边界及模型构造不变量仍未完成；不关闭 #138 |

已定位但未在本批修改的例子包括 renderer.rs 的 PlaneCompositor::prepare、
lens_output.rs 的 plate inverse/apply、color_output.rs 的输入 contract 转换、
model_layers/cameras.rs 的源摄像机采样，以及 image_resources.rs 的代理淘汰 IO
包装。这里列举实际已见边界，不把一次 grep 当作全仓无遗漏审计。

原始命令、源码摘要、初次新增测试的格式语法失败、负对照及最终输出保存于
`artifacts/issue-goal-current/model-source-error-causes/`。

## 最终验证

- Vulkan 完整渲染库 342 项、模型图层集成 58 项、场景导入集成 85 项，共 485 个
  不同测试通过，0 失败、0 忽略。场景库本身 0 个单元测试，两次空运行不累计。
- DX12 的新增 5 项全部通过，属于上述测试的重复后端验证，不重复累计。
- 负对照一只恢复两条目录 String 转换，实际 0 通过/5 失败；负对照二只恢复
  下层 IO/JSON transparent 包装，实际 1 通过/4 失败。前者分别拒绝丢失的
  目录原生身份，后者分别拒绝丢失的 JSON/IO 对象；不靠总通过数判断原因。
- 完整恢复原始字节后，四阶段最终验证前后 2,334 个 Rust/TOML/WGSL/Cargo
  文件摘要一致。带 FFmpeg 的 render/host/desktop 全目标检查通过。
- 默认捕获保存了 6 条实际 Error，文件与 stderr 逐条一致，Info 只进入文件，
  stdout 无日志；三次失败分别有自己的 catalog Info start，修复后的成功无
  额外 Error。这是既有 log 调用的测试 sink 验证，仍不冒充桌面 worker。
- 首次新增测试有一个括号语法错误，格式检查明确失败；修正的是新测试 logger
  的构造写法。没有修改既有断言、期限或忽略；既有 dead_code 警告仍保留。
