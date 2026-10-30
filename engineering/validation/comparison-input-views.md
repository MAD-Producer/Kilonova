# 对比曲线绘图区借用输入

Refs #158。基线为 main `82633385658dcc616edacbcd557f0f477a6f57c1`，分支为 `perf/comparison-input-views`。实施前核对开放 PR、近期文件提交和相关工作树：已有 `perf/curve-input-views` 只覆盖工具栏与通道弹层，绘图区仍保留两处整帧复制；本次不重复登记旧成果。没有修改根工作树或其他作者分支。

## 实现与测量

绘图区使用现有 `InputView` 借用文本、按键数组和剪贴板。输入法活动、已消费输入法事件及失焦时仍只过滤 keys；修饰键滚轮仍只屏蔽分组纵向滚动，原始滚轮继续供曲线导航使用。范围菜单启动函数接受现有 `InputSource`，其他调用方仍可传入拥有数据的 `Input`。未改变模型、动作、采样、缓存、样式、尺寸或命中容差，无新依赖或第三方资产。

同一临时线程局部分配计数器分别实际编译基线和优化后的程序。中英文 × 360/1200 宽 × 正常/composing/ime_consumed/focus_lost × 无修饰/Ctrl/Alt/Shift，共 64 个场景。每场景预热 3 帧、测量 10 帧；输入含 36,864 字节自造文本及四组各 128 项的按键载荷。视口高度 180，输入为负向滚轮；本矩阵的分组偏移均停在上界 0，没有用它证明非零偏移的滚动行为。

640 个测量帧合计减少 **5,120 次分配请求、42,045,440 申请字节**，各场景均未增加。64 份最后一帧的完整 PaintList（形状、颜色、字形、图像和顺序）、关键帧标记、菜单命中区与滚动偏移逐字节相同。每次绘制均未产生编辑动作，输入和工程快照保持不变。这是压力载荷下的申请量，不能推断典型帧、常驻内存、峰值内存、GPU 时间或 FPS；没有宣称整帧零分配。

临时全局分配器和探针已移除，原 main.rs、tests.rs 按字节恢复，再从最终源码编译测试与带 FFmpeg 的桌面程序。测试构建有 26 项警告，与基线探针构建相同；产品构建有 29 项警告，不宣称整个仓库无警告。

## 验证与限制

- 对比曲线既有回归：14 passed，0 failed，0 ignored，覆盖真实通道选择、原子拖动、范围缓存和错误日志。
- 范围菜单回归：24 passed，0 failed，1 个既有人工原生 GPU 捕获保持 ignored。包含实际主窗/浮窗绘制、菜单按压归属、修饰键导航及坏范围/采样预算/读数溢出的文件日志、Error stderr、stdout 隔离和恢复验证；耗时 545.14 秒，未缩减断言或运行次数。两组最终回归合计 38 passed，0 failed，1 ignored；未把忽略项算作通过。
- 带 FFmpeg 的最终桌面构建通过；将其与运行时 DLL 放在独立目录后，实际 `--help` 启动成功。
- 来源检查和面板样式引用检查通过，差异检查通过。
- 探针首次构建因格式化未实现 Debug 的类型失败；随后改为读取输入视图及显式通道字段。首次直接运行测试二进制报 `STATUS_DLL_NOT_FOUND`，补入独立目录的 FFmpeg DLL 后基线和优化版都成功运行。原始失败记录保留，未改产品错误处理或测试断言。
- 此次没有新的界面样式变化，使用完整绘制命令对照验证输出；未新增 GPU 像素截图或物理键盘至系统输入法的全链路验收。远端 CI 以本 PR 当前提交结果为准。

## 错误信道与剩余范围

| 条目 | 处置 | 证据与边界 |
| --- | --- | --- |
| CFI-01 绘图区复制 | 新优化 | 两处 Input 深复制改为借用；640 帧分配量与 64 份最终状态对照。 |
| CFI-02 输入过滤 | 正常分支 | 输入法、失焦、修饰键滚轮和鼠标离开菜单视口仍沿原规则处理，不新增日志、重试或文件 IO。 |
| CFI-03 故障诊断 | 既有覆盖 | 运行实际坏轨道及范围故障用例，保留原有文件日志、Error stderr、stdout 隔离、阈值与恢复断言；没有新增观察站点。 |
| CFI-04 其他复制 | 待完成 | 其他面板输入克隆、跨帧拥有权快照和 #158 其余验收不在本项范围，合集不关闭。 |

## DESIGN §0.7 逐项处置

本次保留下列区域的当前代码，不重新判定历史缺陷是否仍存在，也不把编译通过视为整个 DESIGN §12 验收完成。

| 基线条目 | 状态与原因 |
| --- | --- |
| path_controls 六处行高 | retained with reason：未改路径行节奏。 |
| path_controls 双列宽度 | retained with reason：未改列布局。 |
| path_controls 按钮高度 | retained with reason：未改控件或命中区。 |
| path_controls 数值底色 | retained with reason：未改数值绘制。 |
| path_controls 布尔按钮 | retained with reason：未改开关。 |
| effect_controls 行高/步进 | retained with reason：未改效果行。 |
| effect_controls 布尔按钮 | retained with reason：未改效果控件。 |
| effect_controls 枚举按钮 | retained with reason：未改下拉。 |
| effect_controls hover 底色 | retained with reason：未改颜色或状态。 |
| effect_controls 缩进层级 | retained with reason：未改属性树。 |
| timeline 标签列坐标 | retained with reason：保留当前布局。 |
| timeline 吸附线 | retained with reason：未改拖动反馈。 |

结构化测量见 [comparison-input-views-results.json](comparison-input-views-results.json)。原始日志、两版绘制快照、临时探针与二进制哈希见 [证据包](data/comparison-input-views.zip)；不包含可执行文件或第三方运行时。归档字节、条目与 SHA-256 记录于结构化测量的 evidence 字段。
