# #238 位置与等间距分布验收

## 范围与分支

核对开放 PR、近期本地分支及相关工作树后，未发现其他分布实现。基于 #365（feat/layer-edge-alignment）建立 feat/layer-distribution，作为堆叠 PR；不复制媒体、缓存、快捷键或 CI 分支的改动。没有新增依赖、第三方资产或兼容名称，已核对 PROVENANCE、ATTRIBUTION 和 NOTICE。

对齐面板增加水平/垂直位置分布与等间距四个真实动作。至少三个图层；位置按求值边界的中心、间距按左/上边缘排序，相同坐标按对象 ID 排序，与点击顺序和主选无关。排序首尾固定在原合成位置。不同尺寸、旋转与锚点使用现有 selection_geometry 轮廓，不是透明像素扫描。

中心分布均分首尾中心；间距分布扣除中间图层的实际宽/高，再均分剩余距离。允许负间距重叠；若固定首尾和等间距会反转声明的边缘排序，则整组明确拒绝。这是约束不可同时满足的功能边界，不通过来回改变首尾对象或重复执行求解。重新求值仅有 f32 投影舍入差时不写关键帧或历史。

只改变指定合成轴。旋转父级下会换算为必要的局部 X/Y，补偿所选祖先的移动，包括父级位于中间、子级为固定端点的情况；另一条合成轴、Z、非位置轨道和时间偏移保持。共享 #365 的整数帧、完整规划后单次批量提交、拒绝恢复与单步历史语义。

## 观测与协议边界

| 编号 | 处置 |
| --- | --- |
| DIST-01 既有观测 | distribute_layers 接入既有 serial worker 的 Info start / in-flight / completion / panic / reply 路径。不是新增日志设施，不改 Present。输入受 MAX_LAYERS 约束，不记录完整工程或用户文本。 |
| DIST-02 实际故障 | 缺失对象及无法维持边缘顺序均由真实规划器经 worker 回复。验证默认文件和 Error stderr 同时含原始原因、operation，stdout 无日志；拒绝时工程、revision、undo、redo、frame 不变。 |
| DIST-03 正常分支 | 不足三层禁用、菜单取消、重复执行无可表示移动属于正常分支。没有新增重试、线程重启、超时政策、计数器或清错策略。 |
| DIST-04 功能边界 | 复用既有位置编辑及批量事务；分离轨道只写必要位置分量。原生截图临时隐藏旧示例、添加三层并进行实际编辑，结束撤销整个夹具；保留原有工程/历史断言及主失败加清理失败传播。 |

## 验证证据

- host 初轮 7 通过、1 失败：新增重复执行断言发现 f32 重算产生重复历史；通过与投影量级相关的舍入界限修正，原断言保留。之后 9 项定向通过，包含原 #365 的 5 项；另补充移动父级与固定子端点、极端负间距拒绝，最终结果另记。
- 四个模式 × 分离位置 × 父级 × 选择顺序的实际重采样几何、固定首尾、另一合成轴及非位置轨道；真实工程保存重开、一次撤销/重做；时间偏移动画只更新当前局部键，保留键时刻和其他帧。
- desktop 首轮截图辅助代码漏导入 Property，已补齐。交互首轮 4 通过、1 失败：350 高测试窗口的按钮在可视区外；新增实际 wheel 滚动及复位，再点击四个按钮，主窗/浮窗、中英文均通过，5 项总计 14.31 秒。未改变既有对齐断言。
- 真正 worker 的坏选择、成功恢复、重复执行不增 revision、保存重开及默认日志分流通过；后续扩展极端顺序拒绝的实际失败用例。不是手动制造 Err 或调用 respond。
- 原生带 FFmpeg 构建通过，保留 21 条已有警告。首轮实际截图展示负间距；为便于比较，又将两个端点恢复到分开的初始位置，临时隐藏旧示例，截图结束恢复全部工程与历史。native-verified/ 的中英文各五幅真实图覆盖分布前、四个实际动作，capture-verified.log 通过。该组截图在后续顺序拒绝 guard 之前生成；正常正间距动作及界面未变，最终 guard 的构建和真实故障另外验证，不混同证据范围。
- 在顺序拒绝 guard 之前，完整 host 回归 59 通过、0 失败（70.05 秒），完整 desktop 回归 616 通过、0 失败、1 项既有音频设备忽略（400.14 秒）。证据 host-complete.log、desktop-complete.log；后续 guard 与其真实故障用例定向结果另外补记，不把前一轮完整回归当作最新代码全套重跑。保留 host-initial.log、desktop-initial.log、desktop-verified.log 等失败记录，不删除旧证据或削弱断言。
- 最终含 guard 的带 FFmpeg 原生构建成功（27.94 秒，21 条既有警告），日志 native-order-guard-build.log。来源守卫 1,530 tracked paths、面板样式消费者及差异检查通过。
- 最终 guard 定向验证：host 9 通过（5.94 秒），desktop 5 通过（15.87 秒）。真实 worker 除缺失对象外实际触发 reversed edge order，确认拒绝不改工程/历史/时钟，原始原因同时进入文件及 Error stderr。文件严格核对四条 distribute_layers start；stderr 无 start，stdout 无日志。证据 host-order-guard.log、desktop-order-guard.log。没有将此次定向结果相加为一次完整回归。

## DESIGN §0.7 逐项处置及 §12

| 基线 | 处置 |
| --- | --- |
| path_controls 六处行推进 | retained with reason：不修改路径排版。 |
| path_controls 双列分配 | retained with reason：不修改路径字段几何。 |
| path_controls 混合按钮高度 | retained with reason：不缩小原命中区。 |
| path_controls 数值背景 | retained with reason：不修改数值绘制。 |
| path_controls 全宽布尔按钮 | retained with reason：不修改路径布尔控件。 |
| effect_controls 行高与推进 | retained with reason：不修改效果行。 |
| effect_controls 布尔文本按钮 | retained with reason：不修改效果控件类型。 |
| effect_controls 全宽枚举 | retained with reason：不修改效果枚举。 |
| effect_controls with_alpha 悬停 | retained with reason：不修改该分支。 |
| effect_controls 树缩进 | retained with reason：不重写效果树。 |
| timeline 列坐标 | retained with reason：保留既有坐标和交互。 |
| timeline 吸附线颜色 | retained with reason：不修改拖动反馈。 |

新分布动作复用原生按钮与现有尺寸/颜色 token；没有 switch/enum 冒充按钮、虚构 token、属性额外行或新右键菜单。短窗实际滚动和双语主/浮窗逻辑路由有测试，DPI 1 的实际 GPU 截图有证据。高 DPI、其他 OS 人工输入及 OS 跨屏焦点仍未覆盖，不声称 DESIGN 整套合规。

## 父分支与原生验收修复组合（2026-10-20）

同步 #365 `3782379b0dfee1687cf4c1af1611840b867dd90a`，中文集成提交 `1f4c806`，保持 #368 → #365 → #366 的堆叠顺序。引入 17 个上游文件（含父分支验收记录），没有改分布规划、动作/错误通道、面板、菜单或 token；仍要求整组原子拒绝及一步历史。

本次组合定向验收：host 对齐/分布 **9 通过，11.16 秒**；中英文主窗/浮窗路由与真实 worker 失败/恢复 **4 通过，24.75 秒**；原生四模式采样 **1 通过，37.57 秒**；中英文 PNG 导出 **1 通过，15.12 秒**；工作流 **5 通过，6.180 秒**。证据 `artifacts/pr-queue/distribution-native-ci-{host,desktop,sampling,frame-export,workflow}.log`。来源守卫（1537 跟踪路径）、面板样式消费者及差异检查通过。

本次没有重跑整套桌面/渲染或 FFmpeg 发布构建，旧 head 的完整套件与截图保留原范围；此次无用户可见 UI 变化，不重拍旧界面。最终远程组合 CI、高 DPI 与其他 OS 人工交互仍待验证。父 PR 合入 main 后再核对/调整 base；原采样递增断言、PNG 等待与分布顺序拒绝断言均未改。

本地 Git 传输连接重置且 HTTP/2 连接失败后，改用 Git 数据接口发布。仅将两个未发布提交的时间时区统一为 UTC，并同步本段集成提交引用；作者、时刻、中文消息、父关系与已验证的产品文件内容保持一致。发布前逐项比较树和对象哈希，远程仅快进，不改已发布历史。原网络失败与发布对象核对记录位于 artifacts/pr-queue/。

## Current alignment-parent integration

Local candidate `27d603d6` cleanly merges updated alignment parent `18194c7a`
and matches preview tree `fb091bed`. Relative to the new parent, the same ten
feature files change; every added/deleted feature line matches the original
distribution delta. The integration inherits real-main preferences, property
reveal, grapheme editing, image-trim reuse and the reviewed CI/native helper.
It introduces no new distribution behavior, layout, style or menu changes.

Fresh tests passed: host alignment/distribution 9 (9.07 seconds) and desktop
controls, real worker failures/recovery and position regression 5 (20.40):
fourteen cases, no failures or ignores. Provenance (1,559 tracked paths),
style-consumer and whitespace guards passed. Evidence is retained in
`.tools/issue-closure-review/366-current-parent-{integration,acceptance}.json`
and `artifacts/pr-queue/366-current-parent-{host,desktop}.log`.

This candidate and its following documentation-only commit remain local while
#365's current-head checks run. Its historical screenshots and full-suite
results keep their original source scope. No new full-suite, native capture,
packaged video or other-platform manual run is claimed. After #365 reaches
main, recheck the real squash/main tree before updating this existing PR and
requiring its own fresh remote checks. No remote head was changed in this step.

## Actual main cache and registry integration — 2026-10-20

Local source commit `76fb32ba1d811c822419482601ab3cc7a61c72a5` has tree
`b359aeeb000a3ba067224e6b0ba57ee6efc0a39e` and incorporates actual main `44a5a0d2a9f8bc8d28ee66de786a17488c66cf33`.
The clean preview and actual merge agree. Exactly fourteen inherited paths
match the already merged cache, dependency-walk and session-registry blobs;
every other blob matches the prior held candidate. No remote head changed.

Fresh default-level acceptance passed 14 targeted cases:

- host: 9 passed, zero failures/ignores, 7.01 seconds; `366-main-host.log`
- desktop: 5 passed, zero failures/ignores, 18.73 seconds; `366-main-desktop.log`

The nine host cases cover alignment/distribution geometry, whole-batch
rejection, local-time animation and history/storage behavior. The five
desktop cases cover bilingual controls in both hosts, actual worker
failure/recovery and the position-component regression. No feature,
layout, menu, token or assertion was changed by this integration.

Provenance (1,566 tracked paths), style-consumer and whitespace guards
passed on the source integration. Exact-head integration and acceptance
records are `366-current-main-{integration,acceptance}.json` under
`.tools/issue-closure-review`. Builds, temporary files and logs remain on E;
`MOTION_LOG` was unset. This following commit changes only this document.

The candidate is still unpublished. Actual parent squash reconciliation,
final PR base/head verification and all six final-head checks remain
required. Historical screenshots keep their original source scope; this
update claims no new full-suite, physical-input/DPI, cross-platform manual
or packaged-video execution.


## Diamond main integration acceptance — 2026-10-20

Actual main `c5f81223` is integrated in tested source `9b3b75ab4192507ec689bfaadf0ae04339cb11c7`.
Compared with the previous held candidate, the only incoming code/test changes
are the exact three already merged diamond files; their implementation and
regressions match main byte for byte. Incoming completion documentation retains
its own earlier evidence scope. Other feature code and assertions are unchanged.
The complete shared CI document is retained when resolving its add/add conflict.
This is a local candidate and has not been pushed to the published PR.

With MOTION_LOG unset, 14 targeted tests passed with zero failures/ignores:
- `366-main348-host.log`: 9 passed (5.05s)
- `366-main348-desktop-recovered.log`: 5 passed (18.06s)

The first desktop compile failed before tests because the reused non-test
motion-host library lacked ops::alignment despite the source declaring it and
the nine host cases passing. Verbose output marked that library Fresh and the
old dependency record did not include alignment. The guarded package-only
motion-host refresh removed 330 cache files (3.5 GiB); the unchanged source then
passed all five panel cases. A brief compiler-activity guard deferred the first
cleanup attempt rather than touching an active build. Failure, verbose and
cache-refresh logs remain under `artifacts/pr-queue/366-main348-*`.

Provenance (1,568 tracked paths), panel style-consumer and whitespace guards
passed. Builds, temporary files and retained logs remain on E. The full suite
and historical visual acceptance above keep their original source boundaries;
no new complete application screenshot or full-suite run is claimed here.
The following documentation-only commit does not change tested source. Actual
parent squash reconciliation and all six final-head remote checks remain required.
