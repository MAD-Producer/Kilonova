# #216 关键帧选择集合与批量编辑进度

本阶段建立明确的“类型化通道地址 + 合成帧号”选择集合和主选键，接入批量 Delete、F9、插值及剪贴板命令边界。**#216 尚未完成**：鼠标追加／切换、框选、选择外观及普通构建的真实系统输入验收仍待完成。本记录不是 issue 关闭依据，也不把内部选择集测试称作已交付的鼠标功能。

实施前检查了远端打开的 PR 和本地工作树。`perf/effect-dropdown-keys` 处理效果枚举缓存；`feat/78-render-dag` 处理渲染结构，本阶段没有修改其文件。工作在 `feat/keyframe-selection` 独立工作树，根工作树保持不动。没有审阅其他人的 PR，没有新增依赖、第三方资产、工程格式或旧版迁移。

## 功能边界

| ID | 分类 | 实现或限制 |
|---|---|---|
| KS-01 | 新功能基础 | 选择集按目标通道聚合有序、唯一的合成帧号。Replace／Add／Toggle 在完整输入和 16 MiB 元数据检查后修改集合；同一手势的重复命中只切换一次。主选键确定性保留。 |
| KS-02 | 文档身份 | 根目录、合成、文档代数变化使选择失效；权威修订变化重新检查原始已创作关键帧，视图变化复用检查。缺失修订时不缓存。选择不依赖当前可见／展开的行。 |
| KS-03 | 防止整层操作 | 空集合和失效集合继续持有关键帧所有权，Delete／F9 给出既有本地化提示，Copy／Cut 不降级到图层。图层锁定或观察模式禁止写入。显式图层／属性导航才释放集合。 |
| KS-04 | 原子批量编辑 | `EaseKeyframes`／`DeleteKeyframes` 使用既有类型化通道与事务。先检查所有目标、锁、分离轴、别名、偏移与关键帧，再编辑；失败不改变工程或历史。一次成功产生一个撤销项，子合成编辑不落到主合成。 |
| KS-05 | 快速缓动 | 复用原 F9 的曲线策略，按相邻区间汇总选中端点的入／出侧，再写入。相邻两个选中键不会彼此覆盖共享区间的一侧；未选中的侧、值、时间与空间切线保留。允许有效区间起点位于合成零点之前。 |
| KS-06 | 焦点与协议 | 集合先于旧的单属性 Delete 路由；文字、IME、菜单与正在进行的编辑继续优先。动作沿用 `EditorAction` 文档／修订戳，旧戳是正常取消，不记故障。原属性控制器仍负责主属性和既有拖动。 |
| KS-07 | 尚未接入 | 鼠标点选的追加／切换、时间轴和图表背景框选、选择轮廓、Ctrl+A 及主选属性投影仍未交付。旧鼠标控制器当前不会创建此集合；本阶段不能改善实际鼠标多选。 |

## 错误信道审计

| ID | 分类 | 证据与边界 |
|---|---|---|
| KO-01 | 既有观察复用 | 新编辑命令经过现有串行 `editor_action`：阻塞处理前发布在途状态和默认 Info start，失败保留原因、操作和对象，Error 到文件与 stderr，stdout 保留协议。没有新增 Present 热路径日志、重试、重启、超时或吞错分支。 |
| KO-02 | 实际失败 | `key_selection_edits_keep_original_failures_on_default_channels_and_recover_atomically` 启动独立子进程，实际提交有效首通道 + 失效第二通道，分别覆盖缺失关键帧、缺失对象、锁定图层及两种批量命令。检查六条原始 Error 的文件／stderr 对应与 stdout 隔离，工程和修订不变；随后成功提交、正常旧戳取消、一次撤销恢复。 |
| KO-03 | 新降级观察 | 原始快照的 `keys` 缺失／不是数组和二分查找遇到非整数时间均禁用编辑并进入既有有界 panel observer。它按进程生命周期、每个静态 site 累积，首次 Warn，60 次 Error，此后抑制；健康邻居不重置该生命周期计数。没有把正常空通道、缺失目标、普通关键帧未命中或取消算成故障。 |
| KO-04 | 实际降级 | 独立子进程在默认日志级别分别调用两种损坏快照 120 次，并穿插健康命中／未命中。断言每个 site 仅一条首次 Warn 和一条第 60 次 Error，Error 同时进入 stderr，stdout 没有诊断；正常空数组和失效地址不产生日志。运行结果见后续验证记录。 |
| KO-05 | 边界限制 | 模型与命令测试不证明真实鼠标、操作系统焦点、GPU 呈现或跨平台行为。尚未完成普通构建的主窗／原生浮窗中英文系统输入、框选取消、失效反馈和截图验收。没有改变既有断言来取得通过。 |
| KO-06 | 待接入观察 | 鼠标调用的 `choose` 入口为选择元数据预检失败保留原始原因并发布 Error 与本地化提示。目前未接入生产鼠标控制器，其实际 UI 故障和提示验收仍待 KS-07，不能用集合直接调用的原子性测试替代。 |

## 已完成的验证

- 核心 `cargo test -p motion-core --test keyframe_clipboard`：26 项通过，含相邻选中键两侧缓动、负合成起点、平面／嵌套矢量、蒙版、效果和颜色曲线，失败不变性、单次撤销／重做及真实存储保存重开。
- 桌面第一轮 `key_selection`：11 项通过，包含九个本阶段测试和两个既有匹配项；覆盖跨图层／分离轴／偏移、集合缓存与过期、空选择、主属性意图仲裁、双语言及主／浮窗命令路由和实际默认故障信道。
- 既有剪贴板 6 项、时间轴快捷键 48 项、动画菜单 7 项、键盘路由 9 项均通过。第一次误用 `panels::property_focus::tests` 过滤器匹配 0 项，不计作通过；正确的既有 focus 测试另行记录。
- 补充原始通道对照／默认降级信道后的 `key_selection` 第二轮：15 项通过。对照以真实有效 `Engine` 工程的模型捕获结果为准，不用模拟成功返回替代。随后修正锁定状态提示保留和节点焦点的早期路由边界，最终同组 15 项再次通过（18.20s）；正确 `property_focus` 过滤器的 9 项通过（11.36s），其中 1 项与前组重叠，不重复累加。
- `git diff --check` 与 `tools.ci_desktop.verify_style_consumers` 通过。没有把未使用的待接入 `choose` 入口告警隐藏；既有编译告警保留。

原始日志位于本地 `artifacts/pr-ci-followups/`：`keyframe-selection-core-first.log`、`keyframe-selection-foundation.log`、`keyframe-selection-routing-regressions.log`、`keyframe-selection-source-first.log`、`keyframe-selection-final.log`、`keyframe-selection-focus-regression.log`。最终故障子进程原件在 `keyframe-selection-actions-final/` 和 `keyframe-selection-source-final/`。首次私有类型导致的编译失败保存在 `keyframe-selection-set-first.log`，修正可见性后的日志为 `keyframe-selection-set-fixed.log`；没有删除失败原件。第一次回归启动使用 PowerShell `Tee-Object -LiteralPath ... -Append` 参数组合失败，没有执行测试，不计入结果。

基础代码提交为 `cf3a7dbb77d8886c074b55010633bed630be965f`。[机器可读结果](keyframe-selection-foundation-results.json) 记录每份原件的 SHA-256 与边界；[日志压缩包](data/keyframe-selection-foundation-cf3a7dbb.zip) 包含 15 个成员、34,683 字节，SHA-256 为 `a654f01b20053d38b4ad0bd7a2d85045300f99e266317adc1e7777d5ca586c27`，已逐成员校验 CRC。不包含运行时复制的效果包。

## DESIGN.md §0.7 与截图边界

此阶段没有修改任何绘制、行布局、颜色、尺寸、控件或右键菜单，因此没有新增样式截图。不能以本阶段编译或单元测试宣称 §12 视觉验收通过。

| §0.7 项目 | 状态 | 原因 |
|---|---|---|
| path_controls 六处行步进 | retained with reason | 本阶段不修改路径属性行。 |
| path_controls 两列分配 | retained with reason | 本阶段不修改路径属性行。 |
| path_controls 按钮与字段高度 | retained with reason | 本阶段不修改控件和命中范围。 |
| path_controls 数值字段外观 | retained with reason | 本阶段不修改数值绘制。 |
| path_controls 布尔按钮 | retained with reason | 本阶段不新增或修改属性开关。 |
| effect_controls 行高 | retained with reason | 本阶段不修改效果属性行。 |
| effect_controls 布尔按钮 | retained with reason | 本阶段不修改效果开关。 |
| effect_controls 枚举按钮 | retained with reason | 另一分支有缓存工作，本阶段不修改该文件。 |
| effect_controls hover alpha | retained with reason | 本阶段不修改绘制。 |
| effect_controls 缩进层级 | retained with reason | 本阶段不修改效果树。 |
| timeline 11 列位置 | retained with reason | 保持兼容基线，尚未接入选择绘制。 |
| timeline 吸附线颜色 | retained with reason | 保持既有吸附行为，本阶段未触及。 |

后续交付需先完成 KS-07，再验证普通时间轴／图表两种视图、主窗／原生浮窗、中英文、追加与切换、框选成功／取消、撤销与失效选择，查看多张实际截图。涉及工程改变的操作继续要求完整工程断言、一次撤销、保存及独立进程重开。
