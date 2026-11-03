# 工作区布局变化的真实帧验收（#134 / #602）

接续稳定节点拖动验收，增加真实工作区预设、尺寸、最大化和标签变化的缓存失效检查。没有改变生产行为或修改既有断言。开始前核对其他分支：#575 占用预渲染/引擎路径，#599 占用宿主参数动画，本批只增加 `panels/workload_tests.rs` 中的测试，不重复这些工作。

## 执行与结果

通过真实 Shell 的 `draw_commands` 和 `ui_frame`，中英文分别切换 Default、Animation、Nodes、Drawing、All 五种内置预设；每种验证尺寸变化、合成面板最大化、恢复，以及存在多标签 dock 时改变 active。预设和最大化使用实际 Workspace API，标签直接修改实际树的 active 字段；不是物理点击验收。

共 **48 次布局变化、96 帧**：每次变化的首帧只求解一次，随后的相同布局帧求解零次，chrome 均一次。缓存返回的所有 panel、tabs、content 与未缓存的原求解器逐项精确一致；整个过程没有编辑 Shell 的工程状态。没有在各次变化前手动清空缓存以掩盖失效问题。

新增用例先在开发分支通过。负对照实际移除缓存 get 的布局/尺寸身份检查，使其错误复用前次结果；测试在 `En/Default/resize` 失败，exit 101，求解数 0 而非 1。之后逐字节恢复 `frame_cache.rs`，保存恢复哈希并重新编译。

进一步构造 main `67040e8ad02ac60618842c1b9cc49193fac42c90` 与本批功能代码的组合。Git fetch 的代理连接失败、直连连接重置后，改用 GitHub API 元数据和已有本地 Git blob：导入 47 个主线变动，重建树与远端 main 的 `53d21ed0df9df460a7cdfca621ba8fb281485097` 完全一致，没有缺失 blob。使用该精确树的本地快照提交作集成，不冒充远端 main 的提交历史。47 个文件与 #601 的 9 个文件不交叠，也未修改本批节点/面板计数入口；实际组合提交和树见 `integration-input.json`。

最终组合运行 **209 项互不重复的定向测试通过、0 failed、0 ignored**：节点 183、布局缓存 4、面板 13、呈现交互 7、稳定拖动 1、新布局变化 1。另有 48 个稳定拖动样本继续保持静止/拖动绘制数量相等。相同测试程序的哈希记录在三份执行收据中。带 FFmpeg 的测试与正式 dev 程序均构建成功。正式程序使用子进程独立 LOCALAPPDATA 启动 `--command-list`，退出 0，stdout 是完整 JSON；没有改用户配置。

首次组合构建在最终 motion-desktop 测试 crate 的 rustc 中 `STATUS_ACCESS_VIOLATION` 退出，没有运行测试。保留 jsonl/stderr，确认进程终止并检查资源后，仅将最终测试 crate 的 codegen-units 从 8 改为 1，源码不变，test、debug assertions 和 overflow checks 仍开启。后续构建及上述测试通过，不声称修复了编译器。默认用户数据目录下的首次命令列表启动返回 OS PermissionDenied(5)；保留 stdout/stderr，首轮收集器未保存精确退出码。隔离 LOCALAPPDATA 后通过不能证明默认目录权限问题已修复。

## 审计与限制

| ID | 分类 | 证据与边界 |
|---|---|---|
| WL-INVALIDATE | 新验收 | 实际布局变化后求解一次、稳定帧零次，所有返回几何与原求解器相同。 |
| WL-NEGATIVE | 真实失败路径 | 缓存身份检查移除后拒绝，源码逐字节恢复后最终组合重新构建通过。 |
| WL-INTEGRATE | 集成验收 | 精确 main 树与功能代码的组合通过 209 项定向回归；不是整个应用/仓库全套测试。 |
| WL-OBSERVE | 既有边界 | 仅 cfg(test) 的有界计数；没有新增生产日志、观察键、错误回复、重试、线程或超时。原错误与 panic 语义保持。 |
| WL-PENDING | 未完成 | 精确最终提交 CI、main 自身 CI、原生 Present/GPU、物理输入、帧耗时/内存峰值，以及变化帧卡片和全部 #134 验收仍待完成。 |

布局在帧前改变，测试不涵盖帧中输入继续改变布局的双几何阶段，也不承诺缓存拒绝、忙碌、销毁或不合法输入时只求解一次。稳定绘制数相等不等于所有拖动类型、全部绘制数据或 FPS 相等。原按下/释放/视口变化的卡片重建断言保持。

没有用户可见 UI、尺寸、颜色、命中范围、资产、依赖或格式改动。DESIGN §0.7 的五项路径、五项效果、时间轴列坐标与吸附色全部 retained with reason：未编辑对应实现。不将本测试视为 §12 全面视觉验收。

命令、构建元数据、原始失败、程序与源文件快照见 `workspace-layout-transitions-evidence/{manifest.json,evidence.zip}`。原 `workspace-frame-costs-evidence` 是 `f954ed85804fe469f9e9083d78c3930f1468570b` 阶段的 208 项验证快照；本包包含新增测试后的当前源码和主线组合的最终 209 项结果。整个 #134 保持开放。
