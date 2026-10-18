# 属性行关键帧导航

对应 #211。基于 PR #330 `f87acdc` 堆叠实现。属性原生截图、带 FFmpeg 优化构建及完整原生验收已验证；远端 CI 与合并仍需独立核实，不能据本记录手工关闭 issue。

## 实现和责任边界

| ID | 分类 | 实现与证据 |
| --- | --- | --- |
| KEY-01 | 新功能 | 可动画变换、分离轴、摄影机、蒙版路径/数值、形状参数、音频左右电平和效果参数行增加上一键/增删当前键/下一键。共享有限帧、合成边界及 J/K 候选规则；保留小数寻帧，整帧编辑不静默取整。静态配置不伪装成可动画属性。 |
| KEY-02 | 通道和事务 | 复用现有核心命令；绑定 object/property/axis 或现有蒙版、形状、音频、效果实例的类型化目标。图层偏移由已有核心时间转换处理；增删保留其他通道，单次新增只形成一个撤销步骤。Anchor 按两分量采样，不按三分量解码。 |
| KEY-03 | 曲线参数 | 效果 Curve 新增当前键时保留已有控制点，并保存当时插值后的 LUT，避免插入中间键时改写当前颜色映射。使用核心既有 CurveObject 格式，不新增持久化协议。 |
| KEY-04 | 交互 | 无上一/下一键时禁用对应箭头；当前键突出中心按钮；锁定、观察模式、取消及失焦不提交新增。主窗/原生浮窗及双语使用同一输入路径。取消和消失的目标属于正常分支。 |
| KEY-05 | 布局 | 三个完整 icon.slot，未缩小命中容差；字段仍在同一行。修复窄 Masks 宿主中新增导航后数值区压到秒表的几何问题。旧 Repeater 与属性手势测试改为点击移动后的字段，保留原值、撤销和取消断言。 |
| OBS-01 | 新观察 | effects.navigation_parameter 将键盘导航中的坏参数原因为默认级别记录，含对象、实例与参数名。新受控坏数据测试验证文件、Error stderr、stdout 隔离及 120 次重复抑制；目标删除不报错。 |
| OBS-02 | 既有覆盖 | 复用 panels::observation 的每调用点进程生命周期计数：首次 Warn、累计第 60 次 Error、之后抑制；健康邻居不重置。不是连续失败计数，也不解释为一秒。上下文沿用 4096 字符限制，无健康帧文件 IO。 |
| OBS-03 | 协议边界 | animation.key_scalar/key_anchor/key_vector 观察已有强类型采样结果的二次解码；合法 sample 结果满足形状约定，不用 mocked 成功声称故障覆盖。实际坏轨道仍走既有 animation.value_decode/key_decode 信道。没有新 worker、超时、重试或 panic 策略。 |
| LIMIT-01 | 范围 | 不将效果实例的时间轴摘要伪装成完整参数行；参数入口位于已有属性/效果控件宿主。全局多选与键盘所有权另属 #214/#215，本次沿用既有选择语义，并在蒙版按钮接管时清理残留形状选择。 |

## 验证记录

初次完整测试 436 passed / 4 failed / 1 ignored，四项均为新增导航后仍点击旧数值坐标的手势夹具。更新点击目标，不修改既有编辑值、保存或撤销断言后，`key-navigation-regression.log` 为 440 passed / 0 failed / 1 ignored（94.05 秒）。这是当时版本，不能代替后续新增测试和最终整合验证。

新增实际输入测试覆盖：中英文 × Properties/Timeline × 主窗/浮窗的分离 X；摄影机分离轴；蒙版四通道含路径整姿态；形状位置、填充、描边、修剪参数；音频左右电平隔离；效果实例向量参数及曲线 LUT。检查合成/本地帧、只寻帧不写历史、当前键增删、撤销重做及保存重开。候选规则单独覆盖小数帧、负帧、重复帧、非有限值和合成末端。

整合 #330 后首次完整测试 467 passed / 1 failed / 1 ignored：新增几何测试错误地要求 Timeline 秒表也为 icon.slot（20），而既有 timeline.property.watch_width 为 14。没有修改生产命中区域；测试改为分别断言两个宿主各自的既有 token 后，最终完整测试 `key-navigation-integrated-final.log` 为 **468 passed / 0 failed / 1 ignored**，161.35 秒。随后仅将摄影机 render 的可见性收窄至 panels，修复本次新增的私有返回类型警告，不改变运行行为。

整合后的工具测试 `key-navigation-integrated-tools.log` 为 **54 项通过，1 项跳过**，32.05 秒。新增主窗/模式截图后再次通过（`property-keyframe-navigation/tools-final.log`，34.68 秒）。摄影机及效果宿主进一步补入 Esc、失焦、锁定、观察模式的实际点击断言，`key-navigation-cancellation.log` **7 项通过**，10.89 秒。最终产品代码无新增私有接口警告，带 FFmpeg 调试构建通过（`key-navigation-modes-build.log`，22.92 秒，21 条既有警告）。

首次原生验收在实际菜单打开断言处失败，原因为离散效果菜单截图仍点击旧 x=80，未越过新增导航。只更新截图输入坐标，保留原菜单断言与 90 秒限制。新增普通时间轴截图还发现夹具的通用分支覆盖模式，已排除普通模式并对两个场景加入实际 graph_mode 断言；没有修改产品模式切换。

最终 `native-properties-verified.log` **通过**：23 个场景 × 中英文，共 46 张实际 GPU 截图，含主窗、原生浮窗、220px 属性面板、蒙版、现有右键菜单、普通时间轴及曲线模式。目视检查主窗与浮窗属性行、两个时间轴模式、蒙版分组和效果菜单：导航、秒表和行内数值不重叠，关键帧与属性行对应；220px 的长标签/多分量数值仍按现有字段截断，不宣称全部长文本可见。截图见 `property-keyframe-navigation/screens-verified/`。

完整原生检查另行执行，调试构建在英文普通 smoke 的既有 **40 秒**门限超时（`native-full.log`）；该次后续阶段未运行，未放宽时限。随后带 FFmpeg 优化构建通过（`release-build.log`，8 分 17 秒，21 条既有警告）；本地仅将 motion-desktop 的 codegen-units 设为 16，保留 release 优化及 thin LTO，未修改仓库构建配置。该实际优化产物的完整原生验收也通过（`release-native.log`），包括中英文、样式覆盖、原生浮窗、实时节点和共享 MCP，沿用原有 40/90/125 秒门限。多 DPI、其他平台及远端 CI 尚需独立验证。

## DESIGN §0.7 处置

| 既有项目 | 本次处置 |
| --- | --- |
| Path 六处纵向步长 | retained：导航不增加属性行，不改现有纵向节奏。 |
| Path 两列分割 | retained：保留既有多分量；本次仅为导航预留横向位置。蒙版秒表重叠另以 KEY-05 修复，不声称整个历史分割问题已解决。 |
| Path 混合按钮高度 | retained：没有缩小现有字段、按钮或命中区。 |
| Path 数值背景 | retained：沿用现有数值控件。 |
| Path 全宽布尔按钮 | retained：不修改静态布尔配置。 |
| Effect 行高和步进 | retained：沿用现有 effect.row.height 等 token，曲线图仍是已有特殊编辑区域。 |
| Effect 布尔按钮 | retained：不改布尔控件类型。 |
| Effect 枚举按钮 | retained：不改下拉选择行为。 |
| Effect hover alpha | retained：新导航走已有 button.icon 状态，不扩散手工 alpha。 |
| Effect 层级 | retained：保留既有实例/参数树缩进。 |
| Timeline 固定列坐标 | retained：导航位于原树分支之前，不移动图层列或轨道起点。 |
| Timeline 吸附线颜色 | retained：未改吸附。 |

无新增颜色/尺寸 token、依赖或第三方资产，复用仓库已有图标。默认样式的实际属性截图与完整样式覆盖已验证；多 DPI和其他平台仍待验证，没有宣称整体 DESIGN 合规。
