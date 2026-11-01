# #257：按局部关键帧身份首次启用 Roving

接续 `0f92528f`，交付到已有草稿 PR #579。开工核对本地分支与开放 PR：
#575 为预渲染，#580／#581／#582 为跟踪、重连及跟踪原型，没有发现重复实现。
没有修改或审查这些分支。#257 仍保持打开，本记录不代表整个 issue 已完成。

## 功能与协议

首次 Roving 转换此前只读取整数合成帧选区，遗漏带符号的局部作者身份。
新增 `SpatialKeyEdit::EnableLocal` 与命令目录中的 `enable_local`：接受严格递增、
互异、最多 16385 个的 i32 局部帧。局部 -9 显示在合成 6.5 或 15.5 时不取整，
不寻找相邻整数键，不修改图层时钟／素材重映射，也不反转作者几何顺序。
既有 `Enable` 的整数合成坐标语义保留，UI 候选按同一模型时钟精确反解。

菜单计划纳入本地选区；普通键、局部键和已转换空间通道混选时保持原子性。
属性面板的旧选区入口也会先认领带身份的选区，再提交同一个 EditorAction。
只有实际返回的文档来源、下一 revision 和完整目标轨道全部匹配，才将选区替换
为稳定空间 ID。用户换选区、取消、换文档或出现无关 revision 时不补交、不改选区。
复用 revision 范围内的计划缓存，空闲菜单不逐帧重算几何。

缺失键、重复／无序地址、首尾键、分离轴、锁定、只读或不支持的混合选区沿用
模型拒绝／能力禁用，不提交部分通道，不把选区回退成图层操作。转换后冻结时间
保留采样结果；撤销恢复完整原工程和作者整数轨道。

## 验证

功能与既有回归共 116 个不同测试通过：连续空间命令 8、既有空间命令 8、
局部菜单 6、既有 Roving 菜单 10、实际 worker 2、选区回归 82。包含中英文、
主窗／浮窗快捷键、正倒放与负局部帧、带小数的合成投影、混合普通／局部通道、
只读／锁定／首尾／陈旧选区禁用、revision 缓存、提交后的 ID 认领及取消。
模型成功用例比较完整工程，覆盖撤销重做、目录保存重开和工程包导出／导入。
实际 worker 验证首个通道转换成功、第二个通道缺失键时完整批次回滚，随后合法
批次只有一条历史。局部转换后冻结的采样逐点相等。

损坏时钟在默认日志等级的真实能力入口重复 90 次、恢复成功、再重复 90 次，
文件及 stderr 各仅一条累计第 60 次 Error；恢复后能力可用，stdout 无应用日志。
正倒放实际 worker 的两次缺失键错误均进入文件及 stderr，包含对象、操作、
enable_local 阶段和原始原因，Info start 保留，stdout 无应用日志或完整用户名称。
正常禁用、取消和空历史不作为故障。本阶段没有改线程、阻塞处理或 panic hook。

原生补图验收通过，合计 117 个不同测试。正倒放 × 中英文 × 主窗／原生浮窗 ×
转换前／Roving／固定三态，生成 24 张实际 GPU 截图；主窗 12 张菜单均断言保持
展开，浮窗 12 张为真实浮窗 surface 的时间轴。转换走实际串行 worker，核对选区
稳定 ID、完整工程与原子历史，截图本身不写工程。查看了菜单前后与英文倒放浮窗：

- [转换前菜单](layer-time-roving-conversion-evidence/menu-before-zh.png)
- [Roving 菜单状态](layer-time-roving-conversion-evidence/menu-roving-zh.png)
- [英文倒放浮窗](layer-time-roving-conversion-evidence/timeline-floating-en.png)

116 项功能／回归使用补图前程序；最终只改原生验收的重新开菜单步骤和断言，
其余八个 Rust 源文件逐一核对 SHA-256 一致，重新编译后显式运行受影响原生项。
两份可执行文件及源码快照分别记录，没有把不同程序冒充同一个哈希。默认筛选
共 11 项 ignored，其中新原生项已显式运行；其余 10 项手工选区验收未运行，
名称保留在[清单](layer-time-roving-conversion-evidence/manifest.json)，不计为通过。
本阶段没有独立进程重开原生工程；模型层实际目录／工程包重开已覆盖，前阶段的
原生独立进程重开不冒充为本阶段新跑的结果。

最终带 FFmpeg 的 workspace all-targets check 通过（53.13 秒）；六个修改／新增
模块 rustfmt、样式消费者和 git diff --check 通过。原生日志仍有机器已有的 Epic
Overlay Vulkan 注册项缺失 Error，原始 stderr 保留，未声称截图成功即环境错误
已修复。既有标题拥挤／省略、浅色主菜单等外观保留，本次没有视觉整改。
原始构建、失败、默认输出、源码及 24 张最终截图见
[证据包](layer-time-roving-conversion-evidence/evidence.zip)。

首轮构建期间曾补充目录和夹具，初次目录测试使用了更新目录前的库产物；
损坏时钟夹具还曾将不同内容放在同一 revision，违背计划缓存的来源契约。
修正新夹具的 revision 后冻结源码重新编译，上述检查全部通过。随后看图发现
选区变更会按既有规则收起菜单，原生夹具增加“重新打开菜单”的步骤和保持展开
断言；仅重建验收程序并补跑受影响的原生用例。没有修改既有断言或生产菜单规则。
初次失败、补图前成功结果及最终原生结果分别保留，不混算重复测试。

## 错误处理审计

| ID | 分类 | 处置与证据边界 |
|---|---|---|
| ROVING-L-01 | 功能／协议 | 增加局部身份转换与目录描述；保留整数入口及完整事务验证。|
| ROVING-L-02 | 新观察 | 新增固定 `animation.roving.clock` 站点，沿用面板观察器的有界原因、键数及进程生命周期计数。首次 Warn、累计第 60 次 Error，成功不重置该站点计数；不把次数解释为秒。|
| ROVING-L-03 | 既有信道 | 选择认领沿用 `choose_addressed` 的面板错误与 Error 日志；异步选区更新沿用已有信道。无新日志 sink、线程、超时、重试或重启。|
| ROVING-L-04 | 既有 worker | 非 Present Info start／在途状态、错误文件及 stderr、stdout 隔离和原子回滚不改；新转换经过实际串行 worker。阻塞与 panic 机制沿用既有覆盖，本阶段不声称重新验证所有处理器。|
| ROVING-L-05 | 正常分支 | 不支持的选区、能力检查、取消、陈旧来源与空历史是正常交互，不新增故障日志。|
| ROVING-L-06 | 剩余范围 | 空间 mini 编辑入口、伴随补偿以及真实媒体设备／高倍速最终集成验收仍需推进；跨平台 CI 未完成前不宣称通过。|

## DESIGN §0.7 处置

本阶段只接通已有菜单及选区命令，没有新布局、行、尺寸、颜色、图标、控件、
第三方素材或依赖。`choose_addressed` 仅放宽模块可见性，错误语义不改。

| 基线位置 | 状态与原因 |
|---|---|
| path_controls.rs 六处行步进 | retained with reason：未改路径行步进。 |
| path_controls.rs 双列分割 | retained with reason：未改路径列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：未改按钮及命中范围。 |
| path_controls.rs 自绘数字背景 | retained with reason：未改数字绘制。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：未改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：未改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：未改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：未改效果树。 |
| timeline.rs 十一列坐标 | retained with reason：保留既有列坐标。 |
| timeline.rs 吸附线颜色 | retained with reason：未改吸附绘制。 |

§5.2 没有增加属性行或缩小命中区域；§12 的完整平台、DPI、布局及视觉基线
仍需各自验收。原生截图只验证本阶段实际状态，不证明整个 UI 已整改。
