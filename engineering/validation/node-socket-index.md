# 节点连线端口索引（#134 子项）

## 范围与协调

基于 main `cc4467b6`，独立分支 `codex/node-socket-index`。实施前核对所有本地工作树的节点目录改动、开放 PR #425/#426 的文件清单。节点导航和自定义快捷键旧工作树修改 interaction/mod/context_menu 等；本批仅改 canvas/wires 和新增测试，没有覆盖其导航或键位逻辑。资源预算、模型不变量工作也不与本批文件重叠。保留根目录和其他工作树的未提交内容。

当前主线仍在 paint_canvas 为每条连线扫描全部 sockets，且拖出连线时对每个输入标签重新扫描来源端口。wire_at/collect_cut_inputs 同样通过 wire_curve 对每条连线重扫。改为各次绘制/命中/切线处理建立一次借用索引，键为 `(node, port, output)`，不复制端口名称；拖线来源在标签循环外查询一次。无连接且没有拖线的绘制跳过索引分配；空连接的命中/切线也提前正常返回。

索引仅借用当前 cards，结束本次处理即释放；不会跨工程、编辑、移动、折叠、pan/zoom 或窗口保留旧坐标。重复键保留第一个 socket，与原 find 一致。缺失端点继续跳过曲线。连线方向、绘制顺序、Bezier、裁剪、屏幕命中容差、断线集合和撤销语义不变。

设 S 为端口数、E 为连线数：原端点解析最坏 O(E·S)，现在构建 O(S)、查询平均 O(E)，额外瞬时内存 O(S)。拖线标签的兼容性判断不再逐标签 O(S) 搜索。HashMap 不是最坏情况常数保证；仍保留曲线采样、卡片与标签构建成本，不能将这个局部改动描述为全帧线性、零分配或确定的 FPS 提升。

#134 继续开放：整帧双绘制、dock 布局、cards 重建、其他卡片/连接候选查找及其实际瓶颈仍需独立核验。没有借此关闭整个合集。

## 验证

新增 socket_index_tests 逐项比较实际索引与原线性查找：方向、同名端口、缺失节点/端口、重复键首项；实际 paint_canvas/wire_at/collect_cut_inputs 检查三个缩放、移动、折叠及中英文时的同一曲线和无编辑行为。另以 128/512/2048 端口查询完整集合，对照原算法实际比较次数与索引构建加查询用时；用时只作本机构建的观察，不设易波动的 CI 性能断言。

最终节点回归 **140 通过、0 失败、0 忽略**，包括全部现有节点输入、连线、编组、撤销和真实失败信道测试，以及新增四项。首轮 139 项通过后补充重叠连线优先级/全量切线检查，再对最终源码重跑 140 项；两轮数字不相加。最终日志 `.tools/node-socket-index/final-node-tests.log`。

| 端口/查询数 | 原线性实际比较数 | 原线性查询 µs | 索引构建 + 查询 µs |
|---:|---:|---:|---:|
| 128 | 8,256 | 256 | 76 |
| 512 | 131,328 | 4,221 | 284 |
| 2,048 | 2,098,176 | 62,721 | 1,098 |

这是 Windows 本机 debug=0、debug assertions 开启的单次测试窗口，不是整个画布基准，也不外推平台/FPS。生产 dev 构建成功，只有 19 项既有桌面警告；来源检查覆盖 1807 tracked paths，样式消费者、差异和新增文件格式检查通过。

原生 Debug 英文 `--node-smoke` 在原有 125 秒外部期限内未完成，保留 129 张已生成截图及原始 TimeoutExpired、stdout/stderr；没有生成完整报告，不能算作成功。stderr 同时报告本机 Vulkan loader 缺少两个 Epic overlay JSON；仅记录环境错误，不认定它就是超时原因。隔离临时目录已由收集器清理，首轮完整文件日志/last-stalls 未保留，这是证据限制。留样 `.tools/node-socket-index/native.log` 和 `artifacts/node-socket-index/`。

后续用 Release 优化构建重验，保持原始断言及 125 秒外部/120 秒内部期限，不增加产品重试、修改既有断言或扩大期限。为本机链接资源使用 release.debug=0、lto=false、codegen-units=256、单任务编译；不改仓库构建配置。Release 和两语言原生验收在本提交时仍进行中，完整远程 CI 也待 PR 运行；不能把 140 项成功当成这些待办已完成。

## 错误与观测审计

| ID | 分类 | 处理和边界 |
|---|---|---|
| NSI-MISS | 正常分支 | 空集合和缺失端点保留原 None/空曲线，没有错误重试或每帧日志。 |
| NSI-SPECS | 既有观测 | cards 的 canvas.sockets 和标签的 canvas.labels 仍调用原 observed，保留原错误与受限节点上下文；本批不修改失败阈值。 |
| NSI-INPUT | 功能边界 | socket_at 的 nearest、front-card 遮挡、9px/可见半径及候选验证逻辑不变。wire_at 仍后画连线优先，切线仍累积输入集合。 |
| NSI-LIFE | 生命周期 | 只保留一次处理的借用引用；无线程、handler、超时、重启、额外文件 IO 或健康 Present 日志。 |
| NSI-LOG | 既有覆盖复验 | 节点 observability_tests 两项真实失败回归通过，核对默认级别文件记录、Error stderr、stdout 隔离及 first/60 抑制；不新增/修改观察点，不把成功曲线或编译当成日志验收。 |
| NSI-NATIVE | 待验证与证据限制 | Debug 总期限失败保留，优化构建和中英文原生验收待完成；超时原因未证实，首轮隔离文件日志未保留。新收集器为后续运行保留 runtime 目录。 |

## DESIGN §0.7 与 §12

本批没有改变任何尺寸、颜色、控件、文本或布局；没有新 token、依赖、第三方资产或兼容名称。已检查 PROVENANCE.md、ATTRIBUTION.md 和 NOTICE，属于独立实现的内部查找优化。

| §0.7 基线行 | 本批处置 |
|---|---|
| path_controls 六种步距 | retained with reason：没有路径面板修改。 |
| path_controls 两列硬编码 | retained with reason：没有路径布局修改。 |
| path_controls 按钮/字段高度 | retained with reason：没有控件或命中区修改。 |
| path_controls 自绘数值底色 | retained with reason：没有数值字段修改。 |
| path_controls 布尔宽按钮 | retained with reason：没有布尔控件修改。 |
| effect_controls 行高/步距 | retained with reason：没有效果面板修改。 |
| effect_controls 布尔按钮 | retained with reason：没有效果控件修改。 |
| effect_controls 枚举宽按钮 | retained with reason：没有枚举控件修改。 |
| effect_controls hover 底色 | retained with reason：没有绘制状态修改。 |
| effect_controls 缩进/层级 | retained with reason：没有属性树修改。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，没有时间轴修改。 |
| timeline snap 指示线 | retained with reason：保留既有表现，没有主题修改。 |

这些处置不表示视觉缺陷已全部修复，也不表示全体 19 面板和各平台 DPI 已验收。节点自身的既有颜色/尺寸字面量未在此做无关重构；本批只替换内部端点查找，视觉与命中证据另列于上。
