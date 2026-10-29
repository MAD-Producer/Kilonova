# 四类曲线真实窗口、读数与工程文件字节阶段（#259，仍未完成）

接续[默认故障阶段](curve-mini-faults-progress.md)。实际源码 `4ac302e92e13bee5b295a5bd6ea79d4ee93fb648`，二进制SHA-256 `a81956e381be0668afaa996d527522fa1c4bef87e1f8364e06b4a65d0478ae1e`，Rust/WGSL/Cargo/样式来源前后哈希一致。本阶段133项不同普通用例通过；默认filter中的1项原生用例明确为ignored，随后显式运行并通过1项，内部包含32个场景，不把32场景冒称32个独立Rust测试，也不把ignored算作通过。

## 实际覆盖

普通属性、效果参数、同单位对比组、Mini四种图，各跑值/速度两种模式；Mini速度模式使用预先创作的Velocity曲线，不通过更改创作数据来切换验收状态。每种模式采用以下四种组合，共32场景、160张真实GPU截图：

| 语言/窗口 | 逻辑尺寸 | 受控像素比例 |
|---|---|---|
| 英文主窗 | 900×600 | 1 |
| 中文主窗 | 1600×1000 | 1.5 |
| 英文原生浮窗 | 720×640 | 2 |
| 中文原生浮窗 | 900×760 | 1 |

两个隐藏原生窗口真实attach/resize，生产chrome接收显式Input注入，CaptureUi真实渲染/回读GPU像素。没有测试物理鼠标/前台OS事件，也没有修改系统DPI设置；这是上述四种组合，不声称所有语言/尺寸/比例的笛卡尔积。初始折线、范围菜单、选区适配后的高倍率图、锁轴菜单、实际读数分别截图。

每场景验证实际More左键入口、键盘或指针选择菜单、原生Checkbox锁X/Y、IME/重复Space门控、Esc消费、失焦/跨窗输入取消；真实worker seek至85.25帧并输入Ctrl/Ctrl+Alt/Shift滚轮，锁定范围保持。解锁并自动范围后恢复原视图。Mini另外通过真实滚轮达到归一化X跨度<0.002，保留高倍率图；当前帧、工程revision/history和作者数据的边界分别核对。

## 模型与原生像素

场景由仓库独立创作的soft_sphere_motion节点图生成包，安装到测试目录自己的plugins，使用真实worker快照和catalogue，不使用伪成功模型。图层偏移10帧，普通键按合成时间10/70/130显示，效果键20/80/140；曲线来自真实Core采样。Mini进度/速度定义保持归一化；选中的有效输出区间映射到合成时间与实际分量单位，向量X负速度和Y零速度保留。

每场景至少2个模型点在原生图像中找到附近有色曲线像素；记录逻辑坐标、物理坐标和实际距离，最大最近像素距离0.667262物理像素。此指标是最近曲线像素位置检查，不是连续中心线误差上界；0.36逻辑像素密集模型参考仍是已有细分回归的另一种证据，不合并成更强的数学保证。菜单与读数还需审阅真实图像，文件存在或退出0不代表视觉已通过。

## 实际保存与失效

先显式保存，递归读取实际工程文件，保留saved-before副本；32场景全部只产生视图操作，逐场景检查真实worker project/revision/history不变，再显式保存并逐字节比较。随后单独制造一次真实外部SetScalar编辑，使已打开的普通图菜单来源失效；Delete被消费、取消菜单不再改动外部编辑后的project/revision/history。该外部编辑及Undo与视图不变性阶段分开核对，不把测试自身的历史变化藏掉。Undo后保存、文件逐字节恢复，Core storage实际重读后工程相等，保留saved-after副本。

实际保存文件1项，project.json 10595字节，SHA-256 `3a82f6018f0a184758a537215ed450b70e46f2f7fdba431664c92bab06973bea`；比较的是磁盘文件，非仅序列化内存值。唯一排除的是root/.motion-editor.lock：engine.rs ProjectLock持有的零字节OS互斥文件，记录长度必须0；其他文件读失败仍使验收失败。真实worker revision失效是单独的普通图场景，不冒称在每一个32组合中都做了外部编辑。

## 保留失败与修正

- App16/17均131项通过；原生17在读取进程锁时失败，未进入完整场景。只明确排除该零字节运行时互斥文件，不跳过工程数据文件或一般IO错误。
- App18仍131项通过；原生18在对比图More左键开启后的下一帧失败。组标题位于比较框选viewport，按键还被旧背景选择流程当作框选起点，并投射关键帧上下文，导致新菜单失效。入口调用既有block_background取得本次输入归属；补中英主/浮窗左键回归，保持原native断言。
- App19 132项与原生32场景/160截图通过，但图像人工审阅发现长图层名挤掉Mini实际数值/单位。Data保留完整图例标题，另提供结构化短属性/分量标题供Mini读数；共享读数先显示帧号，避免长标题挤掉时间。新增中英1/1.5/2比例的真实字体宽度检查。当前App20全部133项和显式原生场景重验，旧19不能替代最终显示验收。
- 高倍率Mini另捕获无悬停卡片的折线帧，以便审阅实际曲线；读数仍单独截图。没有删除旧截图、修改旧断言、缩小命中区或改变模型采样来通过验收。

本阶段未增加依赖或第三方资产。记录探针只在cfg(test)存在；功能修正是More输入归属，显示修正是短标签和帧号优先，两者与既有故障观察计数分开。四个Mini默认故障站点继续实际回归，其余16站点仍按前阶段表待验证/边界处置；本阶段不以健康截图替代故障信道审计。

## DESIGN §0.7及后续

以下12处继续保留及原因，不声称修复。未新增property行、按钮式开关/枚举或画代码尺寸/颜色。

| 基线 | 处置与原因 |
|---|---|
| Path六处纵向步进 | retained with reason：未修改Path行，本轮不重新验收该处。 |
| Path两列分割 | retained with reason：未改Path列宽。 |
| Path按钮高度 | retained with reason：未改Path按钮和原命中范围。 |
| Path自绘字段背景 | retained with reason：未改数值字段。 |
| Path布尔宽按钮 | retained with reason：未改Path布尔，新锁轴使用原生Checkbox。 |
| Effect Controls行高/步进 | retained with reason：未改效果属性行。 |
| Effect Controls fx布尔 | retained with reason：未改效果开关，不冒认主分支整改。 |
| Effect Controls枚举 | retained with reason：未改枚举语义或控件。 |
| Effect Controls手工hover | retained with reason：未改该处，没有新增颜色。 |
| Effect Controls层级 | retained with reason：未改属性树层级。 |
| Timeline标签栏列坐标 | retained with reason：未改标签栏坐标，继承主分支独立开关列。 |
| Timeline吸附线颜色 | retained with reason：未改吸附反馈或既有颜色。 |






性能、最终all-targets及最新main组合、其余观察站点、完整§12逐条责任边界和#259自身PR/CI仍待完成，不能据本阶段关闭#259。证据包略去可由fixture.msfx重建的plugins安装目录及上述零字节锁；保留全部阶段原日志、18失败路径现有截图、19代表图和本轮160图、真实文件副本及哈希manifest。19其余157张重复成功图继续保留工作区，哈希记录在省略清单，不重复放进ZIP。人工查看本轮7张代表图，未声称逐一审阅160张；1.5倍效果图的工具预览被缩放，精确像素度量来自原PNG。

[结构化结果](curve-native-ranges-progress-results.json) · [原始证据包](data/curve-native-ranges-stage-20.zip)：326项/15045919字节，SHA-256 `d9e7c02de66cd537490485e28d9f7fb4c3ad946b704eaf1ba65212d83c0f0b2e`。
