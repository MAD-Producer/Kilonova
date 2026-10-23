# 属性显示快捷键保留分离轴（#158 子项）

基于 main `954c3dee`，分支 `codex/reveal-selected-axis`。开始前检查开放 PR #425/#428/#433 和所有工作树 timeline_shortcuts.rs 的未提交选轴/显示逻辑差异，没有相同修正；#432 仅处理原生捕获夹具。根工作树和其他工作树未提交文件保持原样。

## 行为与验证

原实现显示指定属性时总取 property_rows 的第一个通道。用户已选择 Position Y 的关键帧，按 P 后会改成 X，并清掉原来的关键帧选择。现在只有经现有 track_selection 验证、属于同一工程/对象/属性的当前通道才沿用原轴；不同属性、失效工程或通道仍使用合法的初始轴。没有跨工程继承轴号，也不改变属性显示/隐藏规则、默认键位、编辑/菜单优先级或其他上下文的 Delete 语义。

修改前实际探针以 `Some(X)` 与预期 `Some(Y)` 的状态差异失败，exit 101；另一个失效上下文探针通过。原日志和原探针源码保存在 `.tools/reveal-selected-axis/original-axis-regression.log`、`original-reveal-axis-tests.rs`。首个状态断言失败后的操作不算已执行证据。

修复后完整相关套件 **43 通过、0 失败、0 忽略**，原有 39 项断言未修改。新增四项测试包含：

- 中英文、时间轴/曲线、主窗/浮窗 owner、三个轴和 F9/Delete，共 48 种组合。使用实际 keyboard::route、真实核心编辑命令与 undo；明确检查缓动的批命令和删除命令都指向原轴/原合成时间，其他轴及未删除关键帧保持，撤销恢复原工程。
- 属性面板的 P：两种语言、主窗/浮窗 owner、三个轴，共 12 种组合，保留当前轴和关键帧。这里只验证其显示操作，不扩大既有 Delete 支持范围。
- Scale/Rotation 共六种合法轴选择；不同属性与工程切换的两种失效上下文不会复用旧轴或旧关键帧。

F9 的原协议是两个区间的 EditBatch，Delete 是单个 DeleteKey；最终探针按真实协议验证，不改变生产协议。实际输入路由和真实模型/撤销结果证明功能行为；这些 owner 参数验证不是物理原生窗口或操作系统输入法验收。没有视觉布局、尺寸、颜色、命中区、依赖、资产或工程格式变化。

最终日志 `.tools/reveal-selected-axis/final-timeline-shortcuts-expanded.log`，正常产品 cargo check 通过（19 项既有警告），结果另存 `check.log`。首次 42 项通过属于补充 Scale/Rotation 探针前的源码阶段，不与最终 43 相加。远程 CI 待本 PR 执行。

## 观测审计

| ID | 分类 | 处置与证据 |
|---|---|---|
| RVA-SELECTION | 功能边界 | 只沿用已验证的同一通道；48 种真实编辑/撤销组合与 12 种属性路由组合证明原轴保持。 |
| RVA-STALE | 正常分支 | 工程/属性变化、缺失通道是正常选择失效，沿用现有默认通道处理，不增加错误、计数或重试。旧工程/属性探针及既有上下文测试通过。 |
| RVA-HOST | 既有边界 | 不改宿主错误、线程、in-flight、panic、日志、Present 或协议；P 不发工程编辑，F9/Delete 仍沿用原类型命令。没有新增观测项。 |
| RVA-NATIVE | 验证范围 | 使用生产路由与真实核心，不冒充物理窗口、全部平台/DPI 或新增的完整默认错误信道验收。 |

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md；没有新增素材或兼容名称。

| 基线行 | 处置 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：未修改布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件或命中区。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值字段。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔控件。 |
| effect_controls 行高/步距 | retained with reason：未修改效果行。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果控件。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，未修改时间轴布局。 |
| timeline snap 指示线 | retained with reason：未修改主题或拖动反馈。 |

实际交互/数据证据如上，不把编译或十二项处置视为全部视觉基线整改。#158 的其他剩余项继续开放。
