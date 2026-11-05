# 完整行字素光标、选区与语法着色验收

本批继续处理 #129 的文字编辑一致性，基于 #609 的 `5468e210`，最终实现源为 `56d55e78`（首轮完整回归源 `a8ec5ff9`）。不关闭整个缺陷合集，不改面板排列、主题值、快捷键路由、字体选择、旧工程兼容或图形预算。

## 原因与最终行为

原单行光标单独测量源前缀；多行点击和选区也反复塑形前缀；语法高亮把每个颜色片段独立塑形。因此 AV 字距、从右到左文本和阿拉伯文连写会与已绘制的完整行不一致。修复共享整行塑形结果：每个扩展字素保留逻辑起止对应的视觉位置，点击选择最近合法字素停点，逻辑选区绘制独立视觉区段，语法颜色附着于原字形簇。单行实际选区可替换、删除、Shift 扩选和撤销；超宽编辑内容按光标滚动，非编辑标签仍使用原截断路径。

制表符保留源字节与展开文本的映射，无制表符路径借用原文本。塑形器可能把变体选择符或其他不可见控制拆成子簇，必须归一到完整字素；不能要求这些子簇本身就是编辑边界。连字内部按原总前进量等分到源字素，不重新塑形，也不在 emoji 序列内部增加停点。

语法回调只能提供展示颜色。它返回的片段若不再等于原文本，生产路径记录有界 `editor_syntax` 故障并绘制原文，防止替换源字形。此前仅有 Debug 断言；这是一项明确的生产行为修正，不能当作新增日志本身。回调自身 panic 继续传播，未新增捕获、重试、重启或修改默认 panic hook。

## 红绿证据与范围

归档 `ui-text-caret-evidence.zip` 包含执行参数、源哈希、原始 stdout/stderr、结果 JSON 和实际 GPU PNG；聚焦结果都是完整结果子集，不相加。

| 项目 | 原实现与修复证据 |
|---|---|
| C1 单行光标 | `baseline` 在原生产源下实际 0 过、2 失败：纯 RTL Home/End 与 AV 完整运行前进量。字体覆盖、运行方向、真实字距前提先通过；修复后原断言通过。 |
| C2 编辑与高亮 | `render-baseline` 在相同原生产路径下实际 0 过、3 失败：颜色片段破坏整行几何、RTL 点击边界错位、单行 emoji 选区未被替换。修复后原断言通过；保留原始失败。 |
| C3 既有变体选择符 | `expanded` 中既有 `unsupported_variation_sequences_do_not_add_fallback_spacing` 实际失败。修正子簇归一后原断言通过，没有删改原断言或容差。 |
| C4 公共布局 API | 五项独立测试覆盖组合标记、补充平面变体选择符、连接控制、emoji/旗帜的字素停点、非有限点击、实际 ffi 连字和混合双向选区。emoji 停点正确不等于彩色 emoji 实际可绘制。 |
| C5 实际 GPU | `editor_rendering` 的实际控件与独立整行参考进行五档 DPI × 两种宽度 × 四种情形，共 40 组 RGBA 逐字节比较；三种光标旧算法共 30 组负对照必须可见地失败。参考光标取完整运行字距/纯 RTL 两端，不调用新 `caret_x`；溢出参考保持整行并独立裁剪。 |
| C6 默认故障信道 | 子进程移除 MOTION_LOG，使用原 Info 文件与 Error stderr 接收器。真实超长布局、非法字素边界、错误语法回调各调用 120 次；成功布局穿插、clear 不重置实例累计；每 key 第一次 Warn、第 60 次 Error，后续抑制。替换图集重新计数。8 行文件、4 行 Error stderr，stdout 与用户文本隔离。 |
| C7 既有焦点输入 | 首次整套 `full-initial` 在 `framework::unicode_cursor_and_selection_do_not_split_utf8_or_erase_on_focus` 实际失败，执行到该目标累计 169 过、1 失败并停止，不能称整套通过。原因是把初始默认全选也改成了“左键折叠到开头”；修复保留初始字段原有的逐字素导航，仅显式局部选区折叠到边界。原测试与所有断言不变。原包装脚本只统计成功目标，另存 `full-initial-observed-counts.json` 补上失败目标计数，原输出不覆写。 |

早期 `corrected` 和 `final-focused` 的公共集成测试确实通过，但其 116 项库测试清单缺少新增单元测试，也缺少当前父分支的数值/裁剪测试，不能作为当前源码验收。共享构建目录复用了父分支库测试产物；保留原始输出，明确废止这两份结果的库测试结论。随后强制重新编译并检查测试名称，`fresh-unit` 的 121 项实际库测试通过，包含新增信道和数值/裁剪测试。最终整套还要求关键测试名称都出现在实际输出中，并对运行前后源哈希核对；不靠 Cargo 退出码推断新增测试已执行。

最终冻结源 `56d55e78f1b8adfa9aa49eb1a4460c81869308eb`：`cargo test -p motion-ui -- --nocapture` 31 个目标、283 项通过（含 1 项 compile-fail 文档测试），零失败、零忽略、零过滤，229.75 秒；五个关键测试名称清单均实际出现。`cargo check -p motion-desktop --features ffmpeg` 退出 0，36 秒。完整源在运行前后哈希一致，随后只有验收文档和 PNG/zip 归档变化。

`--nocapture` 输出中的 panic 来自既有数值格式化、剪贴板投毒、回调传播及非法字形索引负对照；文档编译错误来自应失败的 compile-fail 用例，各测试均明确验收对应失败。原渲染与桌面未使用项警告保留，本批没有隐藏警告或删改门禁。默认错误信道子进程原始文件和 stderr 都在归档中。

归档 205 个成员，420911 字节，SHA-256 `6bd00716b92f9a52cb91420d93ef6511f55b3878f74d4e112bbaa5daea94cd21`；已逐成员读回核验、检查 CRC 与哈希。最终 70 张 GPU 原始 PNG 与首轮同场景输出逐字节相同；初始全选输入修正不改变这些绘制情形。

## 新观察、既有覆盖和剩余边界

`editor`、`caret_boundary`、`editor_syntax` 新故障路径有上述实际默认级输出。`editor_pointer`、`expanded_boundary`、`editor_selection` 是输入/内部映射完整性观察，复用同一有界实例计数器；当前未逐一注入这些边界，不宣称已完成各自故障验收。合法的查询 None、空文本和正常撤销空栈不是故障。

计数沿用既有图集生命周期语义，健康布局和清图集不清计数；替换图集重置。不是“连续 60 次”等同一秒。沿用最多 128 个观察 key、128 字符 key 上限和原有错误上下文；不记录完整源文本。正常绘制不新增文件 IO、告警或全局锁。新逻辑没有工作线程/信道或可能永久等候的新生产操作，原栅格化和字体源故障观察仍保留。

每个缓存行增加每字素两个 f32 及其 Vec 容量，已纳入原布局/截断缓存字节核算，原 128 条、1 MiB 限制不变。冷缓存增加分配；不宣称全帧零分配、所有 emoji 字体支持、整个窗口时延改善或峰值内存下降。双向交界使用“后一个逻辑字素的前沿，EOF 取前一个后沿”的单一停点，不实现双光标 affinity、完整视觉方向键导航或字体 GDEF 连字专用停点；这些边界继续开放。

## 画面与 DESIGN 验收

`ui-text-caret-native/before-after.png` 由真实控件 GPU 读回拼接，左侧仅替换成旧前缀测量光标，右侧为本次实际 TextEditor。宽窄两种字段、默认主题和字体、100/125/150/175/200% DPI PNG 全部在归档；夹具字段高 40 逻辑像素，未改产品行高。它是组件级实际像素证据，不是本次完整客户端窗口、IME 候选窗或每个面板的原生交互验收。无右键菜单变化。

以下 12 项 DESIGN §0.7 全部 **retained with reason**：本批不调整面板布局或 token。三个面板文件、dark.json、DESIGN.md 与父源换行统一后字节相同，见 `scope-identity.json`。保留不代表既有违规已修好。

| 基线条目 | 处置与理由 |
|---|---|
| path_controls 六处纵向步进 | retained with reason：未编辑行高 |
| path_controls 双列切分 | retained with reason：未编辑列宽 |
| path_controls 混合按钮高度 | retained with reason：未编辑尺寸或命中 |
| path_controls 自定义数值背景 | retained with reason：未编辑背景 |
| path_controls 布尔全宽按钮 | retained with reason：未编辑开关 |
| effect_controls 行高与步进 | retained with reason：未编辑效果排列 |
| effect_controls fx/— 布尔按钮 | retained with reason：未编辑布尔交互 |
| effect_controls 枚举全宽按钮 | retained with reason：未编辑下拉入口 |
| effect_controls 手工 hover alpha | retained with reason：未编辑颜色状态 |
| effect_controls 缺少层级 | retained with reason：未编辑属性树 |
| timeline 固定列位置 | retained with reason：保持基线坐标 |
| timeline 吸附线颜色 | retained with reason：未编辑 token |

无新增依赖、字体文件、第三方资产或兼容名称；复用已有系统字体、绘制和测试依赖。#129 其他项目与最终跨平台 CI 继续独立验收。
