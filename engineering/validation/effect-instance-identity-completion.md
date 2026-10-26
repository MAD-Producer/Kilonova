# 效果实例标识校验与持续异常诊断

Refs #134。代码与证据冻结于 `805903734e735e028c4fe796c0009baaf99f2100`，基线 `880739de0f7912b8b1683ce72593fad9035d6d24`。这是面板 JSON 快照边界，不表示工程存储模型允许保存缺少 ID 的实例；最终主分支组合的状态以 PR 当前提交和检查为准。

## 行为与边界

- EFFECT-ID-01 功能修复：缺失或非无符号整数 ID 曾在实例循环直接 unwrap 崩溃，高度估算还会保留无效实例的空行。现在以 Option 校验并跳过该实例的绘制和高度；不替换成 0，不删除或改写快照，不改变 Core 工程验证或命令返回值。有效实例仍使用原始数组索引。
- EFFECT-ID-02 新观察：每个 Controls 实例以单个 u8 统计无效条目的遇见次数，最大 60。首次 Debug，恰好第 60 次 Error 一次，此后抑制；完整扫描到合法或空列表后重置，实例列表被选择器遮蔽而未扫描时保留计数。这是条目遇见次数，不是帧数或秒数。仅记录操作、对象、原始索引、阶段与固定原因，无动态键、完整 JSON 或用户文本。
- EFFECT-ID-03 既有覆盖：沿用现有 log 的默认 Info、文件与 Error stderr 路径；不修改日志基础设施、线程/协议、重试或全局观察计数器。正常空列表与健康扫描不新增记录或文件 I/O。
- EFFECT-ID-04 验证：同一份最终回归源文件在原实现上 0 passed / 3 failed，暴露高度 126 对 88 的误差以及两条实际 ID unwrap 崩溃路径；补丁后三项通过。八种异常实例 × 两种语言，16 份完整 PaintList（形状、颜色、字形、顺序及最终 y）与仅有效实例的输出逐字节一致；真实右键上下文保留 `(effect=11,index=1)`，输入快照未改。
- EFFECT-ID-05 实际故障输出：默认 Info 的健康扫描严格保持初始日志原样；前 59 次不新增记录，第 60 次同时写 Error 文件与 stderr；继续 120 次不重复，健康及空列表恢复后分别重新达到阈值。持久化捕获包含一个正常初始化 Info 与三个 Error，stdout 无诊断。不是仅调用 logger 的模拟成功测试。
- EFFECT-ID-06 剩余范围：550 项面板回归通过、1 项既有手动原生关键帧拖动捕获保持忽略。没有修改已有断言；其他 JSON 读取与 #134 的布局/绘制性能事项仍需各自审计，不关闭集合 issue，不宣称 DESIGN §12 全部通过或原 40/90 秒完整脚本通过。

## 证据排除与来源

初始新夹具有一次临时借用编译错误，未运行测试；随后一次夹具把日志初始化与字体准备记录误认作健康扫描异常。编译日志、初始对照、两项功能通过/日志夹具失败的聚焦记录及停止的局部面板运行均保留，不计入最终结果。改正的是新夹具的准备顺序，并先刷新、精确冻结正常初始化记录；其后的健康、阈值、抑制和恢复记录继续完整比较。最终夹具重新在原实现上证明三项失败，再在补丁上通过，已有仓库测试断言未改。

共享 target 切换工作树时主动更新入口时间，并核对实际桌面编译路径；聚焦与完整面板运行使用同一 SHA-256 程序、同一冻结源码。完整面板阶段复用的是本工作树已验证的程序，不把其他工作树的缓存程序当作新源码验证。

实施前核对 439 个引用、27 个相关文件版本及 126 个工作树，均仍含同一未校验读取，未发现重复实现。未新增依赖或第三方资产。

## DESIGN §0.7 处置

合法属性行的尺寸、token、控件和命中区均未改；仅在无效 ID 时跳过对应条目及高度。以下均为 retained with reason，不将其登记为此次修复的视觉缺陷。

| 基线条目 | 状态 | 原因 |
| --- | --- | --- |
| path_controls 六处行高 | retained with reason | 未改行高或纵向节奏 |
| path_controls 双列宽度 | retained with reason | 未改列宽 |
| path_controls 按钮高度 | retained with reason | 未改尺寸或命中区域 |
| path_controls 数值背景 | retained with reason | 未改字段状态绘制 |
| path_controls 布尔控件 | retained with reason | 未改控件与事件路由 |
| effect_controls 行高与步进 | retained with reason | 合法实例原有属性行不变；无效实例不再虚占高度 |
| effect_controls 布尔按钮 | retained with reason | 未改布尔或启用控件 |
| effect_controls 枚举按钮 | retained with reason | 未改枚举控件 |
| effect_controls 悬浮 alpha | retained with reason | 未改 hover 或 token |
| effect_controls 缩进层级 | retained with reason | 未改合法属性树 |
| timeline 标签列坐标 | retained with reason | 保留兼容基线 |
| timeline 吸附引导颜色 | retained with reason | 未改颜色或新增 token |

## 冻结证据

[data/effect-instance-identity-880739de.zip](data/effect-instance-identity-880739de.zip)：98966 字节、32 项，SHA-256 `c63c5bdb81630050e6107ef9a6b86f2c57fc2382fcddb49de70305b3d847567f`。包括真实前后日志、实际故障输出、冻结源码和排除记录。结构化数据见 [effect-instance-identity-results.json](effect-instance-identity-results.json)。新主分支组合须单独核验，不能用此旧基线结果替代。
