# #257：按精确局部时间编辑音量包络

接续 `d66bd83c`，沿用 `feat/layer-time-mapping`。开工核对开放 PR 与近期分支：
#575 为原生预渲染，其他活跃工作为跟踪／相机校准；本阶段只处理音量包络入口。
#257 仍未完成。音频沿用已确认的音高随速度变化策略，不加入保音高处理。

## 功能修正

音量左右声道的数字输入、秒表、关键帧增删、相邻键导航和 J/K 导航，改用图层的
属性时间。3/2 伸缩、offset=2 时，6.5 合成帧可以准确编辑有符号局部键 -3；
8.75 合成帧对应非整数局部时间，不允许偷偷取整后写入动画键。源时间重映射
不参与属性键的寻址。旧的未映射整数编辑行为保留，静态增益仍为全局设置。

数字手势记录工程身份、文档代次、时钟、偏移和播放头。时间上下文变化时通过
原事务取消入口回滚一次，后续释放不再提交。导航选择保存有符号局部键身份，
不把负数或小数投影塞回旧的无符号合成帧选择。

未创建音量包络时，精确时间编辑与旧命令一样从默认 0 dB 开始。先在工程外构造
候选、校验时间／数值／包络，再发布，非法操作不留下默认包络或撤销项。
不改变媒体重采样、音频设备、渲染语义或工程格式。

## 验证

本阶段 **126 个不同测试通过**：核心 TrackAt 17、音量面板 29、效果参数 55、
属性面板 15、数字输入 9、新增手动原生验收 1。原生验收内部覆盖正放／倒放、
中英文和主窗／浮窗八组。另有五项既有手动测试未运行，不计通过。

最终音量组和原生验收使用语言字体修正后的程序。效果／属性／数字组使用修正
前的程序；生产源码完全相同，只是新夹具补选中英文字体，未重复无关回归。
最终十份源码哈希、两版桌面测试程序及核心程序哈希、逐项结果和原始日志见
[清单](layer-time-audio-authoring-evidence/manifest.json)与
[证据包](layer-time-audio-authoring-evidence/evidence.zip)。

主示例为[中文主窗](layer-time-audio-authoring-evidence/audio-main-zh.png)与
[英文浮窗](layer-time-audio-authoring-evidence/audio-floating-en.png)。24 张最终
截图已保存，八份工程在不同进程重新打开，比较完整工程及空撤销历史。截图使用
真实原生 GPU 窗口与生产 chrome 合成输入，不代表物理鼠标输入、音频设备播放
或变速音高验收。目视确认两种语言的声道标签、目标键和数值可读；原有紧密列头
布局保留，不声称整个时间轴已完成视觉整改。

带 FFmpeg 与宿主 diagnostics 的 `cargo check --workspace --all-targets
--exclude motion-android --locked` 通过。新模块 rustfmt、样式消费者与 diff 检查
通过。仍有既有 unused/dead-code 警告；本机 Epic Overlay Vulkan JSON 缺失仍
出现在原生日志，未屏蔽或修改系统配置。上述通过不等于全局无 Error。

首轮新增夹具使用 in=0、offset=2，违反既有音频可恢复源区间校验；改为 in=2，
保留负局部属性键和源重映射，没有放宽生产模型或修改既有断言。一次后续构建
的 rustc 以 STATUS_HEAP_CORRUPTION 退出，原始日志保留，不能记作通过。
另两次夹具失败分别为重开时缺少 WAV 文件，以及纯色负对照仍保留媒体源重映射；
补齐独立生成的 PCM WAV，并清除负对照不适用的源重映射，未改生产校验。
音量组首轮的一项既有受控 panic 测试在并行编译期间超过原五秒等待；保留失败
证据，编译结束后完整音量组通过相同断言，没有改期限或屏蔽 panic。
首轮原生截图发现夹具未选择中文字体，中文标签空白；只修正新夹具的语言字体
选择后重新构建并验证全部八组。首轮图片保留为失败证据，不作为最终中文验收。

## 可观测性审计

| ID | 分类 | 处置与边界 |
|---|---|---|
| AUDIO-T-01 | 功能修正 | 映射包络使用 TrackAt、精确时间和局部选择；没有修改音频 DSP。 |
| AUDIO-T-02 | 新观察 | 数字开始／继续、轨道导航时间、导航／增删选择、快捷键投影六个固定站点通过已有 observe! 报告原始原因和 object/channel/phase 上下文。 |
| AUDIO-T-03 | 既有覆盖 | 复用 TrackAt 原子事务、worker Info 开始／在途状态、Error 文件和 stderr，保留原始原因。 |
| AUDIO-T-04 | 正常分支 | 未创建包络为默认音量；非整数局部时刻不可增键，取消／失焦／改时钟回滚不作故障。 |
| AUDIO-T-05 | 计数边界 | 沿用面板进程生命周期计数器：首次 Warn、第 60 次 Error、其余抑制；健康行不会复位邻近坏行的累计计数，新进程重置。60 次不等于一秒。固定观察键、饱和计数及 4096 字符上下文上限不变，没有每帧文件 IO、超时、重启或重试。 |
| AUDIO-T-05a | 实际失败验证 | 默认 MOTION_LOG 下，在生产数字、行渲染与导航入口注入错误身份／时间／时钟，验证六站点各首次 Warn 和第 60 次 Error、后续抑制及健康调用不复位；文件共 12 条、stderr 仅 6 条 Error，stdout 不含业务日志。 |
| AUDIO-T-05b | 既有信道验证 | 原生验收向真实串行 worker 提交非整数局部动画编辑，核对 Info start、含 channel/phase 的原始核心错误、Error 文件／stderr 和 stdout 隔离，完整工程保持不变。 |
| AUDIO-T-06 | 待完成 | 其他矢量／蒙版入口、摄影机手势、浮动空间键补偿、连续空间部分编辑、真实媒体预热／音频设备与高速抗混叠验收仍待推进。 |

## DESIGN 处置

§0.7 十二项逐项处置如下。这里的 retained with reason 表示保留对应源码和行为，
本阶段只修改音量编辑入口；这些条目的独立视觉整改／重验仍 pending verification，
音量截图不能充当其他面板的验收证据。

| 基线位置／原行为 | 本阶段前后与处置 |
|---|---|
| path_controls.rs 六处字面行步进 | retained with reason：未编辑路径布局，保持现状。 |
| path_controls.rs 双列分割 | retained with reason：未编辑列布局，保持现状。 |
| path_controls.rs 混合按钮高度 | retained with reason：未编辑按钮尺寸，保持现状。 |
| path_controls.rs 自绘数字背景 | retained with reason：未编辑数字绘制，保持现状。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：未编辑布尔控件，保持现状。 |
| effect_controls.rs 行高与步进 | retained with reason：未编辑效果布局，保持现状。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：未编辑布尔控件，保持现状。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：未编辑枚举控件，保持现状。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：未编辑绘制，保持现状。 |
| effect_controls.rs 树层级 | retained with reason：未编辑树结构，保持现状。 |
| timeline.rs 十一列坐标 | retained with reason：按兼容基线保留；实际音量截图显示原列头布局。 |
| timeline.rs 吸附线颜色 | retained with reason：未改拖动反馈或主题，保持现状。 |

§5.2 没有新行、尺寸、颜色、资产或依赖。§12 截图与输入验证仅用于音量包络
交互，不能代替全产品视觉验收，也不能证明所有屏幕尺寸和平台均已通过。
