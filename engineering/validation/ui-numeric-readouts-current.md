# 数值读数的格式化复用

关联 #129，基于 #608 的 `cb95f451`。这批只移除绘制数值时反复创建的临时字符串，不关闭整个 issue。

## 实现与边界

`TextAtlas` 持有一份可复用的格式化缓冲；`NumberEditor`、`scrub_field` 及桌面属性的 passive 数值绘制共用它。每个图集最多保留 1024 字节容量，超过时丢弃缓冲；当前调用方只格式化 f32/f64，精度仍限制为八位，最大固定表示不超过 319 字节。没有扩大标签缓存、字体缓存或塑形计划缓存。嵌套调用独立借出缓冲，不持有全局锁或线程局部可变借用。

固定精度、紧凑两位小数、负零、非有限数和 scrub 的原有去零规则保持。输入草稿、拖拽、提交、取消、命中区域、主题、行高和排列均未改动。需要拥有字符串的既有 `compact_number` / `format_value` API 保留。

格式化失败返回原始 `fmt::Error`，不绘制部分内容；UI 调用点接入既有观测器。绘制回调 panic 原样传播，没有捕获为成功。panic 后下一次绘制可以重新建立缓冲；没有重试或重启线程。

## 直接证据

原始输出与最终源码哈希收录于同目录 `ui-numeric-readouts-evidence.zip`。

归档共 65 个成员、83191 字节，逐成员读回与原始字节相同；SHA-256 为 `8839bb94d8df817c3f28c8ccd5839f268a8d290864188bc0c4f78fe79f8bd9d8`。`manifest.json` 记录成员哈希，`sources/` 保留实际编译源，统一 CRLF 后与提交 `a5703a2f` 的 Git blob 一致。

| 范围 | 原实现 | 本次实现 |
|---|---:|---:|
| 预热后的固定精度 NumberEditor，1 / 200 个 | 1 / 200 次分配 | 0 / 0 |
| 预热后的紧凑 NumberEditor，1 / 200 个 | 1 / 200 次分配 | 0 / 0 |
| 预热后的 scrub_field，1 / 200 个 | 2 / 390 次分配 | 0 / 0 |

这三项使用真实控件路径和当前线程分配计数器，工作集为二十个不同数值重复绘制。原实现下三个零分配断言实际失败，生产修复后原断言通过；没有放宽断言、改预算或关闭默认 GPU。它们不代表二百个不同标签、冷启动或全帧零分配。

新增实际 GPU 对照将 `NumberEditor` 与原拥有字符串的绘制路径比较：五种 DPI × 两种格式 × 两种对齐 × 十种数值/精度，共二百组 RGBA 逐字节相等。参考路径必须产生字形，覆盖负零、舍入、最大值、最小正值、正负无穷和 NaN；最高精度仍为八位。归档中五对实际/参考 PNG 是 GPU 读回，不是完整编辑器窗口截图。

scrub 的格式与裁切另按五种 DPI、两种宽度和十七种数值逐字形位置、大小、UV 和颜色核对原拥有字符串的实现。缓冲单元测试覆盖 0..=8 位精度、次正规数、嵌套绘制、格式化中途失败、超大容量丢弃和回调 panic 后恢复。

提交 `a5703a2f0499305d32b01b0bf2709d041d4b417a` 的完整 `cargo test -p motion-ui`：29 个目标、261 项通过（260 项运行测试及一项 compile-fail 文档测试），零失败、零忽略、零过滤，耗时 233.29 秒。逐文件哈希确认测试期间源码未变化。上述单元、GPU 和分配结果均为完整回归的子集，不累计。

带 FFmpeg 的 `cargo check -p motion-desktop --features ffmpeg` 退出 0，耗时 35.68 秒。原有 `panels::numeric::contract_tests` 七项通过、零失败，两项既有实际原生窗口测试仍按原声明忽略；没有更改断言或忽略项，编译和执行共 500.56 秒。其余集成目标没有匹配该过滤器，不计作已运行测试；这不是完整桌面回归或当前 EXE 的原生窗口验收。

首次命令误把可执行文件名 motion-studio 当成包名，Cargo 在运行检查前以退出 101 拒绝；改用工作区实际包名 motion-desktop，原始输出保留，不将该命令错误算作产品失败或编译验收。provenance 与差异空白检查退出 0。最终提交的跨平台 CI 待 PR 独立执行。

## 错误处理清单

| ID | 类型 | 实际分支与证据 | 剩余边界 |
|---|---|---|---|
| NUM-01 | 新增观测 | 控制 Display 先写入私有探针再返回 `fmt::Error`；绘制回调不得执行；原错误交由 UI 观测器记录 | 原始数值格式化本身不会返回此控制故障；没有声称真实用户操作已复现该错误 |
| NUM-02 | 既有信道复用 | 子进程显式使用 Info sink，文件含首个 Warn 与第 60 次 Error；stderr 只有 Error；stdout 无业务日志；部分探针文本没有进入日志 | 这是组件的实际故障信道控制，不能代替当前完整产品 EXE 的默认 sink 验收 |
| NUM-03 | 抑制与恢复 | 同一图集生命周期计数；首个 Warn、第 60 次 Error，之后抑制。健康绘制恢复且不重置该生命周期计数；更换图集重新计数，两个周期共四条文件记录、两条 Error stderr | 沿用既有图集生命周期计数，不声称连续失败计数或一秒阈值 |
| NUM-04 | 正常路径 | 有限值、零、非有限显示、缓存命中及普通淘汰不新增日志或文件 IO | 布局缓存未命中的剩余分配仍存在 |
| NUM-05 | 功能/协议边界 | 回调 panic 传播断言通过；输入和模型事务由原控件处理，原行为断言保留 | 不新增 panic hook、Present 日志、超时、重试或错误返回值协议 |

`scratch-unit.stderr` 中的 callback probe 是被测试捕获并确认传播的有意 panic，不是未解释的成功运行异常。最初两轮测试夹具编译失败的原始输出也保留：参数次序及私有字段初始化在执行测试前修正；未将编译失败计为产品红绿证据。

## DESIGN §0.7 逐项处置

以下涉及的三个面板文件、DESIGN.md 与 dark.json 均与基线逐字节相同（统一换行后），哈希见归档。保留项不宣称修复，也不把本次 GPU 数字对照当成这些面板的窗口验收。

| 原基线条目 | 本次处置与理由 |
|---|---|
| path_controls 六处纵向步进 | retained with reason：不改变路径面板布局 |
| path_controls 双列切分 | retained with reason：不改变列宽 |
| path_controls 混合按钮高度 | retained with reason：不改变按钮或命中范围 |
| path_controls 自定义数值背景 | retained with reason：不重构该面板背景 |
| path_controls 布尔全宽按钮 | retained with reason：不改布尔交互 |
| effect_controls 行高与步进 | retained with reason：不改变效果属性排列 |
| effect_controls fx/— 布尔按钮 | retained with reason：不改效果开关 |
| effect_controls 枚举全宽按钮 | retained with reason：不改枚举入口 |
| effect_controls 手工 hover alpha | retained with reason：不改变状态颜色 |
| effect_controls 缺少层级 | retained with reason：不调整属性树 |
| timeline 固定列位置 | retained with reason：按原兼容基线保留 |
| timeline 吸附线颜色 | retained with reason：不新增或改动 token |

本批没有新增依赖或第三方资产。复杂双向文字的光标/选择、完整 emoji、全帧分配以及其他 #129 条目继续未完成；父堆叠 PR 和本批最终提交的跨平台 CI 分别验收。
