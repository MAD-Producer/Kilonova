# 属性曲线范围菜单阶段验收（#259，未完成）

本阶段仅接入普通属性时间图的适配全部/选区、自动范围、X/Y锁轴和菜单输入归属。效果、对比、Mini图、读数统一、实际原生截图/高倍率视觉验收和新故障站点信道审计仍未完成；不提交完成PR或关闭#259。早期细分算法与原始失败见[前三轮记录](curve-view-ranges-progress.md)。

实际源码提交 `7b66d54cd8fc3909fdf3379edf5851e7af9c7020`，二进制SHA-256 `b8a9a51e7aac9ddf997b77c69269f5163d03a15c4f53a257955440ac0e0a1a12`，Rust/WGSL/Cargo/样式来源前后哈希一致。分支已接续主分支模型图层、深度任务、图层独立开关和本人参考线分支；开始前核对其他分支与PR，没有重做其功能。

## 实现和已验证范围

普通属性图头部提供范围菜单，保留原模式按钮和命中范围；窄图头可用右键打开。锁轴使用原生Checkbox，键盘跳过不可用项，输入法与按键重复不能翻转锁轴。范围按文档/合成/代次及通道、分量、模式、单位保存为临时视图，最多128项；文档几何或代次改变清除。X锁阻止时间缩放、水平滚动、导航拖动和跟随，Y锁捕获当前单位范围；自动范围只恢复未锁定轴。

选区时间读取合成坐标，不重复添加图层偏移；已拥有但为空/失效的选区不退回旧单键。适配用实际模型值/物理速度和有界屏幕细分准备范围，常量/单键仍给出有限跨度。普通属性图宽度与共享时间标尺一致，移除二次扣除的右侧间距。菜单、锁轴和导航没有新增工程命令、历史事务、线程、重试或重启。

本轮实际桌面程序共 76 个不同用例通过：细分 6、模型/物理速度 4、既有时间轴 50、既有手柄 2、范围菜单 7、轴原语 7。独立脚本中重复的轴/细分测试不累加。菜单测试含真实主窗/浮窗chrome的受控输入路径、中英文、原生Checkbox、Esc/外部点击/跨窗/失焦/过期来源/选区/轨道变化、单键合成偏移、锁轴导航以及序列化工程字节与撤销状态不变。它们没有创建本项原生窗口或截图，不能替代最终原生视觉验收；本轮未执行实际保存文件的字节比较。

新增文件/Error stderr/stdout故障验证尚待完成。`timeline.graph_view_bounds`、`timeline.graph_fit_all`、`timeline.graph_fit_selection`与先前细分站点复用既有固定站点进程寿命Counter：首Warn、累计第60次Error、其后抑制，进程结束清零；不是连续次数，也不等同于秒。正常范围缺失/取消/失效保持正常分支，范围准备故障保留原始cause，禁止部分成功。已有GraphBox或参考线故障测试不冒充这些新站点的覆盖。

## DESIGN §0.7逐项处置

以下是本阶段责任边界，不将保留处置冒称缺陷已修复；完整§12验收仍待完成。

| 基线 | 处置与原因 |
|---|---|
| path_controls六处纵向步进 | retained with reason：本项未改该文件，独立路径行整改不纳入曲线范围菜单。 |
| path_controls两列分割 | retained with reason：本项未改列宽或值字段。 |
| path_controls按钮高度 | retained with reason：本项未改按钮和原命中容差。 |
| path_controls自绘数值背景 | retained with reason：本项未改数值字段绘制；本轮未复核现状。 |
| path_controls布尔宽按钮 | retained with reason：本项未改路径布尔；新锁轴使用原生Checkbox。 |
| effect_controls行高/步进 | retained with reason：本阶段效果图接线未完成，未改效果属性行。 |
| effect_controls fx布尔 | retained with reason：本项未改效果开关；不重报主分支既有整改为本人修复。 |
| effect_controls枚举宽按钮 | retained with reason：本项未改枚举控件。 |
| effect_controls手工hover | retained with reason：本项未改该处，也没有新增弱hover颜色。 |
| effect_controls层级 | retained with reason：不在本阶段曲线视图范围接线内。 |
| timeline标签栏列坐标 | retained with reason：本人未改标签栏；继承主分支图层开关列改动，不以本项名义整改兼容基线。 |
| timeline吸附线颜色 | retained with reason：本项未改既有吸附反馈或颜色。 |

新菜单宽度先定义于dark.json；尺寸和颜色读取语义令牌，未新增依赖或第三方资产。没有改变旧测试断言或缩小命中容差。完整原输出、源码冻结和脚本见[阶段证据包](data/curve-range-menus-stage-04.zip)（11项，SHA-256 `d6257c444cc4068fbf16815620cc9256f3e226bcb013caddb1cdbbb175ef8017`）及[结构化结果](curve-range-menus-progress-results.json)。
