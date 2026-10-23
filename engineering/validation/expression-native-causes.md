# 表达式上下文保留原生原因

关联 #138、#135，基于本人 #426 的 `f9d88955` 补齐原生原因链，最终与该 PR 的运行时/圆弧不变量统一验收和提交，接入主线 `1021adf0`。开发工作目录使用 `fix/expression-error-causes`；不审阅他人的 PR。开工前核对其他工作目录的候选路径状态和近期提交路径；没有发现其他工作目录的脏候选文件。未在当前祖先上的历史提交属于早先版本/重写图，不表示这些历史分支正在实现本次原因链；本批只修改仍实际存在的表达式 message 拍平边界。

## 修改前实际证据

`expression-native-cause-baseline-v2/` 使用 #426 实际最终 FFmpeg 构建 `compiler-artifact` 指定的 Core、Model 特性闭包库运行公开 API 探针，前后库哈希保持。语法拒绝、结果维数拒绝、后续帧动态编译拒绝，以及显式取消，四种实际返回均含目标和帧，但标准 `Error::source()` 都为 None。另留存六组健康帧的原始浮点 bits。原 stdout SHA `c9213b9cb487a7c0a5685640a5f8866b5fdc19e2a968421dc005908a563fe0fd`。

首版外部探针把 `Engine::apply` 返回类型误写为 `EditResult`，没有成功编译或执行；其编译失败原始 stderr 保留在 `expression-native-cause-baseline/`，不能作为旧实现运行证据。修正探针返回类型后才得到上述实际留样，没有改产品输入、断言、预算或期限。

## 逐项处置

| 编号 | 分类与实现 | 证明范围、正常分支和剩余边界 |
| --- | --- | --- |
| E1 | 原生错误边界：`Error::Expression` 保留 `Box<Error>` 原因，取代把原因提前变成 message；核心包装函数直接移动原原因 | Display 的目标、帧和原因文字保持。公开实际错误必须能通过标准 source 继续诊断，不把字符串匹配当成类型身份 |
| E2 | 已有功能保持：表达式失败编辑仍拒绝并回滚，后续帧拒绝不改工程，正常表达式及恢复继续可用 | 原有沙盒、取消、100ms 帧预算和 20ms 执行期限不变；取消本身不是新故障，不新增取消 Error 日志 |
| E3 | 补充直接边界验证：私有包装器保留原生 IO 类型与 OS 错误码 | 这是受控原因注入，不声称真实用户表达式能直接产生这个 IO 失败；公开探针证明的原因仍是实际 Invalid |
| E4 | 既有观察信道：桌面错误路径继续负责请求、操作、耗时和默认文件/Error stderr，stdout 隔离 | 本批没有新增日志、计数、锁、重试、重启或 UI。完整 worker 与默认实际输出由单独用例验证；不把纯核心测试冒充默认日志验收 |

原生 JS 引擎自身错误仍在现有沙盒适配器变成 Model Invalid；本次保留的是已经收到的原生 Model 原因，不声称重新设计 JS 错误分类。`Session::new` 的表达式禁用回退、UI 字符串展示、桌面 String 响应和导出协议仍是既有功能/协议边界，未通过删除回退、改变响应或忽略错误来完成本次工作。模型中的其他构造、DecodeTask、MaskCache、粒子历史及旧前缀适配继续开放，合集不能因此关闭。

## 验收

待执行：核心/模型完整回归、宿主回归、实际 worker 默认输出、所有目标 FFmpeg/diagnostics 检查与产品构建。最终新库公开探针应与旧库的全部显示文字、帧、目标和健康 bits 一致，唯一预期差异为四种返回现在带原生 source。每阶段冻结输入，保留原始失败及成功记录，以实际终态认定完成。

前轮在原基底上完成模型/核心 334 项通过、0 失败、1 项既有人工测量忽略；宿主编译阶段没有终态。恢复会话后原执行句柄不存在，实际进程列表也没有该 Python/Cargo/测试进程，故明确记为中断，宿主阶段不计通过、不猜测退出码。旧日志及源码收据原样保留。随后接入主线新增摄影机代码，按最终组合重新执行上述验收；不是因一次观察超时而重启同一活任务。

旧圆弧/表达式综合探针打印取消错误的 Debug，而本次把 message 改成原生 source，会有一行预期表示变化。最终比较保留原基线及两份原始输出，要求除该行外逐字节相同；该行必须精确从 `message: "expression cancelled or frame budget exceeded"` 变为 `source: Invalid("expression cancelled or frame budget exceeded")`，其余目标与帧不可变。独立原生原因探针另要求 Display 完全不变。不能把这种有意的 Debug 结构变化称为整份原始 stdout 字节身份保持，也没有更新渲染或像素基线来掩盖差异。

## 最终本地组合结果

接入 main `1021adf0` 后，五阶段均以退出码 0 终态完成，源码从开始到结束保持：完整模型/核心 334 项、宿主 120 项、桌面模型错误信道 7 项，共 461 项通过、0 失败、1 项既有人工测量忽略。所有目标 FFmpeg/host diagnostics 检查及 FFmpeg 产品构建成功。收据与原日志为 `artifacts/pr-ci-followups/expression-native-causes-integrated-results.json`、`expression-native-causes-integrated-source-receipt.json` 及各阶段 `.log`，不与中断前结果或跨平台计数相加。

实际产品 `--command-list` 退出码 0，stdout 是 24475 字节 JSON，stderr 为空；产品 SHA `17466afd88c70ef5daa938072a8519d9e084c3925b8e5e4eadfe543bfb9db4c8`。本轮仍有已有未使用项等编译警告，没有宣称无警告构建，没有通过压低日志或忽略编译警告改变验收结果。

从最终产品 `compiler-artifact` 解析真实 Core/Model 库，并核对 manifest 指向冻结工作目录、新库哈希均不同于旧基线。`expression-native-causes-integrated-final-probe/` 实际验证四种返回带原生 source，Display、目标/帧及六组健康 f32 bits 与旧库一致。`model-runtime-invariants-combined-artifact-resolved/` 另执行原 405 次路径及表达式综合探针；原 343624 字节输出与新 343632 字节仅有上述预期取消 Debug 字段变化，其他全部字节保持，新 stdout SHA `a91442d8205a6e6d37620e2378994f1c1e574656dc2d6c1bbb78f22c86bc4b3c`。这不是像素或性能基线验收。

实际新桌面测试二进制再次直接执行 child 路径，原始文件日志、stderr、stdout、返回原因与二进制哈希保留在 `expression-native-causes-integrated-default-output/`。真实 Engine 的 start/edit/state/seek 验证语法及维数两种拒绝回滚模型/历史，随后健康编辑与 seek 恢复；默认 Info 文件中 start 先于同请求失败，文件与 Error stderr 各两条原始原因，含 request、edit、object=7、实际 frame=0 与耗时，stdout 隔离。不是手动 Flight 或假成功返回，也不是受控阻塞前状态发布实验；正常取消仅在核心层验证，不作为新故障写入日志。

本地验证期间主线又新增效果缓存实现，仅涉及五个效果控制/缓存测试与文档路径，不与本 PR 自有修改重叠。最终远端 CI 必须验证新 head 与最新 main 的实际合并 checkout；上述 461 项及产品是明确以 `1021adf0` 为基底的本地证据，不冒充新增 UI 的执行验证。最终发布与合并仍以远端实际状态和源码树核对为准。
