# 桌面发布验收的 MCP 工具清单

main `0790404f` 已公开 `import_tracking`，但 `tools/ci_desktop.py::verify_mcp` 仍要求旧的十项清单。主线 run [41635401270](https://github.com/MAD-Producer/Kilonova/actions/runs/41635401270) 的 Linux 原生步骤因此报 `missing=[], unexpected=['import_tracking']`，没有执行后续共享工程检查。

本次只在固定预期清单中增加这个已发布工具。实际清单与预期仍须严格相等；未将检查改为子集、未忽略未知工具、未从实际回复生成预期，也未改动 JSON-RPC、共享状态、保存、GPU 检查及原有 20/40/10 秒期限。生产程序和工作流不变。

## 实际验证

| 控制 | 实际结果 |
| --- | --- |
| 原 verifier + 实际桌面程序 | 真实复现主线清单失败，完整异常与原始 stderr 保留 |
| 修改后的同一 verifier + 同一程序 | 完整 verify_mcp 通过：初始化、工具清单、共享编辑、小数帧 seek、保存、进程退出、工程文件和 GUI/GPU 设备检查 |
| 缺少工具负对照 | 从一次真实 tools/list 回复删除 motion_seek，原严格检查拒绝，报告 missing=['motion_seek'] |
| 未知工具负对照 | 向一次真实 tools/list 回复注入 not_a_published_tool，原严格检查拒绝，报告 unexpected=['not_a_published_tool'] |
| 既有打包与工具回归 | 运行35项，34通过、1项原有跳过；无失败 |

负对照只在外部验证脚本的接收队列中改写一次实际回复，不修改程序、工程、产品协议或 verifier。原成功控制没有这种改写。

首次 Python 单元测试调用发生两项模块导入错误，未执行产品测试：缺少 yaml 和 tools 的模块路径。改用仓库现有 tools PYTHONPATH 和已安装、与 CI 固定版本相同的 PyYAML 6.0.3 后运行上述回归。没有新增依赖、修改断言或将导入错误计为产品负对照。

程序 SHA-256：`98a80dc955a38cf08e5dc343d2c56e626c47f4b47e9f59ec6ca77076e7890594`，源码为 #604 的 `ae83440117bd49f35ab4667fb04854ea683cd4e8`。它相对 main 只增加节点编辑器源码；MCP、engine、tracking 和其他运行时代码与 main 完全相同，路径差异收据随归档保存。这是该实际程序的原生验收，不冒称一个单独构建的纯 main 程序。合批后再次核对 crates/desktop/Cargo 运行时 Git 树不变，CI 清单修正不改变已验证程序。

使用默认 MOTION_LOG=info，未覆盖运行时 RUST_MIN_STACK。原生 stderr 含本机 Epic Vulkan overlay 两个 JSON 文件缺失的 loader Error，原样保留，不能称 stderr 为空或这些环境错误已修复。verifier 按原逻辑逐条解析 stdout JSON-RPC；没有将日志转到 stdout 或过滤原始 stderr。

原始失败、成功报告、窗口输出、两项实际回复负对照、原生日志、单元测试输出、前后 verifier 字节及运行脚本见 [清单](desktop-mcp-inventory-evidence/manifest.json) 与同目录 evidence.zip。校验脚本验证产品差异恰为增加一个清单成员，全部其他检查字节不变；归档逐成员 SHA-256 回读校验。

## 并行工作与限制

开始前核对开放 PR 文件及相关差异：#597 正在修改 AAC 对照文件，本次不编辑它；#575 在同一 MCP 清单加入尚未合入的 configure_prerender，本次只处理当前主线已发布工具，未复制或修改其分支。#575 后续整合需要保留双方的已发布清单成员，而不能覆盖回旧集合。这是已记录的单行整合重叠，不宣称文件完全无冲突。

本次不关闭 #125、#134 或发布验收合集。Linux 与当前提交的完整主线工作流仍需实际执行；AAC 映射输出对照仍有独立失败，不能将本次原生控制通过写成主线发布已全绿。没有接手或审查其他 agent 的 PR，也未改动断言、预算、异常处理、自动重试或发布策略。

错误处理处置：新增观察无；生产错误分支无变更；原清单拒绝及其原因保持，缺失/未知成员控制均真实拒绝；模块导入错误作为调用环境失败保留。正常协议回复、缓存和 Present 不新增日志。DESIGN §0.7 所有既有面板项保留，因为本次不编辑面板或样式；本次窗口输出不是 UI 整改验收。
