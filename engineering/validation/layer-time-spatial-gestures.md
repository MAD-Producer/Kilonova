# #257：映射空间关键帧的选区变换

接续 `af44346c` 的模型命令，交付到已有草稿 PR #579。开工核对 PR 与本地最新
分支：#575 负责原生预渲染，#580／#581 负责跟踪与视频重连，没有发现相同选区
变换工作。没有修改或审查这些分支。

## 功能

共享选区 Plan 原先拒绝非默认时钟的空间轨道。现在按稳定 ID 对照 authored
轨道和 timeline 投影，校验局部时刻、合成时刻、固定／Roving 所有权以及图层
时钟。碰撞与零长度区间判断在 authored 局部顺序中进行，候选位移使用带方向
的时钟速率；合成边界则通过实际候选局部时刻投影后检查。倒放后的显示逆序
不会反转路径，也不会把合法移动误判成关键帧越序。

空间键的精确合成时刻加入共享吸附源，保留分数原点。只移动时间的普通拖动可
吸附到可精确表示的播放头；Alt 仍绕过吸附。与局部整数键混选时复用同一公共
时间步长，不分别舍入不同图层的位移。稳定 ID 在预览、提交、撤销和保存中不变。

沿用现有手势所有权：拖动只画预览，松手通过一个 EditorAction 批量提交；
Escape、失焦、文档／代次／时钟变更或锁定时取消，迟到松手和之后的鼠标移动
不能补交旧操作。复制及空间数值方向拖动沿用既有不可用能力边界，不提交部分
选区。本阶段不以此声称所有连续空间编辑入口已经完成。

Plan 仅在准备手势时读取缓存的求解元数据，指针移动不重建空间几何。新增存储
的局部／合成时刻按实际 SolvedKey 大小纳入现有元数据预算，无新观察键、线程、
重试、重启或健康帧日志 IO。

## 验证记录

最终 96 个不同测试通过：新增映射 5、既有选区 77、Roving 计划 10，以及显式
运行的四项原生测试。默认筛选中有 10 项 ignored，随后显式运行其中 4 项；其余
6 项既有人工验收本阶段未运行，完整名称保留在清单，不计为通过。

新映射测试覆盖 40 组方向／语言／窗口／视图组合；另有 20 组不按 Alt 的实际
吸附输入，以及 Escape、失焦、revision、文档代次、时钟编辑、锁定和复制边界。
所有成功操作比较完整工程，检查预览不写入、松手仅提交一次、稳定 ID、单次
撤销／重做和存储重开。混合整数／空间键复用公共步长；不可表示的微调、越序、
越界和不支持的复制在分发前拒绝，没有提交部分通道。

实际 GPU／worker 验收完成同样的 40 组新映射组合，主窗与浮窗使用各自原生
surface，生成 80 张预览／提交截图；每组核对完整工程、单次历史、撤销、重做
和显式保存。两个方向的最终工程在另一进程启动真实 worker 重开，完整工程
一致且历史为空。三项既有空间原生验收另外生成 24 张截图，共 104 张。

- [中文主窗属性行](layer-time-spatial-gestures-evidence/timeline-main-zh.png)
- [英文倒放浮窗对比曲线](layer-time-spatial-gestures-evidence/curve-floating-en.png)
- [中文拖动预览](layer-time-spatial-gestures-evidence/drag-preview-zh.png)

默认等级下的两次损坏投影确实经过用户微调入口：命令未分发、面板显示原始
原因、完整工程不变；文件和 stderr 各记录两条准备失败，stdout 无应用日志。
原生 worker 的非 Present Info start 也保留。原生 Vulkan 启动仍出现机器已有的
Epic Overlay JSON 注册项缺失 Error；原始输出保留，不删除系统注册项，不把
捕获成功等同于这些环境错误已修复。截图中的既有列标题拥挤／省略同样保留。

所有最终组使用同一桌面可执行文件，逐组核对运行前后 SHA-256；四个改动 Rust
文件与最终编译后的源码哈希一致。首次四项通过后因覆盖缺口补充普通吸附测试，
重新编译并用最终文件运行五项及全部本阶段回归，没有混用初次二进制的结果。
带 FFmpeg 的 workspace all-targets check 通过（1 分 26 秒），样式消费者、新增
模块 rustfmt 和 git diff --check 通过。详见
[清单](layer-time-spatial-gestures-evidence/manifest.json) 与
[原始证据包](layer-time-spatial-gestures-evidence/evidence.zip)。

## 错误处理审计

| ID | 分类 | 处置与证据 |
|---|---|---|
| SPATIAL-G-01 | 功能修正 | 接通映射空间键拖动与微调；碰撞使用局部顺序，边界使用候选投影，吸附保留精确合成原点。 |
| SPATIAL-G-02 | 既有信道 | Plan 准备失败沿用 `transform::failed`，包含 operation、prepare 阶段、revision、对象和原始 cause，并显示面板错误。固定键和累计计数器不变。 |
| SPATIAL-G-03 | 正常分支 | 碰撞、越界、混合网格不可表示、复制不支持、取消和空历史沿用正常能力／交互处理，不产生新故障日志。 |
| SPATIAL-G-04 | 既有 worker | 非 Present 在途状态、Info start、原子回滚与默认错误信道不改；没有新阻塞处理器，受控阻塞和 panic 路径沿用前阶段覆盖。 |
| SPATIAL-G-05 | 实际异常 | 新增损坏投影用例，实际调用用户微调入口，核对不分发命令、完整工程不变、错误可见以及默认文件／stderr／stdout 隔离。最终输出见证据包，不以 mocked 成功返回作为证明。 |
| SPATIAL-G-06 | 剩余范围 | 空间轨道的旧 mini 编辑入口、初次局部身份 Roving 转换、伴随补偿以及真实媒体设备／高倍速最终验收仍需推进，#257 保持打开。 |

## DESIGN §0.7 处置

本阶段只改命令准备与已有手势行为，没有新增面板、属性行、尺寸、颜色、图标、
控件或第三方素材；命中半径和吸附容差保留。原生截图证明实际交互，不证明既有
视觉缺陷已经整改。

| 基线位置 | 状态与原因 |
|---|---|
| path_controls.rs 六处行步进 | retained with reason：未改路径行步进。 |
| path_controls.rs 双列分割 | retained with reason：未改路径列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：未改按钮命中区域。 |
| path_controls.rs 自绘数字背景 | retained with reason：未改数字绘制。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：未改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：未改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：未改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：未改效果树。 |
| timeline.rs 十一列坐标 | retained with reason：保留既有列坐标。 |
| timeline.rs 吸附线颜色 | retained with reason：不改颜色，仅接通精确吸附源。 |

§5.2 未增加属性行或缩小命中范围。§12 的全部平台、DPI、布局和既有视觉基线
仍需各自验收，不能由本阶段 Windows 交互结果推定全局合规。
