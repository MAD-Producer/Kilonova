# 空选择不再匹配缺失标识的图层

关联 #134。原始基线 `5a8fdc4927493abe82b2e9b28d484b07f925ba50`；接续后的基线 `880739de0f7912b8b1683ce72593fad9035d6d24`，代码 `9ff7a5630e2ff3bb83eafa818547fb62e4bfcccd`。

## 问题与行为

`auxiliary::selected` 原来比较 `layer["id"].as_u64() == panels.selected`。空选择与缺失/非无符号整数标识会得到 `None == None`，错误返回该图层；真实效果控制面板随后在 `effect_controls.rs:854` 的 ID unwrap 处 panic。该复现针对不完整的面板 JSON 快照，不表示持久化模型允许保存缺 ID 工程。

现在先取得明确的所选 ID；没有选择直接返回 None，只匹配 Some(id)。合法选择、相同 ID 的首次匹配，以及删除/切换快照后的失效选择行为不变；没有默认选择第一个有效图层，也不把坏 ID 替成 0。沿用现有空选择提示，不发编辑动作，不新增样式、行或控件。

## 验证

- 新增三个回归，覆盖八种缺失/错误 ID、明确选中时跳过坏 ID 并保持首次有效匹配，以及中英文真实效果面板输出。16 个异常快照的绘制形状、颜色、字形和顺序必须与正常无图层提示逐字节一致，并且不发动作。
- 原实现：1 passed、2 failed；一项直接误匹配，一项实际面板绘制 panic，失败日志保留。补丁后原基线面板回归：536 passed、0 failed、0 ignored。
- 合入 #501/#504 的最新基线重新执行：550 passed、0 failed、1 ignored。忽略项是上游已有的手动原生关键帧拖动捕获，本次未修改、未运行、不计作通过。原有断言不变，三个新回归源文件在前后运行间一致。
- 当前精确源码的英文默认窗口真实 GPU 属性验收通过，93.92 秒，独立 240 秒本地预算。原尺寸、项目/历史还原、工作区呈现与 F9 真实编辑检查沿用原检查器。该运行超过原属性进程 90 秒门限，不能算原 40/90 秒完整脚本通过。

## 失败边界与审计

- EMPTY-SELECTION-01 功能修复：不完整快照中的无效 ID 不再被空选择误认作选中。返回值边界有意修正为正常无选择；没有 catch_unwind、重试、错误清除或默认对象 ID。
- EMPTY-SELECTION-02 新观察：无。空选择和删除后的失效选择是正常分支，不新增每帧 Warning/Error；工程/协议层的数据验证仍由既有边界负责，没有宣称新增坏工程诊断。
- EMPTY-SELECTION-03 既有覆盖：面板回归中的真实错误输出和默认信道测试保留，原有日志策略、事务与焦点实现未改。
- EMPTY-SELECTION-04 剩余范围：其他未校验 effect ID 等 JSON 读取仍需独立核验；不关闭整个 #134，不把此修复说成所有面板输入均已安全。

实施前核对 438 个引用的 8 个辅助面板版本，以及已注册工作树中的同函数；未发现其他实现或开放 #501/#504 的同文件改动。新增用例只验证本问题，未改任何既有测试或断言。未新增依赖或第三方产品资产。

## DESIGN §0.7 处置

本次只改选择查询，不改下列文件的布局与样式；保留原有处置，不宣称重新证明旧条目仍存在或已修复，也不宣称 DESIGN §12 全部验收通过。

| 基线条目 | 状态 | 原因 |
| --- | --- | --- |
| path_controls 六处行高 | retained with reason | 不改行高或纵向节奏 |
| path_controls 双列宽度 | retained with reason | 不改列宽 |
| path_controls 按钮高度 | retained with reason | 不改按钮尺寸或命中区域 |
| path_controls 数值背景 | retained with reason | 不改字段状态绘制 |
| path_controls 布尔控件 | retained with reason | 不改控件或事件路由 |
| effect_controls 行高与步进 | retained with reason | 不改属性行布局 |
| effect_controls 布尔按钮 | retained with reason | 不改布尔控件 |
| effect_controls 枚举按钮 | retained with reason | 不改下拉控件 |
| effect_controls 悬浮 alpha | retained with reason | 不改 hover 或 token |
| effect_controls 缩进层级 | retained with reason | 不改属性树 |
| timeline 标签列坐标 | retained with reason | 按兼容基线保留 |
| timeline 吸附引导颜色 | retained with reason | 不改颜色，未新增 token |

## 证据

[data/empty-layer-selection-880739de.zip](data/empty-layer-selection-880739de.zip)：739119 字节、35 项，SHA-256 `bee1e2be56848e4a2e0974960d8232968ececc4134c13d52a267c22013d514c2`。包含前后失败/成功日志、代码冻结、实际命令、原生报告及效果时间线截图。结构化结果见 [empty-layer-selection-results.json](empty-layer-selection-results.json)。CI 与合入状态以 PR 为准。
