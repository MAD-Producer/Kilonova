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
