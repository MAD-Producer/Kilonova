# Mini曲线范围和实际区间读数阶段（#259，尚未完成）

接续[对比图阶段](curve-comparison-ranges-progress.md)。源码 `4d7a49ed09930db06c48878a65a79b69d173428f`，实际桌面二进制SHA-256 `7b0393bcec2ae9747ad8f2bba090712e2d36d2d538f43a8f847b0a64170ca27f`，Rust/WGSL/Cargo/样式来源前后哈希一致。开始前重新核对开放PR只有本人#519，已有curve-comparison、curve-view-controls、easing-clipboard分支能力已在基线中；本项接续现有曲线功能，没有重复其功能、新增依赖或第三方资产。

## 实现与语义

Curves面板使用共享临时范围状态和同一原生菜单：适配全部/选区、自动范围、原生Checkbox锁X/Y。Mini X/Y仍是归一化缓动编辑坐标，标题和锁轴标签明确标出；Mini X平移/缩放不会改共享时间轴zoom/offset或锁状态。Ctrl滚轮缩放X、Ctrl+Alt缩放Y、Shift滚轮平移X，指针锚定；在Mini内使用这些操作不同时滚动面板，原Ctrl/Shift以外的面板滚动保持。

有效相邻关键帧的悬停读数把归一化t映射到其合成时间，再复用对比图既有Data实际模型采样、单位、分量和物理速度，不重复图层偏移。显示归一化坐标与实际读数的区别；耦合向量保留全部可见分量，不悄悄只显示X。末帧/没有有效输出区间时只显示归一化读数，不编造时间跨度、帧号或物理单位。原控制柄仍提交原Curve命令和事务；正反坐标变换使用当前X/Y视图，并在拖动期间冻结两轴，保留原10逻辑像素命中半径和5像素标记尺寸。

保留原80间隔作为种子，由共享有界屏幕误差细分器决定最终折线，包含Hold左极限和Elastic周期种子。自动范围包括实际细分极值和模型控制柄。形状/几何/容差缓存与最终视图折线缓存分开；计算失败缓存不可用状态，不返回部分成功。实际模型读数按来源/目标/语言缓存。Mini菜单额外捕获选中区间起始键，外部改变该键时取消并消费当前输入。所有范围都留在临时UI状态，不添加工程字段或历史。

新增尺寸先写dark.json：graph.mini.height=130、graph.mini.handle.hit_radius=10；其他边框、标题、标记、宽度、文字和菜单继续使用已有令牌。归一化t、范围padding、模型坐标限制和采样预算是数据/算法量，不是像素尺寸。未触及属性树行，也未增加按钮式开关/枚举。

## 实际结果与保留失败

当前 127 个不同桌面用例全部通过：细分7、真实模型采样4、原时间轴50、手柄2、范围菜单20、轴7、效果16、对比14、Mini及原控制器7。新增覆盖中英主窗/浮窗chrome受控输入、局部双轴锁、播放/滚轮/面板滚动隔离、选区适配、IME/跨窗/改键取消、末帧与实际百分比读数、缩放后拖柄和Esc恰好一次取消；模型测试覆盖零/负幅度、15/60帧时长、耦合分量、来源失效、10000点自动范围与各段128点的0.36逻辑像素界限。

stage10初版编译因四处把observe返回的Result当Option使用而失败；保持返回错误语义，明确观测后的不可用缓存。stage11编译成功，126通过/1失败：新测试发现原生浮窗chrome没有主窗那条Esc取消路径，Mini拖柄没有发出History(4)。修正Mini控制器自己的Esc/失焦处理，消费取消帧并保留重绘；主窗已先取消时不重复事务。stage12保留同一断言全部通过。没有修改既有断言或把新断言放宽成只检查editing标记，原失败日志完整收录。

stage12原生截图发现24像素范围入口的文字省略号未显示，只剩空按钮；未用截图反推已验证的字体根因。stage13共享入口改用仓库已有Icon::More原生图标，保持24像素入口与右键入口，未新增图标资产。重新运行上述完整用例与原生剪贴板截图；实际图标可见性以本阶段截图核对，不把编译成功当作视觉验收。截图还保留本机Vulkan加载器报告的缺失Epic覆盖层JSON诊断；测试退出0，不据此声称stderr完全安静。

另外显式运行现有忽略的原生GPU剪贴板回归，1通过、6张中英主窗/浮窗截图，覆盖既有Mini复制/粘贴按钮的目标、输出区间、IME/失焦门控和工程revision/history不变。实际隐藏原生窗口、生产Input注入和受控缩放，未测试物理OS输入/系统DPI设置。它证明这条既有回归，**不替代新增范围菜单的原生/DPI验收或实际保存文件字节验证**；普通用例与这1条原生用例分开计数。

## 观察与待完成项

新增curve.mini.fit_all/adaptive/normalized_readout/readout_load/readout_fps/readout_sample复用既有固定站点进程寿命Counter：首Warn、累计第60次Error、之后抑制，进程结束清零，不是连续计数或一秒。缺失/不支持的Data正常不可用状态不作为故障；实际解码/采样失败保留原因。默认文件/Error stderr/stdout实际审计尚待完成，其他图旧故障测试不作替代。没有新增worker、重试或重启。

新增范围菜单的真实原生中英主/浮窗、最小/default/DPI、高倍率视觉、缓存/取消/失效边界、实际文件字节、性能和当前主分支完整组合检查仍待完成，完整DESIGN12未宣称通过；不据此开完成PR或关闭#259。

## DESIGN §0.7逐项处置

本轮未改这12处；继续保留，原因如下。保留处置不是已修复缺陷声明。

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


[结构化结果](curve-mini-ranges-progress-results.json) · [原始证据包](data/curve-mini-ranges-stage-13.zip)：79项/1871797字节，SHA-256 `376243301217cb3aeb60123beb54089fc6325ce09c3f1de1f83c4af8eff96182`。
