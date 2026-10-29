# 效果曲线范围与高倍率绘制阶段验证（#259，未完成）

后续对比图分组范围、缓存和锁项容量边界见[对比阶段记录](curve-comparison-ranges-progress.md)，完整#259仍未完成。

本阶段把普通属性图的范围菜单、锁轴与误差驱动绘制接入效果时间图，并共用显示合成帧/模型值/单位的读数卡片。Mini、对比图、全部原生视觉验收与新增默认故障信道审计仍待完成，不能据本阶段关闭#259或提交完成PR。此前记录见[属性菜单阶段](curve-range-menus-progress.md)。

## 实现与来源

源码提交 `d84bb77fdc0e09992d1e8ef01905c768267208c6`，二进制SHA-256 `bcb6cf40acdb8f10dda7441581896be0d33f3a4e552f71edfec9d469ffb5b468`，完整Rust/WGSL/Cargo/样式源前后哈希一致。开始前核对#524：它处理预设和效果右键，没有修改效果曲线绘制；本项不重复其功能。本阶段未合入尚未合并的#524，没有新依赖或第三方资产，也没有Adobe二进制或资产参考。

效果图按实际EffectParam采样，保留参数夹限、离散保持和既有物理速度；原模型、渲染语义和编辑命令未改。未知单位显示value/value/s，颜色显示RGBA/RGBA/s，不猜测像素或百分比；包hash、参数、分量和模式分别保存Y视图。适配按真实本地键加一次图层偏移，已拥有但为空的关键帧集合不退回旧单键。菜单同时捕获效果选中键；选中键变化会取消旧菜单并消费关闭帧。主窗/浮窗、输入法和键盘归属沿用同一控制器。

效果图宽度与共享时间标尺一致，取消二次扣除间距。普通属性图和效果图读数都显示合成时间及实际样本；新宽度令牌先写入dark.json，读数在图内裁剪，容不下内边距的小视口正常不绘制卡片。X/Y锁、适配和缩放只写临时视图，没有新增工程/历史命令；原关键帧/手柄命中容差和F9行为保持。

## 实际证据与失败修正

本轮真实桌面程序 96 个不同用例通过：细分7、模型/物理速度4、既有时间轴50、既有手柄2、菜单10、轴原语7、效果16。独立脚本重复的原语测试不累加。效果菜单验证中英文、主窗/浮窗chrome受控输入、合成偏移、原生值与每秒速度、关键帧失效取消、空选区、锁轴/滚动/播放、Esc/Delete隔离，以及工程序列化字节与撤销状态不变；没有创建本项真实原生窗口或截图，序列化字节也不替代保存文件字节验收。

stage05编译成功，94项通过/1项失败：真实Core弹性夹限速度在帧18.3765411377的密集对照误差约0.400544，超过原0.36逻辑像素断言。独立复现04/05均13通过/1失败，表明只因时间括号小于像素而停止细分仍可能漏掉跳变后的纵向极值，另记录到约12.408像素误差。修正保留三探针，并检查邻近样本的二次控制包络；包络不平坦时继续误差驱动细分，保留单侧跳变至相邻可表示时间。有限栈深度由32改为64，以覆盖f64边界；点数65536、求值8倍预算、错误链与失败不返回部分成功保持。不是仅增加固定采样数，也没有修改旧断言或0.36界限。

独立复现06的14项通过；最终桌面完整过滤器全通过。实际模型密集对照覆盖Bezier/Elastic/Hold、参数夹限、离散左极限、值与速度、正常/高倍率窗口，每段128个独立密集点保持原界限。离散测试的default/静态值补成合法整数，是夹具修正，未改模型逻辑或断言。新增真实采样Err和Ok(NaN)测试证明原因和非有限值不变成成功范围，不冒称默认文件/Error stderr/stdout已验证。

## 观察与待验收项

新effect.graph_view_bounds/adaptive/fit_all/fit_selection/readout站点复用固定站点进程寿命Counter：首Warn、累计第60次Error、之后抑制，进程结束清零；不是连续次数或秒数。它们的默认级别文件/Error stderr/stdout和正常取消处置仍待实际故障验收。既有malformed_effect_rows测试通过只证明原站点，不能替代这些新增站点。没有工作线程、超时、重启或重试改动。缓存/性能、完整§12和本项原生视觉/实际保存验收仍未完成。

## DESIGN §0.7逐项处置

| 基线 | 本阶段处置 |
|---|---|
| Path六处纵向步进 | retained with reason：未修改Path行；本轮未复核该处现状。 |
| Path两列分割 | retained with reason：未改Path列宽。 |
| Path按钮高度 | retained with reason：未改Path按钮或命中区。 |
| Path自绘数值背景 | retained with reason：未改字段背景。 |
| Path布尔宽按钮 | retained with reason：未改Path布尔；本项锁轴使用原生Checkbox。 |
| Effect Controls行高/步进 | retained with reason：只改效果时间图，未改属性树行。 |
| Effect Controls fx布尔 | retained with reason：未改效果开关，不冒认主分支整改。 |
| Effect Controls枚举 | retained with reason：未改枚举控件或点击语义。 |
| Effect Controls手工hover | retained with reason：未改hover，没有新颜色。 |
| Effect Controls层级 | retained with reason：未改效果属性树层级。 |
| Timeline标签栏列坐标 | retained with reason：本人未改标签栏；继承主分支开关列。 |
| Timeline吸附线颜色 | retained with reason：未改吸附反馈或既有颜色。 |

以上保留处置不是“缺陷已修复”声明。[原始证据包](data/curve-effect-ranges-stage-06.zip)：41项/2835807字节，SHA-256 `fcd21a430ca5fc957315ea0628107263258c7f865b6602c86d6e04a2ab5b9c6b`；[结构化结果](curve-effect-ranges-progress-results.json)。
