# #257：矢量与蒙版参数的精确时间编辑

接续 `04e74a49`，沿用 `feat/layer-time-mapping`，交付至草稿 PR #579。
开工核对开放 PR 和近期本地分支：其他工作为 #575 原生预渲染和跟踪数据导入，
没有发现同一组路径参数入口的重叠工作。#257 保持打开。

## 功能范围

映射图层的矢量组数字字段、颜色弹窗、秒表，以及蒙版数字字段、全路径秒表和
合成视图羽化手柄，使用 TrackAt 的精确局部时间。3/2 伸缩、offset=2 时，
6.5 合成帧可修改局部键 -3，不把播放头或伴随通道的采样时间先取整。
联动羽化仍按原比例更新两轴；全路径秒表把所有节点作为一个原子批次提交。
未映射的既有整数命令、显式选中键命令和静态结构选项不改为播放头命令。

数字手势捕获工程身份、文档代次、图层时钟、offset 和精确播放头。上下文
变化时调用已有事务取消入口回滚一次，释放不能再次提交。功能修正与观察接线
分开：不新增后台线程、超时、重启或重试，不改变产品布局、token、依赖或素材。

## 验证状态

首次构建在新夹具中发现字段名称和导入
冲突；修正新测试后构建通过。首次运行随后发现新测试工程把组内内容包装为
Group 时仍保留组外填充、描边，违反已有模型约束。只修正测试工程，生产
校验和既有测试断言未放宽。两份原始失败日志均保留。

最终 **158 个不同测试通过**：核心 TrackAt 17、新增参数组 5、边界手柄 1、
既有路径组 96、音频 29、数字字段 9、手动原生验收 1。另有五项既有手动
用例未运行；新增原生用例已经单独运行，不计入未运行项。

验证覆盖完整工程不变性、伴随坐标和颜色通道、联动羽化比例、原子撤销、
保存重开和精确时钟失效取消。默认级别错误注入验证四站点各首次 Warn／
第 60 次 Error：文件共八条观察记录，stderr 只有四条 Error，stdout 无业务
日志。中间健康调用不复位进程生命周期累计计数。

原生测试在主窗／浮窗、中英文、正放／倒放共 16 组场景执行生产 chrome
输入，保留 48 张实际 GPU 截图，16 份工程在另一个进程重新打开并比较完整
工程与空历史。它不是物理鼠标输入，也不是视频或音频设备验收。示例见
[中文蒙版主窗](layer-time-path-authoring-evidence/mask-main-zh.png)和
[英文路径浮窗](layer-time-path-authoring-evidence/vector-floating-en.png)。
目视确认蒙版 X=17.25、Y=24，以及矢量位置 X=17.25、Y=40 的伴随值保留；
路径浮窗原有数字字形底部裁切仍可见，本阶段未修改其排版，不声称视觉整改完成。

带 FFmpeg、宿主 diagnostics 的 workspace all-targets check、样式消费者、
新模块 rustfmt 与 diff 检查通过。仍有既有编译警告和 Epic Overlay 缺失
Vulkan JSON 的本机日志，没有隐藏或修改系统配置。

一次音频回归因复制测试程序时未携带 PDB，不能符号化 panic 堆栈而失败；
补齐同一构建的 PDB 后，相同程序通过原断言。原失败日志保留，未改生产
代码、panic hook 或既有测试断言。源码、程序和调试符号哈希、逐项结果、
运行脚本、初次失败和最终原生日志见
[清单](layer-time-path-authoring-evidence/manifest.json)及
[证据包](layer-time-path-authoring-evidence/evidence.zip)。

## 可观测性审计

| ID | 分类 | 处置与限制 |
|---|---|---|
| PATH-T-01 | 功能修正 | 参数和秒表按精确局部时间写入，数字拖动时间上下文变化回滚；不改变媒体时钟或 DSP。 |
| PATH-T-02 | 新观察 | `mask.numeric_time_begin`、`mask.numeric_time_continue`、`vector.numeric_time_begin`、`vector.numeric_time_continue` 四个固定站点沿用 observe!，带 object/mask/item/parameter/phase 与原始原因。蒙版边界手柄共用数字开始站点。 |
| PATH-T-03 | 既有覆盖 | TrackAt 候选校验与事务原子性、worker 非 Present 的 Info start/在途发布、Error 文件与 stderr；本阶段不替换这些实现。 |
| PATH-T-04 | 正常分支 | 正常取消、失焦、时间变更和字段不存在不作为故障；非整数局部键不可用时由原 worker 返回原因，不偷偷取整。 |
| PATH-T-05 | 计数范围 | 面板进程生命周期累计，首次 Warn、第 60 次 Error，后续抑制；健康调用不复位，新进程重置。固定键、饱和计数及 4096 字符上下文上限沿用既有规则，60 次不等于一秒。 |
| PATH-T-06 | 实际失败验证 | 四站点的真实错误日志、stderr/stdout 隔离及健康交错抑制验证通过；原生 worker 对非整数局部整数键的拒绝同时进入文件与 stderr，完整工程和历史不变。 |
| PATH-T-07 | 剩余工作 | 路径部分导航、连续空间轨道的旧编辑入口、相机专项手势、roving 补偿及真实媒体预热／设备／高倍速抗混叠尚未全部验收；不关闭 #257。 |

## DESIGN §0.7 逐项处置

本阶段不作视觉整改；前后保留既有绘制与命中尺寸。下表的 retained 不代表
原有问题已经解决，独立视觉合规仍 pending verification。

| 基线位置／原行为 | 本阶段处置 |
|---|---|
| path_controls.rs 六处字面行步进 | retained with reason：不改路径布局和步进。 |
| path_controls.rs 双列分割 | retained with reason：不改列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：不改按钮语义和命中尺寸。 |
| path_controls.rs 自绘数字背景 | retained with reason：复用原数字绘制，仅改命令时间。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：不改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：不改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：不改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：不改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：不改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：不改效果树结构。 |
| timeline.rs 十一列坐标 | retained with reason：按兼容基线保留。 |
| timeline.rs 吸附线颜色 | retained with reason：不改拖动反馈或主题。 |

§5.2 没有新增属性行、行高、颜色或第三方资产。§12 的最终截图与输入验证
仅能证明具体路径参数场景，不能证明全产品、所有尺寸或所有平台合规。
