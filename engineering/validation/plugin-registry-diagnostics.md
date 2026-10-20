# 插件注册表恢复诊断的默认日志

关联 #132 第五项及 #352 的 MR-05。主线已能从损坏的 `disabled.json` 恢复、保留原文件并把原因放入 catalogue `errors`，但 Session 构造没有把这种 `Ok(Registry)` 的降级发布到默认日志。本次只补观察，不改变恢复行为、返回值、错误回复、插件选择或文件内容。

## 分支协调

从 main `58bf6742` 创建 `fix/plugin-registry-diagnostics`，开工前核对所有开放 PR、近期分支及 worktree：#344 修改 session 的 sampling 字段/生命周期、#349 属性筛选、#351 配置隔离、#352 边界测试、#353 图标生成器；表达式沙箱分支有自己的未提交测试。session.rs 仅增加观察模块、替换单个 registry load 调用、在 with_session 的锁前声明观察作用域，避开 #344 的 sampling 实现。其兼容性另外检查，不以文件名不同代替冲突审查。

## 观察审计

| ID | 分类 | 实现、证据与限制 |
| --- | --- | --- |
| PRD-01 | 新观察 | 宿主 Session 初始化调用的注册表加载返回 Ok 且有 diagnostics 时记录 Error；携带固定 site/operation、可用 session ID、路径、耗时、总诊断数、保留原因和省略数。初次构造尚无 session ID，明确记录 None，不虚构 request ID。真实加载 Err 仍原样传播，由现有启动/命令错误信道处理。 |
| PRD-02 | 正常分支 | 缺失配置、合法空配置和修复后重新打开保持安静；普通 state/Present 不读取配置文件、不复制诊断、不记录日志。未新增 Present start/done 或重启/重试。 |
| PRD-03 | 有界与抑制 | 每次冷加载最多保留 4 个原因，各 512 字符，路径 512 字符；作用域最多排队 4 次加载，多余次数饱和计数并附在最后一条记录中，最外层退出即清空。冷加载真实配置损坏立即 Error，不采用每帧退化的 60 次阈值；没有全局路径键集合或进程生命周期累计计数。连续普通查询不会重新加载，也不会重复报错。 |
| PRD-04 | 锁与主因 | with_session 在取业务锁前声明作用域，正常返回、Err、panic 都先释放业务锁，再从 TLS 队列取走事件，释放借用后调用 log。宿主测试实际 try_lock / try_borrow_mut 验证 logger 回调无锁，原 Err 与 panic 载荷保持，六次真实损坏加载只输出四条，记录 omitted_loads=2；修复配置后无残留。 |
| PRD-05 | 实际默认信道 | 桌面测试用真实 Engine、filesystem 和 production logging，MOTION_LOG 未设置，覆盖 missing/valid/damaged/oversized/directory 五场景，保留损坏源和项目字节、catalogue 原原因，120 次状态查询不重复日志；修复后显式重新打开恢复健康。每种损坏只在默认文件和 stderr 各出现一条 Error，stdout 不含该日志，记录位于既有 worker startup phase=session 之后。 |
| PRD-06 | 保留证据 | `artifacts/property-selection-context/plugin-registry-default-evidence/` 保存真实 damaged 场景的 motion-studio.log、stderr.txt、stdout.txt、summary.json；结果 exitCode=0，默认级别，fileErrors=1、stderrErrors=1、stdoutErrors=0。复现脚本 `capture-plugin-recovery.py` 在同级 artifacts 中；不是模拟成功返回或仅检查内存 logger。 |
| PRD-07 | 既有覆盖与边界 | 初始 worker startup 已在进入 Session 前写入并 flush 阶段记录；普通非 Present 命令的 in-flight/start 与 panic/timeout/reply 信道沿用现有 engine 实现。本次不新增命令、超时、线程重启或 UI 错误结构；不把这次读真实文件的结果当作所有阻塞场景的受控证明。 |
| PRD-08 | 待处理 | 观察范围是宿主 Session 构造中的 registry load；低层 motion-effects 的独立调用、install 内部重新扫描仍按既有诊断/错误协议，不声称均已新增日志。#132 的硬件断电、Unix 发布后 fsync 故障等验收仍未完成，不能关闭整个合集。原始 Registry diagnostics 保持原有形式，本次仅限制新增日志副本，不声称解决底层诊断总容量。 |

## 验证

Windows，现有 portable MSVC 环境，独立 `CARGO_TARGET_DIR=E:/Dev/aem/target/model-robustness-audit`，无新依赖或第三方素材。

- `cargo test -p motion-host --locked --lib registry_observation -- --test-threads=1`：初次 1 项通过，含 return/panic/bounded 三个独立子进程，执行 24.94 秒；`plugin-registry-host.log`。补充实际 6000 字符错误原因的截断用例后，四个子进程全部通过，执行 37.98 秒，`plugin-registry-host-final.log`；原始协议诊断仍超过 6000 字符，新增日志保持限长且不修改源文件。
- `cargo test -p motion-desktop --bin motion-studio --locked engine::failure::project_tests::recovered_plugin_settings_reach_default_file_and_stderr_without_polling_noise -- --exact --test-threads=1`：1 项通过，五个真实场景，执行 37.31 秒；`plugin-registry-desktop-final.log`。首次命令把短测试名与 --exact 搭配，实际 0 项，没有计为通过；保留 `plugin-registry-desktop.log` 后改为完整名称运行。
- `cargo test -p motion-host --locked --lib -- --test-threads=1`：35 项通过，无失败/忽略，执行 116.34 秒，`plugin-registry-host-all.log`；这轮先于新增 long 子场景，后者由上述专项复跑验证。
- `cargo test -p motion-desktop --bin motion-studio --locked engine::failure:: -- --test-threads=1`：17 项通过，无失败/忽略，执行 132.49 秒，`plugin-registry-engine-failure.log`；包括默认日志、原始保存错误、资源超限、panic、断连和回复超时信道。
- `cargo test -p motion-effects --locked --test registry_failures --test registry_recovery -- --test-threads=1`：4 项通过，无失败/忽略，`plugin-registry-effects.log`。上述计数含专项重叠，不累加声称不同测试总数。

组合兼容检查使用独立 worktree `.worktrees/integration-registry-sampling`，基于 #344 `4d528512`。首轮确实发现 session.rs 相邻新增模块的插入冲突；将观察模块声明移到既有 last_error 模块前后，用三方补丁重新检查，session.rs 干净应用，未改 sampling。独立 target `target/registry-sampling-integration` 编译 3 分 34 秒，四个真实恢复子场景专项 **1 项通过**，执行 21.55 秒，见 `plugin-registry-sampling-integration.log`。这证明宿主组合编译和本项诊断契约，不代表整个采样功能或组合 GPU/原生窗口已验收。

没有修改既有断言、面板、颜色/尺寸 token、视觉或交互。DESIGN §0.7 的十二项既有整改基线全部 retained：不在本次宿主日志范围；没有把编译通过当作界面验收。
