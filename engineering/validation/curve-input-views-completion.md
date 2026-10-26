# 曲线工具栏借用输入数据验收

Refs #129、#158。基线 `a9a26d5c52588138033a6dd346471ed6cf47673f`，代码 `8bdbd7bf8f16a5437f68acdeddf0e0a35b179d81`。最终主分支组合以 PR 当前提交和检查为准，不用旧基线结果代替新组合。

## 改动与边界

工具栏与通道弹层在 composing、ime_consumed 或 focus_lost 时，仅为清空 keys 而 clone 整帧 Input，连带复制文本、重复标记与按键向量。现在使用既有 InputView，仍只清空 keys，其余标量、文本、重复标记、按住/释放键和剪贴板借用规则沿用框架接口。静默及打开弹层的空输入改用等价的 InputView 默认值；私有导航辅助函数接收视图。

已有滚动路径本来就是 begin_view，本次没有将它登记为新优化。无控件、几何、token、命中区、状态机、日志策略或模型格式变化，无新依赖或第三方资产。没有跨帧保存借用对象，也没有改变原先仅过滤 keys 的语义。

## 实际测量与交互

128 组完整主窗/浮窗 chrome，覆盖中英文、900×600/1600×1000、弹层开/关、正常/composing/IME consumed/失焦，以及空载荷和 81920 字节文本压力载荷。每组先热身 3 帧，再测 10 帧，共 1280 个测量帧。128 份完整 PaintList、控制器状态、动作与工程快照逐字节相等；每帧输入与工程保持不变，未发编辑动作。

分配请求减少 3600 次、申请字节减少 58996800，所有场景均未增加。正常输入场景的减少为 0，收益来自三个过滤状态。数字包含大文本压力载荷，不能推断典型帧、常驻内存、峰值内存、GPU 时间或 FPS，也不宣称整个绘制过程零分配。

前后测试均确认从对应工作树实际编译桌面程序，固定相同探针与场景，不复用其他工作树的旧程序。计数器仅统计被测线程里的分配请求和申请字节，不统计释放或存活内存；绘制和字体等其他分配仍包含在帧记录中。临时探针、全局分配器及测试模块已全部移除，原 tests.rs 按字节恢复。

最终产品源码另行编译后，既有比较功能回归 10 passed / 0 failed / 1 ignored。该忽略项为原生 GPU 捕获，随后通过显式 --ignored 单独执行，1 passed / 0 failed / 0 ignored；不是将忽略计作通过。生成 13 张中英、默认/最小窗口、比例 1/1.5、值/速度、弹层及原生浮窗截图，全部与基线归档 curve-comparison-native-final 逐字节一致。检查了中文弹层与浮窗原图，未重绘截图。两阶段使用同一最终程序哈希，源码不变。

## 错误、正常分支与剩余范围

- CIV-01 新观察：无。输入法、失焦和空输入都是正常分支，没有新日志或文件 I/O。
- CIV-02 既有观察：现有曲线解码、采样与样式错误信道未改；既有错误/失效回归包含在上述 10 项中。没有以本次正常绘制测量证明所有故障站点均已验证，也未改变断言、抑制策略或重试。
- CIV-03 运行环境：原生日志保留本机 Vulkan 可选 overlay 注册文件的既有 Error，不宣称整个环境日志无 Error。GPU 捕获使用生产 Input 注入，未注入系统物理键鼠，不替代完整系统 IME 验收。
- CIV-04 剩余范围：view.rs 的其他输入克隆、跨帧保留所有权的输入快照，以及 #129/#158 其他待验事项未由此修复，两个集合保持开放。没有宣称整个 DESIGN §12 或原 40/90 秒桌面完整脚本已通过。

实施前核对分支引用及具有该文件的工作树，没有其他移除这两处 clone 的方案。初始去重守卫发现已有 begin_view，核查后确认它是既有滚动路径；据此缩小事实描述，没有重复登记已有实现。

## DESIGN §0.7 逐项处置

| 基线条目 | 状态 | 原因 |
| --- | --- | --- |
| path_controls 六处行高 | retained with reason | 未改路径行节奏 |
| path_controls 双列宽度 | retained with reason | 未改列宽 |
| path_controls 按钮高度 | retained with reason | 未改尺寸和命中区 |
| path_controls 数值背景 | retained with reason | 未改字段绘制 |
| path_controls 布尔控件 | retained with reason | 未改控件或事件 |
| effect_controls 行高与步进 | retained with reason | 未改效果行布局 |
| effect_controls 布尔按钮 | retained with reason | 未改开关 |
| effect_controls 枚举按钮 | retained with reason | 未改下拉 |
| effect_controls 悬浮 alpha | retained with reason | 未改颜色或状态 |
| effect_controls 缩进层级 | retained with reason | 未改属性树 |
| timeline 标签列坐标 | retained with reason | 保留兼容基线 |
| timeline 吸附引导颜色 | retained with reason | 未改颜色或 token |

## 证据

[data/curve-input-views-a9a26d5c.zip](data/curve-input-views-a9a26d5c.zip)：12003747 字节、300 项，SHA-256 `61a688acdd9660b19ce2a42528235b1918bc52c2a0c4fbf169441a44095fd232`。包含前后完整绘制快照、原始命令/日志、实际编译来源、输入原件、最终源码和 13 张真实 GPU 截图。结构化数据见 [curve-input-views-results.json](curve-input-views-results.json)。
