# 命令目录与 MCP schema 构造不变量

关联 #138、#135，工作分支 `refactor/command-catalogue-invariants` 基于本人 #426 最终源码 head `f04ad7ca`。本批独立开发，不改 #426 已提交的源文件或重启其 CI。开工前核对其他工作目录及近期提交的候选路径；同名脏文件只在两个 10 月 10 日旧目录，没有修改它们，近期未在当前祖先中的记录属于已被后续主线取代的历史提交。没有审阅他人的 PR。

## 分类与边界

| 编号 | 实现与分类 | 验证范围与限制 |
| --- | --- | --- |
| C1 | 构造清理：命令描述使用带原生字符串 id 的结构，在序列化前过滤内部命令，消除从刚生成的 JSON 反取 id 的 unwrap | id 仍来自同一个穷尽 Command 匹配表，不把私有描述丢失当成已复现正常用户崩溃 |
| C2 | 构造清理：浮点数组维数使用显式 `[f32; N]` 声明和原生 usize，替代解析类型说明字符串再 unwrap | 现有 2/3/4 维定义保持；非法维数的控制夹具应在编译期拒绝。不是动态用户输入的新拒绝，不添加假故障日志 |
| C3 | 构造清理：MCP 使用同一描述结构直接提供的 `input_schemas()`，消除对目录 JSON 的数组 unwrap | JSON catalogue 与 MCP schema 来自同一表，不改返回值、协议错误或客户端权限。未声称移除 MCP 中所有其他 JSON 构造不变量 |

Named 字段仍沿用原有标量、可选类型与自定义载荷说明；自定义载荷的 object schema 属于现有正常协议，不当作故障。没有新增 logger、计数、默认值恢复、重试或重启，没有 UI、样式、菜单、依赖、资产、工程或效果包格式变化。消除的是三处构造 panic 表达，不是三个已证明公开可达的故障；demo 常量、其他模型、DecodeTask、MaskCache、粒子历史及旧文本协议仍需分别处理，合集不能关闭。

## 修改前真实留样

`artifacts/pr-ci-followups/command-catalogue-baseline/` 使用 #426 实际 FFmpeg 产品，二进制 SHA `17466afd88c70ef5daa938072a8519d9e084c3925b8e5e4eadfe543bfb9db4c8`。真实 CLI `--command-list` 退出 0，stdout 24475 字节、55 条命令、stderr 空，SHA `f15c565bf8a172a89f2c4784a8d47134afd61f1a64225fae1c44e5c7e42f1ac0`。真实 `--mcp` 在隔离工程与本地配置目录运行 initialize、initialized 通知、tools/list、command_list 调用，返回三个 JSON-RPC 响应、stderr 空，stdout SHA `c762b670a09f84db1180fac55e3c93b0d4698fc1a02ebfbb028af3abab068edf`；CLI 与 MCP structuredContent 一致。原始请求、输出与执行收据保留，不是自算期望或模拟成功返回。

## 当前执行证据

四阶段已实际结束：完整核心回归 271 通过、0 失败、1 项既有人工测量忽略，包含真实命令目录唯一性、可解析 schema 与内部包装命令过滤的既有测试；实际 MCP 回归 6 通过、0 失败、0 忽略；所有目标 FFmpeg/diagnostics 检查和 FFmpeg 产品构建退出 0。合计 277 项通过，不与父批或其他源码阶段累加。每阶段均核对全部 Rust/Cargo/CI 输入字节未变，原始日志和 SHA 保存在 `command-catalogue-invariants-results.json`；既有 unused/dead_code 警告保留，不宣称零警告。

`command-catalogue-literal-controls/` 使用 `git show` 取得的旧实际 descriptor 和新源文件的 FieldKind/宏片段，并锁定同一 serde_json 库的字节身份。旧 descriptor 对控制字段 `[f32; nope]` 实际编译成功，运行产生 ParseIntError panic，由夹具捕获；新的 2/3/4 维和标量健康控制实际编译并运行成功，新 `[f32; "nope"]` 在编译期以 E0308（期望 usize，实际为 &str）拒绝。编译/运行 stdout、stderr、片段和库哈希全部保留。没有向产品源码注入该字面量，也不把内部控制构造冒充正常用户入口的崩溃复现。

`command-catalogue-final-product/` 按成功构建的实际 compiler-artifact 记录选择 FFmpeg 产品，核对工作目录 manifest、feature 和二进制身份。最终二进制 SHA `ba99ef490f020869dc3662376ca1b06f64b0b4b3cab1f13d14387b9558de0e82`，与旧产品不同；真实 CLI stdout 24475 字节、MCP stdout 97630 字节，分别与原始旧产品逐字节一致，55 条命令和三个 JSON-RPC 响应核对通过，两个实际进程均退出 0、stderr 空、stdout 仅 JSON。整个执行期间源码和二进制字节保持，不使用新实现自己生成的期望替代旧留样。

本地验收完成，远端 CI 与合入尚待对应实际提交。没有新错误观察或降级策略，也不据此关闭 #138/#135 的其他未完成项。

本批随后与 demo 构造合为 `refactor/model-command-invariants`，最终源码独立执行七阶段 466 项通过、0 失败、1 项既有人工测量忽略，所有目标检查和 FFmpeg 产品构建成功。最终真实 CLI/MCP 仍与这里的原始旧产品逐字节一致；这是另一个完整组合的收据，不能与上述 277 项相加。最终产品和 demo 原生库对照见 `engineering/validation/demo-construction-invariants.md` 及 `model-command-final-product/`，远端结果仍须按最终提交核验。
