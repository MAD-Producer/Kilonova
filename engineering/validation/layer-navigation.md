# 图层选择与排序验收

对应 #208；基于 #362 的 `5b83dc9` 堆叠。开始前核对现有 worktree、未提交文件和开放 PR 文件列表；保留 #349 的属性筛选、#365/#366 的空间对齐与分布，以及其他代理分支的独立改动。

## 行为与边界

- Ctrl+上/下沿当前筛选后可见的图层标题移动选择，Shift 扩展或反向收缩连续范围。展开的属性不会成为图层导航目标；导航保留展开状态并滚动显示目标标题。当前选择不可见时，下从首项、上从末项开始；空结果不改选择。兼容虚拟摄影机 ID 0 不作为可选图层。
- Ctrl+Alt+上/下上移、下移，追加 Shift 置顶、置底。图层菜单和右键“排列”共用同一规划器、当前多选和有效快捷键提示；所选、未选两组都保留各自相对顺序。
- 锁定、只读、空选择、失效 ID 不产生部分修改；边界不产生空撤销。真实变更通过现有 InComposition 命令和带 revision 的单次异步批处理提交。工程切换清除待定位目标。
- 文本、IME、节点、菜单、活动手势和焦点丢失保持既有优先级。图层导航只在时间轴上下文生效，排序允许时间轴或合成上下文；左右逐帧键保持原语义。
- 版本 1 设置/预设显式占用新增默认键时，加载保留原映射并解除对应新命令；交互绑定和恢复默认仍严格检查冲突，不隐式抢占用户键。

## 验证证据

- 完整桌面回归：**642 通过、0 失败、1 个既有音频设备测试忽略，367.66 秒**；`artifacts/pr-queue/layer-navigation-full-tests.log`。含旧映射迁移、全部 1..7 层选择子集和四种排序的穷举、真实批处理回滚/撤销/重做/保存重开、筛选范围反向收缩、展开属性下滚动定位和主窗/浮窗路由。
- 带 FFmpeg 原生开发构建通过，63 秒，21 条既有警告；`artifacts/pr-queue/layer-navigation-build.log`。没有为本改动增加依赖或资产。
- 原生 GPU 子进程真实绘制中英文选择、图层菜单、右键根菜单和排列子菜单，共 8 张截图；`artifacts/layer-navigation/native-final/`。菜单按边界禁用命令，未重新设计既有浅色右键菜单。
- 最终二进制的真实 Windows 键盘/前台窗口验证：英文主窗、中文浮动时间轴分别点击选中首项，Ctrl+下选择下一层，Ctrl+Alt+下改变真实工程顺序，Ctrl+Z 单次恢复。MCP 只读取实际工程/历史；未用协议编辑替代物理输入。两窗口正常退出，证据 `artifacts/layer-navigation/os-focus-final/summary.json`、各自 `ordered.png`、协议 stdout 和 stderr。
- 首轮 OS 检查遗漏必需面板的归属，触发既有布局校验回退，点击坐标未命中。保留 `os-focus.log` 与原截图；补齐隐藏面板归属后复验通过，没有更改产品校验或放宽断言。中间辅助脚本因默认 GBK 读取 UTF-8 失败，保留记录，不计为产品成功。

## 错误和可观测性清单

| ID | 分类 | 实际证据与限制 |
| --- | --- | --- |
| NAV-01 | 既有覆盖/真实失败 | 原生引擎先实际推进 revision，再提交旧快照排序。原始 revision 错误保留，工程、revision、历史、canUndo/canRedo 不变；默认 Info 文件和 Error stderr 都包含原始原因，stdout 不含该原因或初始化日志。`native-final/expected-error.txt`、`motion-studio.log`、`stderr.log`；父测试实际断言三路输出。 |
| NAV-02 | 正常分支 | 空选、边界、锁定、只读、已失效选择通过既有提示或无操作返回，不当作系统故障、也不生成空历史。针对性测试检查真实命令列表。 |
| NAV-03 | 功能/协议边界 | 排序复用现有 serial edit worker 的 start、in-flight、耗时和异步错误渠道，动作上下文标记 `reorder_layers`；缺少工程身份、revision 或损坏排序快照向原动作错误边界返回 Err。没有新增线程、重启、重试、阈值或热路径日志。 |
| NAV-04 | 保留限制 | 未新增观测计数器、panic hook 或阻塞操作；本次未重做既有 worker 的受控阻塞验收，不能据此声称全仓库可观测性完成。 |

## DESIGN §0.7 逐项处置

| 基线位置 | 状态与原因 |
| --- | --- |
| path_controls 六处行推进 | retained with reason：本次不改路径行节奏。 |
| path_controls 双列分配 | retained with reason：不改字段几何。 |
| path_controls 混合按钮高度 | retained with reason：不缩小命中区。 |
| path_controls 自绘数值背景 | retained with reason：不改数值控件。 |
| path_controls 全宽布尔按钮 | retained with reason：不改布尔控件。 |
| effect_controls 行高与推进 | retained with reason：不改效果行排版。 |
| effect_controls 布尔文本按钮 | retained with reason：不改效果控件。 |
| effect_controls 全宽枚举 | retained with reason：不改枚举。 |
| effect_controls with_alpha 悬停 | retained with reason：不改悬停绘制。 |
| effect_controls 树缩进 | retained with reason：不重写属性树。 |
| timeline 列坐标 | retained with reason：保持兼容基线，定位只消费既有行计划与 row.height，不改列宽。 |
| timeline 吸附线颜色 | retained with reason：不改拖动反馈。 |

§12 局部验收：没有新颜色/尺寸字面量、token、前端栈、工程格式或历史模型；菜单复用既有控件。实际中英文原生截图及 Windows DPI 1 的主窗/浮窗物理输入已验证；全套 19 面板、所有 DPI、macOS/Linux 人工交互及性能基线仍 pending verification，不能把本次通过表述为整个 DESIGN 全项完成。Provenance、样式消费检查和 diff 检查另随提交记录。

## 快捷键父分支与原生验收修复组合（2026-10-20）

同步 #362 `e2b491b569b81b119c0999a935ba6a84763f9ac0`，中文集成提交 `c44d2cc`。相对原 head 只有八个上游文件：工作流、采样/PNG 测试、三个父级验收记录及 Intel 预算/工作流回归；图层导航、排序、菜单、快捷键映射和 token 均未改。依赖顺序保持 #368 → #357 → #362 → #371 → #373。

实际组合定向验收：导航 **7 通过，12.73 秒**（含真实原生绘制、过期 revision 故障及默认日志分流）；快捷键映射 **6 通过，0.10 秒**；原生四模式采样 **1 通过，45.49 秒**；中英文 PNG 导出 **1 通过，13.58 秒**；工作流 **5 通过，7.314 秒**。日志为 `artifacts/pr-queue/navigation-native-ci-{feature,bindings,sampling,frame-export,workflow}.log`。来源守卫（1547 跟踪路径）、面板样式消费者及差异检查通过。

本次没有重跑完整桌面或 FFmpeg 发布构建；旧 head 的 642 项完整回归、物理键盘验证与已展示截图保留其历史范围。本批没有用户可见样式改动，不追加视觉验收主张，也不以定向结果替代远程完整 CI。高 DPI、其他 OS 人工交互及最终组合 CI 仍待验证；父 PR 合入 main 后再核对/调整 base。

常规 Git 传输连接失败后，本次通过 Git 数据接口发布。仅将两个未发布提交的日期时区表示统一为 UTC，并同步本段集成提交引用；作者、时刻、中文消息、父关系与已验证的产品文件内容保持一致。发布前逐项比较树和对象哈希，远程仅快进，不改已发布历史。网络失败和发布对象核对记录位于 artifacts/pr-queue/。

## Local parent refresh after main squashes

The local review candidate now incorporates #362 candidate `57ff99cb` at
`1c1d186`. The clean preview and actual merge both produced tree
`4512378d3da93069bb96c98128cd20df5686c60d`. Eighteen inherited files bring in
the shared native test helper, CI routing, grapheme edit fix and icon validation.
The layer navigation, keyboard router, panel fields and shortcut catalogue
remain byte-identical to the previously tested navigation candidate.

Fresh default-level local tests passed: seven layer-navigation cases in
9.37 seconds, including native bilingual menus and stale-revision observation,
and all 39 timeline shortcut cases in 2.43 seconds. Logs are
`artifacts/pr-queue/371-current-navigation.log` and
`artifacts/pr-queue/371-current-shortcuts.log`; the exact-head record is
`.tools/issue-closure-review/371-current-parent-acceptance.json`. All build and
temporary outputs use E. No production assertion, timeout or style was changed.

This integration remains local. Remote #371 still has head `26d7557b` and
requires its own final parent/main integration and six current-head checks.
The earlier retained screenshots remain historical visual evidence; this
update does not claim a new physical-input, DPI or complete GUI matrix run.


## Published shortcut-parent reconciliation — 2026-10-20

The held local candidate now incorporates the actual published #362 head
`de8ff26f74fcfdd72dbbca80a50bc5dcf3d115db`, rather than its earlier local
candidate. The clean merge changes exactly four inherited files: the three
already merged #370 renderer/test/completion blobs and the parent's newer
shortcut validation document. Each incoming blob matches the parent exactly.
All other files, including every desktop source, navigation regression,
shortcut guide and native test helper, match the previously tested local
navigation candidate `fccfcbe78bc0d4e28d92cf9f7e236bfcf916eac6` byte for byte.

This paragraph adds documentation only. The existing seven navigation and
39 timeline shortcut test results retain their recorded source scope; no
fresh execution is claimed here. #370's actual main integration and its six
successful checks are separate evidence for the inherited renderer change.
The navigation candidate remains unpublished until the actual #362 squash,
main reconciliation, final PR targeting and current-head CI are verified.

## Actual main cache and registry integration — 2026-10-20

Local source commit `3747016b232d406ca6d5a220dfc117399b4df2b2` has tree
`1213d6fc1b2b43d8821d0ad822df047cfc8db8aa` and incorporates actual main `44a5a0d2a9f8bc8d28ee66de786a17488c66cf33`.
The clean preview and actual merge agree. Exactly fourteen inherited paths
match the already merged cache, dependency-walk and session-registry blobs;
every other blob matches the prior held candidate. No remote head changed.

Fresh default-level acceptance passed 46 targeted cases:

- navigation: 7 passed, zero failures/ignores, 11.80 seconds; `371-main-navigation-recovered.log`
- shortcuts: 39 passed, zero failures/ignores, 2.75 seconds; `371-main-shortcuts.log`

The navigation source remains unchanged. The seven navigation cases include
native bilingual menu rendering and a real stale-revision worker failure;
the other 39 cases exercise timeline shortcut routing. The first build
stopped before any tests: Cargo marked an old `motion-ui` library fresh
despite missing key variants present in this worktree. The failed build,
verbose dependency diagnosis and package-only E-drive cache refresh are
preserved in `371-main-{cache-diagnosis,ui-cache-refresh}.json` and the
corresponding raw logs. No source or assertion changed during recovery.

Provenance (1,571 tracked paths), style-consumer and whitespace guards
passed on the source integration. Exact-head integration and acceptance
records are `371-current-main-{integration,acceptance}.json` under
`.tools/issue-closure-review`. Builds, temporary files and logs remain on E;
`MOTION_LOG` was unset. This following commit changes only this document.

The candidate is still unpublished. Actual parent squash reconciliation,
final PR base/head verification and all six final-head checks remain
required. Historical screenshots keep their original source scope; this
update claims no new full-suite, physical-input/DPI, cross-platform manual
or packaged-video execution.


## Diamond main integration acceptance — 2026-10-20

Actual main `c5f81223` is integrated in tested source `854c569ba8c166f59516b0652da5730e77e77e68`.
Compared with the previous held candidate, the only incoming code/test changes
are the exact three already merged diamond files; their implementation and
regressions match main byte for byte. Incoming completion documentation retains
its own earlier evidence scope. Other feature code and assertions are unchanged.
The complete shared CI document is retained when resolving its add/add conflict.
This is a local candidate and has not been pushed to the published PR.

With MOTION_LOG unset, 46 targeted tests passed with zero failures/ignores:
- `371-main348-navigation.log`: 7 passed (7.19s)
- `371-main348-shortcuts.log`: 39 passed (2.34s)

Provenance (1,573 tracked paths), panel style-consumer and whitespace guards
passed. Builds, temporary files and retained logs remain on E. The full suite
and historical visual acceptance above keep their original source boundaries;
no new complete application screenshot or full-suite run is claimed here.
The following documentation-only commit does not change tested source. Actual
parent squash reconciliation and all six final-head remote checks remain required.


## Combined distribution and navigation acceptance — 2026-10-20

Unpublished source `c78ef6e94e3f228b6c30bc078e78fe4c7b133e60` integrates the final published distribution
candidate and the actual main ancestry of the alignment and shortcut squashes.
Its non-validation blobs exactly match reviewed navigation tree `ddbd2618`.
The resolved action handler retains ordering, alignment and distribution;
keyboard documentation retains all features without duplicated introductions.

With MOTION_LOG unset, 51 selected cases passed with no failures or ignores:
- `371-combined-parent-navigation.log`: 7 passed (10.44s)
- `371-combined-parent-shortcuts.log`: 39 passed (3.38s)
- `371-combined-parent-align.log`: 5 passed (18.03s)

These groups cover layer navigation, atomic ordering and save/reopen, shortcut
input ownership, bilingual main/floating-host alignment/distribution, and real
worker failure/recovery with the default file/stderr/stdout channel contract.
Provenance, panel style-consumer and whitespace checks passed. Source, assertion,
timeout and ignore settings remained unchanged during validation; no cache was
deleted in this run. Build output and logs remain on E. This selected execution
does not add a new full-suite, complete native screenshot or other-platform
manual claim to the historical evidence above.

The candidate remains local while final distribution CI runs. Reconcile the
actual distribution squash/main, publish a concrete main-based head and require
six successful checks for that head before merging. Historical green checks on
the earlier feature-parent branch are not final-head acceptance.


## Final actual-parent reconciliation — 2026-10-20

Source `ae57060e21b91ced3964276443d49db022fd0510` integrates the verified actual parent squash/main
`a133219b254825abff1d845d1087e4ca8d8f1140` with real two-parent ancestry. All application and Rust-test blobs remain
identical to the 51-case tested source `c78ef6e94e3f228b6c30bc078e78fe4c7b133e60`
above. Shared CI tooling follows reviewed policy `766b1e96b5d1aabed3334e3303b7cd07c9db7c90`; its six
metadata/tool files and validation records are outside that product identity
claim. The normal remote CI commits are retained through `008ecd80262557d8d0c8e116fdadde38a035926a`.
The documented prospective squash rehearsal was rechecked against the actual
commit. No implementation, interaction, assertion, timeout or ignore changed.

This following documentation-only head is published directly against main.
Its own four successful native-policy checks are required before merge; old feature-parent
green results are historical evidence. Earlier full-suite, native screenshot,
physical DPI and other-platform manual results retain their stated source
boundaries. Exact reconciliation and publication proofs remain under
`.tools/issue-closure-review/371-final-main-*.json`.
