# 路径松手漂移与参数形状宽高

证据目录：`E:/Dev/aem/artifacts/property-selection-context/`。用户补充了两个独立症状：松开后节点偶尔跳到后来的光标位置；拖矩形角点触发 `convert shape to path before editing nodes`。本轮采用用户认可的交互：钢笔工具下参数形状的角点调整宽高并固定对角，自由路径节点保持节点编辑。

## 功能边界与证据

| 编号 | 处置 | 证据及限制 |
| --- | --- | --- |
| PATH-01 | 功能修复：已有节点/手柄以及新绘制手柄，结束时使用 `released` 事件坐标，不使用其后合并的最新 `mouse`。命中使用 `pressed` 坐标。 | `path-drag-before-fix.log` 原始失败：松开应只移动 15×10，却跳到 280×160；`path-drag-after-fix.log` 2 项通过，覆盖钢笔/蒙版、节点及两侧手柄、撤销重做和保存重开。 |
| PATH-02 | 功能修复：没有左键保持状态时结束旧拖拽；失焦、Esc 或工程/合成/图层/帧上下文失效时回滚活动事务，不把旧节点交给下一次按下。新绘制手柄也在丢失按键后结束放置。 | 原始失败证明旧 capture 被保留；控件测试检查结束后下一次 held 不产生编辑，事务能重新开启。正常 capture 结束不是故障，不再保留状态并累计 `path.drag_release` 错误。 |
| PATH-03 | 功能修复：形状 grips 使用包围盒四角，以原有 `EditBatch` 原子写入 Geometry `size`、`position` 轨道，固定对角，避免中间尺寸单独呈现。一轮拖动使用原有 Begin/Commit，一次撤销恢复整个形状。 | 25 种参数形状×4 角；保留内部 VectorContent、绘制和形状参数，不修改图层 Transform/源尺寸；圆角矩形的嵌套旋转/非等比缩放/斜切及局部动画帧覆盖。不会把角点发给 `SetNode` 或自动转换为自由路径。 |
| PATH-04 | 功能修复：自由路径 grips 从 authored 节点及真实 item/path/node ID 生成，使用同一组变换矩阵。分组节点走既有 `SetGroupParameter(node:path:node)`，不按渲染输出序号修改。 | 两个 Geometry 内路径/节点 ID 重复、中继器输出仍只暴露两份真实源地址；只改变目标 item。Trim/中继器的生成输出不是额外可编辑节点；不新增扁平化命令。分组节点删除/插入不在本轮新增范围。 |
| PATH-05 | 新观测：源几何采样失败使用每个 Controls 实例的有界连续计数器，首个 Debug，第 60 次 Error；成功、无目标和蒙版路径重置。 | 单一固定 site，记录 object、frame、阶段和原始固定原因，不输出用户内容。默认 Info 的实际损坏数据、文件日志、stderr、stdout 隔离、59/60 阈值、持续失败抑制及恢复重置由子进程测试检查。不是把 60 次等同于 1 秒。 |
| PATH-06 | 既有错误信道：实际模型编辑仍经过现有 engine 非 Present 命令、请求 ID、Info start、失败回复及 Error 文件/stderr。 | 没有修改 respond、重试/重启、超时、panic 或清理返回值。参数形状角点现在发送正确类型的命令，不屏蔽模型的非法节点错误。无效组变换不能产生伪造 grip。 |
| PATH-07 | 原生证据：新增矩形拖动前/后及宽高属性截图；真实输入经面板路由，实际 engine 事务执行，检查 420×260、中心偏移 50×30 及矩形类型保留。 | `path-release-build.log` 优化 FFmpeg 构建成功（10 分 25 秒）；`path-native-acceptance.log` 完整原生验收退出 0。双语及主题覆盖截图在 `release/native-path-fixes/`，已目视检查中文前/后及属性图：对角固定、尺寸与源类型保留。捕获器恢复活动事务和 fixture，保留主失败及清理失败，不修改已有断言和验收时限。 |

## 检查记录

- `path-all-tests.log` 保留抽取组变换函数时遗漏原采样参数局部变量的编译失败；恢复原渲染分支及其参数采样后，`path-all-tests-corrected.log` 为 46 项通过。
- `path-resize-tests.log` 保留嵌套组夹具使用旧工程版本导致的失败；夹具改为合法组格式版本，不修改产品断言。
- `path-final-targeted-tests.log` 为 49 项通过；随后 `path-full-tests.log` 为 509 项通过、1 项既有忽略，219.07 秒。后续增加的故障观测及原生场景仍需定向验证，不能把前次运行声称为覆盖尚未编译的代码。
- 随后默认日志故障测试加入后，`path-final-tests-with-observations.log` 为 51 项通过；最终原子批量修改及第二条命令失败回滚加入后，`path-atomic-final-tests.log` 为 52 项通过，9.74 秒。后者同时编译了下一阶段效果选择镜像源码，不以这些路径测试证明效果选择行为。
- 原生验收保持 40/125/90 秒预算和所有恢复断言，完整覆盖主窗/浮窗、双语、节点实时渲染、共享 MCP 和主题覆盖。该目录的 stderr 文件均为空。中文拖动前、拖动后及宽高属性截图均已实际查看；用户已收到前/后截图。
- 当前本地 FFmpeg 包仅证明解码能力；构建报告 `video_encoder=false`，本轮不宣称编码导出支持。模型库测试命令执行 0 项测试，仅计为编译证据。
- 未新增依赖、第三方资产或 token。新截图来自独立的 Motion Studio 程序及自建 fixture；用户录屏只作症状依据。

## DESIGN §0.7 逐项处置

| 基线项 | 处置 |
| --- | --- |
| Path 六处纵向步长 | retained：本轮修改视口交互，不修改属性行布局。 |
| Path 两列分割 | retained：未修改字段布局。 |
| Path 混合按钮高度 | retained：未修改控件和 grip 命中容差。 |
| Path 数值背景 | retained：未修改字段绘制。 |
| Path 全宽布尔按钮 | retained：未修改布尔控件。 |
| Effect 行高/步进 | retained：未修改效果面板。 |
| Effect 布尔按钮 | retained：未修改效果面板。 |
| Effect 枚举按钮 | retained：未修改效果面板。 |
| Effect hover alpha | retained：未修改效果面板。 |
| Effect 层级 | retained：未修改效果面板。 |
| Timeline 固定列坐标 | retained：未修改时间轴布局。 |
| Timeline 吸附线颜色 | retained：未修改。 |

复用现有源路径绘制、颜色/提示 token、命中半径和投影；矩形的四角包围框是尺寸控制的功能变化。此记录不声称整体 DESIGN 合规，也不关闭未完成的 #214。
