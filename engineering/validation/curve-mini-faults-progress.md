# Mini默认故障信道、缓存恢复与主分支接续（#259，未完成）

接续[Mini范围阶段](curve-mini-ranges-progress.md)。当前实际编译源码 `bbab3144deb6f9ddeec966c94a3f4ced58138372`、二进制SHA-256 `ee442fe39683e69b0a9f7c8a4dd0a6a693982751a1aac3750ef2ac284e3226fd`，Rust/WGSL/Cargo/样式来源哈希前后相同；本阶段131个不同桌面用例通过，仍未完成完整#259。无新增依赖或第三方资产。

## 交付与真实合并边界

[PR #519](https://github.com/MAD-Producer/Kilonova/pull/519)五项非跳过检查全部通过；三平台Desktop日志各确认130项既有相关回归。五项实际checkout同为7a4ae98541f7c2d47e795760cd73900f73c2c633、树1df38885a6ba3b37ae9f61cfdf97e5c3c6d7e586，与当时完整合并结果相同后才执行中文squash。实际提交 `72d23d740f7885d7a3c1d118fd5e547564e3ccc1`，唯一父为4184f0c8，树与预检相同，#263自动关闭。仅自己的远端codex/viewer-guides按精确head租约删除，保留本地工作树。

曲线分支合入该主分支时，DESIGN/keyboard/mod的冲突保留完整范围规则、输入门控和overlay；完成文档保留主分支新增场景验证链接。逐块检查后，三个产品/设计文件与合入前本分支的规范化内容一致，没有借机删掉新菜单路由；再编译运行131项。当前与main差异没有参考线模块或crates文件。这只证明本阶段接续和所列回归，不把#519 CI冒称#259 CI或完整组合验收。

## 默认信道实际证据

新增测试直接走View.draw、实际Data解码、Core导数和物理单位采样，不调用假的observe结果或改模型返回值。删除MOTION_LOG覆盖，以默认Info运行两次独立子进程。健康分支先显式flush，然后核对只有logger初始化Info一条；正常缺失通道/无输出区间不计为故障。

| 站点 | 实际触发 | 验证 |
|---|---|---|
| curve.mini.fit_all | 零高度绘图区造成真实InvalidGeometry；后续更换几何、恢复有效几何 | 相同失败几何缓存；成功后恢复折线。 |
| curve.mini.readout_load | provider快照关键帧frame改为not-a-frame，实际serde/Track解码失败 | 相同失败来源120次仍只一次观测；换revision重新加载，恢复正确数据。 |
| curve.mini.readout_fps | provider快照fps=0 | 显示实际读数不可用，正确fps恢复物理单位。 |
| curve.mini.readout_sample | provider携带有限但非法的1e308fps，真实导数转f32物理样本溢出 | Data返回原始non-finite错误，不显示部分成功卡片，正确fps恢复。 |

零几何是显示边界，0/1e308fps是降级provider快照故障，不声称是合法工程值。每站点在文件保留首Warn和进程寿命累计第60次Error，此后抑制；交错成功不重置既有寿命计数。stderr恰好各站点1条Error、没有Warn；stdout没有诊断。每进程4条Error、两进程独立从首Warn起步，证明进程重置。保留原始not-a-frame、fps缺失、模型non-finite和geometry原因；新增phase、目标、generation/revision与可测的耗时上下文。没有新增线程、重试、恢复动作或改变模型返回值。

新增正常行为验证覆盖未变视图120次复用几何、缩放仅重建折线、尺寸/模式变更、移出归一化域的正常空折线；选区区分Unowned/Empty/Stale和双端点；中英主/浮窗Ctrl X、Ctrl+Alt Y指针锚定缩放、Shift X平移，面板滚动和共享时间轴不动。用例总数131包含一次父测试，两个故障子进程不是额外两个测试数。

stage14也通过131项，并显式重跑既有忽略的原生剪贴板回归1项/6截图（源码6230f8f3）。stage15在合入main并强化flush核对后重编译运行131项；没有在stage15重跑该原生用例，保留其准确来源，不用旧截图证明新增菜单原生验收。旧stage13及更早证据仍是历史记录。

## 完整站点处置表

以当前main72d23d74的同名调用点为原有覆盖，四个相关渲染器共计20个新增站点，4个已实际验证默认信道、16个待故障证据或有根据的边界处置；没有把待验证站点标成修复。旧加载故障、纯细分测试或本阶段四站点不能替代其证据。

| 站点 | 当前源码位置 | 处置 |
|---|---|---|
| `timeline.graph_early_return` | `desktop/app/src/panels/timeline_graph.rs:118` | 原有，只记回归覆盖 |
| `timeline.graph_samples` | `desktop/app/src/panels/timeline_graph.rs:359` | 原有，只记回归覆盖 |
| `timeline.graph_view_bounds` | `desktop/app/src/panels/timeline_graph.rs:391` | 新增，待验证/边界处置 |
| `timeline.graph_adaptive` | `desktop/app/src/panels/timeline_graph.rs:443` | 新增，待验证/边界处置 |
| `timeline.graph_key_sample` | `desktop/app/src/panels/timeline_graph.rs:526` | 原有，只记回归覆盖 |
| `timeline.graph_tooltip_sample` | `desktop/app/src/panels/timeline_graph.rs:644` | 原有，只记回归覆盖 |
| `timeline.graph_fit_all` | `desktop/app/src/panels/timeline_graph.rs:704` | 新增，待验证/边界处置 |
| `timeline.graph_fit_selection` | `desktop/app/src/panels/timeline_graph.rs:709` | 新增，待验证/边界处置 |
| `effect.graph_samples` | `desktop/app/src/panels/effect_timeline/graph.rs:343` | 原有，只记回归覆盖 |
| `effect.graph_view_bounds` | `desktop/app/src/panels/effect_timeline/graph.rs:374` | 新增，待验证/边界处置 |
| `effect.graph_adaptive` | `desktop/app/src/panels/effect_timeline/graph.rs:448` | 新增，待验证/边界处置 |
| `effect.graph_key_sample` | `desktop/app/src/panels/effect_timeline/graph.rs:516` | 原有，只记回归覆盖 |
| `effect.graph_readout` | `desktop/app/src/panels/effect_timeline/graph.rs:723` | 新增，待验证/边界处置 |
| `effect.graph_fit_all` | `desktop/app/src/panels/effect_timeline/graph.rs:771` | 新增，待验证/边界处置 |
| `effect.graph_fit_selection` | `desktop/app/src/panels/effect_timeline/graph.rs:776` | 新增，待验证/边界处置 |
| `timeline.effect_curve` | `desktop/app/src/panels/effect_timeline/graph.rs:896` | 原有，只记回归覆盖 |
| `curve.comparison.samples` | `desktop/app/src/panels/curve_comparison/view.rs:355` | 原有，只记回归覆盖 |
| `curve.comparison.view_bounds` | `desktop/app/src/panels/curve_comparison/view.rs:400` | 新增，待验证/边界处置 |
| `curve.comparison.adaptive` | `desktop/app/src/panels/curve_comparison/view.rs:505` | 新增，待验证/边界处置 |
| `curve.comparison.readout` | `desktop/app/src/panels/curve_comparison/view.rs:532` | 新增，待验证/边界处置 |
| `curve.comparison.key_sample` | `desktop/app/src/panels/curve_comparison/view.rs:555` | 原有，只记回归覆盖 |
| `curve.comparison.fit_all` | `desktop/app/src/panels/curve_comparison/view.rs:642` | 新增，待验证/边界处置 |
| `curve.comparison.fit_selection` | `desktop/app/src/panels/curve_comparison/view.rs:646` | 新增，待验证/边界处置 |
| `curve.mini.fit_all` | `desktop/app/src/panels/animation_controls/mini.rs:247` | 新增，默认信道实际验证 |
| `curve.mini.adaptive` | `desktop/app/src/panels/animation_controls/mini.rs:398` | 新增，待验证/边界处置 |
| `curve.mini.normalized_readout` | `desktop/app/src/panels/animation_controls/mini.rs:455` | 新增，待验证/边界处置 |
| `curve.mini.readout_load` | `desktop/app/src/panels/animation_controls/mini.rs:514` | 新增，默认信道实际验证 |
| `curve.mini.readout_fps` | `desktop/app/src/panels/animation_controls/mini.rs:544` | 新增，默认信道实际验证 |
| `curve.mini.readout_sample` | `desktop/app/src/panels/animation_controls/mini.rs:575` | 新增，默认信道实际验证 |

## DESIGN §0.7逐项处置及余项

本阶段改观察上下文和测试；不改面板行/颜色/尺寸，既有12处继续逐项保留，不宣称修复。

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




完整新范围菜单原生中英主/浮窗、最小/default/DPI、高倍率、IME/失效/取消、实际保存文件字节、性能、最终all-targets/main组合和#259自身CI仍待完成。完整DESIGN12未通过，不提交完成PR或关闭#259。

[结构化结果](curve-mini-faults-progress-results.json) · [原始证据包](data/curve-mini-faults-stage-15.zip)：69项/965269字节，SHA-256 `3ec0208b51f8dcf8f706d83be953d4d71ff549e3b82486585091f800f12812bf`。
