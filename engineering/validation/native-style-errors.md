# 样式原生错误传递验收（#138）

本次补齐样式文件及覆盖配置的原生错误边界，不代表 #138 全部完成。

| 项目 | 处置 | 验证边界 |
|---|---|---|
| STYLE-IO | 新增类型化错误，保留原始 std::io::Error、路径、打开/读取阶段 | 缺失文件、目录代替文件；平台决定实际失败发生在 open 或 read |
| STYLE-JSON | 新增类型化错误，保留 serde_json::Error 的分类、行号和列号 | 文件语法错误、覆盖配置 schema 错误 |
| STYLE-LIMIT | 原有有界读取保留，超限成为独立 Cause::TooLarge | 256 KiB 限制保持，读取最多 limit+1 |
| STYLE-VALIDATION | 语义规则保留，分类为 Cause::Invalid | 版本、负尺寸和非对象配置依旧拒绝；细分每条编译规则仍待后续 |
| STYLE-TRANSPORT | 接入现有 Host NativeSource 与 engine 共享错误，首选项 Context 不格式化来源 | clone 仍指向同一个原生错误；没有通过字符串推断类型 |
| STYLE-ATOMIC | 保留既有先验证/准备，再持久化与安装的顺序 | 验证/解析失败后完整样式文档、编译样式、设置文件、首选项和工程状态不变；不扩大为样式源保存失败也有事务性保证 |
| STYLE-LOG | 复用既有 Info 开始、首次 Warn、第 60 次 Error 文件和 stderr、限流与恢复重置 | 子进程移除 MOTION_LOG，确认每轮 1 条 Warn（仅文件）、1 条 Error（文件及 stderr），其余抑制，确认 stdout 隔离及两轮故障后的恢复 |
| STYLE-BOUNDARY | 字符串只在展示错误的原有 UI/示例入口生成 | 不改参数面板、交互、布局、token 或资源 |
| STYLE-REMAINING | 字体提供者、首选项持久化提供者仍暴露字符串；#138 畸形输入审计仍有独立范围 | 本提交不宣称这些已修复，不关闭 #138 |

未新增依赖或第三方资源；不变更工程格式、渲染语义或附件操作策略。DESIGN.md §0.7 的现有面板条目保留，本次没有面板呈现修改。

既有 Shell 测试为新的错误返回类型增加 to_string()，继续同样的 contains 判断；菜单测试保留原错误内容的精确比较，并新增 Compile 阶段、Invalid 类型及无文件路径检查。没有放宽原内容判断、超时、预算或忽略策略。新增测试的初次失败、修正和负对照应保留原始输出，不能以绿色计数代替错误信道验证。

## 实际验证

基线提交：`5a025a2d23f2cc35ee4d306550861b086e0b2378`。Windows 本机验证，Rust 1.97.1，带 FFmpeg 8.1 的桌面测试。`run.py` 保存命令、环境白名单、输出、退出码、耗时及测试前后完整源码 SHA-256；下面各轮源码前后均一致。

| 验证 | 真实结果 | 原始记录前缀 |
|---|---:|---|
| UI 库完整单元测试（包含新增 5 项原生错误检查及既有菜单检查） | 135 通过 / 0 忽略 | `ui-lib-v2` |
| 既有样式集成测试 | 6 通过 / 0 忽略 | `ui-style-regression` |
| 真实 Shell 原生错误、完整首选项/设置文件/工程不变、共享错误身份、默认信道和恢复 | 1 通过 / 0 忽略 | `desktop-native-v2` |
| 既有 Shell 热重载及关联反馈保留 | 2 通过 / 0 忽略 | `desktop-style-regression` |
| 既有首选项、快捷键、应用/重启、播放选项与失败回滚 | 6 通过 / 0 忽略 | `desktop-preferences-regression` |
| 既有真实 fallback 故障、限流、恢复与原样式保留 | 1 通过 / 0 忽略 | `desktop-fallback-regression` |
| UI 全部目标（含 framework 示例）编译检查 | 通过 | `ui-all-targets-v1` |
| 桌面全部目标（FFmpeg）编译检查 | 通过；最终记录由 `desktop-check-final` 复核 | `desktop-check-v1` |

共 **151 项不同测试**。针对新增来源检查的独立重复执行不重复计数。未运行全工作区完整测试，不将本机结果泛化为全部平台。

### 负对照

仅将 `style::Error::source()` 的 IO 和 JSON 来源连接改为 `None`，保持错误本身和所有测试断言不变。`ui-source-negative` 实际执行 5 项：2 通过、3 失败，退出码 101；缺失文件、目录读取和 JSON 的原生来源检查都拒绝该回退。随后按字节恢复原文件（恢复前后 SHA-256 为 `834da8b18d7210247349bf664b7bc10d65644675af5732237b1a19732f61234d`），`ui-native-final` 5 项再次通过。`negative-summary.json` 记录负对照及恢复身份。

### 初次失败及保留边界

- `ui-native-v1/v2`：原菜单测试直接比较 String，与新错误类型不兼容；迁移为上述保留原条件并加强类型的判断，修正 Rust 模式守卫借用后，`ui-native-v3` 及完整 UI 库均通过。没有给错误类型添加字符串身份匹配兼容层。
- `ui-lib-v1`：直接启动测试程序未带仓库 Cargo 配置提供的可写临时目录，13 项创建临时文件失败（PermissionDenied）。使用工程内临时目录后，`ui-lib-v2` 同一份 UI 源码 135 项通过；未改断言。
- `desktop-native-v1`：新增测试错误地假定首次记录是 Debug，真实共享 Counter 是首次 Warn、第 60 次 Error。保留生产 Counter，新增测试严格检查两轮共 2 条 Warn 和 2 条 Error，以及 Warn 不进入 stderr、Error 必须进入 stderr。`native-v1-failure` 原始输出与首次失败完整保留。
- 共享 Counter 首次 Warn 是既有覆盖，本次不宣称其满足“通常首次 Debug”的推荐，也未改变它影响其他故障站点的策略。
- 保留原有编译警告，不在本 PR 修改无关面板或渲染资源代码。

完整本地证据位于 `E:/Dev/aem/artifacts/issue-goal-current/style-error-causes`。`style-error-causes-evidence.zip` 包含原始成功/失败输出、负对照、默认日志与 stderr/stdout、命令包装器、源码快照及清单；归档校验见同目录 `evidence.json`。不包含临时工程、构建产物或系统字体。
