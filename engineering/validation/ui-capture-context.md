# UI 截图失败的路径与阶段上下文

Refs #138。本批只补充 `CaptureUi` 已有失败的诊断上下文，整个错误类型合集继续开放。

## 问题与实现

实际原生验收曾在写截图时遇到磁盘空间不足。默认日志只有请求号、`capture_ui`、耗时和系统原因，无法直接区分输出目标或失败阶段。修复前的真实 worker 负对照再次验证：未附加 surface、向目录保存 PNG 均返回原错误，但缺少上述上下文。

处理器现在为返回 `Err` 的 UI 截图额外写一条 Error，包含请求号、session、窗口、阶段、耗时、路径及原原因。日志在 `with_session` 和帧资源记账完成后写出，不持有业务或观察锁。复用既有格式化器：路径最多 256 字符，原因最多 2048 字符，控制字符归一为空格。每次请求只保留栈上的阶段，不添加字典、计数器或跨请求原因缓存。

既有 `respond` 的原原因 Error 和回复保持，故同一失败可以看到上下文记录与原记录；不将后者重复计为新增故障观察。成功捕获不写新的上下文记录，Present 分支未改动。

## 实际验证

前后对照使用同一份新测试。基线 `1bf63268`，修复 `e4d9b66d`，全部受控输入仅 `desktop/app/src/engine.rs` 改变。合入曲线主线后的 `75b6929d` 与已测修复提交完整 Git 树相同，未冒称重新执行。

| 验证 | 实际结果 |
|---|---|
| 修复前 | 0 通过 / 1 失败 / 0 忽略，16.27 秒；三个原错误、文件/Error stderr、stdout 隔离和健康恢复先通过，随后仅缺失上下文断言失败 |
| 修复后 | 1 通过 / 0 失败 / 0 忽略，10.64 秒 |
| 已有错误长度与回复测试 | 1 项通过 |
| 已有命令队列测试 | 4 项通过 |
| 已有四种精度的真实 UI/节点截图 | 1 项通过，4 种精度各保存 UI 和节点 PNG |
| 消费者检查 | FFmpeg 桌面全目标检查通过 |

共 **7 项不同测试**通过；子进程重复入口不累加。新测试实际调用三次失败、两次成功，不把五次调用算成五项测试。两条失败分支为未附加 surface、向现有目录保存 PNG。第二次无 surface 请求携带超长中文路径，验证截断后尾部标记不会进入文件或 stderr；保存失败使用真实 OS IO 错误，不伪造返回值或再次填满磁盘。

健康截图在每次运行的错误前后像素一致；修复前后对应的两张 PNG 也逐字节相同。项目和 revision 保持。默认 `MOTION_LOG` 不设置，检查初始化为 Info、每个请求的原错误与新上下文均出现在文件及子进程 Error stderr、stdout 不混入日志。负对照的父测试会在断言失败时转储文件日志，该转储不冒充日志器的 stderr 行为。

测试子进程有 300 秒上限；这是测试看门狗，不是产品线程超时、重试或重启。实际运行均在上述时间完成。

## 观察分类与剩余边界

| ID | 分类 | 证据与处置 |
|---|---|---|
| UC-1 | 新增上下文 | `capture.target` 两次真实无 surface 错误、`capture.save` 一次真实写出错误；同请求号的路径、阶段、session、窗口、耗时、原因实际核验 |
| UC-2 | 已有覆盖 | 原 Error 和 String 回复来自既有 `respond`；本批保留该边界，未迁移桌面协议错误类型 |
| UC-3 | 有界输入 | 复用 256/2048 字符上限；超长路径尾部被截断，既有长错误测试验证原因有界且回复不截断 |
| UC-4 | 正常分支 | 两次成功捕获、健康像素和工程状态验证；该 API 成功返回 JSON，没有需要改成故障的 `Ok(false)` / `Ok(None)` 分支 |
| UC-5 | 已有运行状态 | 非 Present 的 Info start 和 in-flight 发布流程不变；精度用例核验默认 start，队列用例核验受控阻塞期间状态。未新增完成日志替代 start |
| UC-6 | 协议边界 | 未改接收端退出后的 best-effort `respond`，也未新增接收端退出注入或改变返回值 |
| UC-7 | 待注入 | session 查找、资源分配、painter、atlas、presenter、paint、节点预览、提交、readback 的阶段按源码归属记录；本批未注入这些失败，不算已验证故障站点 |
| UC-8 | 已有覆盖及限制 | panic hook 与非返回操作沿用现有诊断；新增阶段是局部变量，仅返回 Err 时输出，没有新增 panic/挂起时的阶段发布，未重新注入这些情形 |
| UC-9 | 功能与视觉边界 | 无重试、清错、回滚、线程恢复、错误结构、面板、样式、交互、依赖或第三方素材改动；不将 DESIGN 既有整改列为本批完成 |

两次测试基础设施失败保留：`before-01` 缺邻接 DLL，退出 `0xC0000135`，未执行正文；`before-02` 使用 `--ignored` 而 GPU 测试已启用，选中零项，被回执拒绝。补齐同版本 DLL、使用精确入口及 `--include-ignored` 后，`before-03` 才是有效负对照。未改测试断言、产品行为或 GPU 门槛。

## 复现与证据

Windows 的实际 GPU/隐藏窗口环境，按项目既有方法准备 FFmpeg 运行库后执行：

```powershell
cargo test -p motion-desktop --bin motion-studio --features ffmpeg,gpu-tests --locked -- actual_capture_failures_report_bounded_paths_and_phases_without_changing_replies --include-ignored --nocapture --test-threads=1
```

[机器回执](ui-capture-context-results.json)记录源码/二进制哈希和精确数量；[原始证据包](data/ui-capture-context-evidence.zip)包含启动失败、零用例拒绝、有效负对照、修复后日志/PNG、既有回归及消费者检查。只含本项目自制夹具产生的日志、JSON 和 PNG，无 EXE/DLL/PDB/字体。Linux/macOS 本批仅由 CI 编译验证，未冒称执行 Windows 原生失败用例。

最终 PR CI 仍待验证；合并还要求五项有效检查、Windows 新用例实际执行以及全部 job 的完整代码树与最新 main 组合一致。主分支发布 CI 不在本地通过声明内。
