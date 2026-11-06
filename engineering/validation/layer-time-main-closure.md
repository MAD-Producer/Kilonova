# 图层时间编辑：主线关闭审计

关联 #257。伸缩、倒放、素材重映射及其编辑入口已合入主线；本轮补齐此前缺少的
原生 FFmpeg 终态和独立重开验证，没有修改生产功能或既有测试。

## 逐项范围

| ID | 要求 | 证据与结论 |
| --- | --- | --- |
| LT-CLOCK | 素材、合成、clip offset 和关键帧映射组合 | 主线显式 LayerClock、独立 remap 和叶子量化；历史模型时间 7 个目标共 56 项与媒体 16 项证据保留，其相关源码逐项对照当前主线。 |
| LT-EDIT | 可见伸缩、倒放、重映射与批量原子应用 | 历史桌面草稿/故障信道 7 项；本轮真实 Shell/GPU 菜单与对话框输入覆盖中英文、主窗与 150% 原生浮窗、模态归属和批量应用。 |
| LT-MEDIA | 视频、音频、插值和不可用能力 | 原生视频按源时钟/PTS，音频按源导数重采样；线性、保持和标准缓动有明确语义。音高随速度变化，冻结画面保留、音频静音。Roving 锚点补偿明确拒绝，不冒称已实现。 |
| LT-END | 时间区间、末样本和无映射控制 | 历史原生末量子、嵌套/分块、CFR/VFR 与无映射控制；本轮主线 FFmpeg 实际通过两项视频源时钟/未映射 oracle 测试。 |
| LT-OUTPUT | 保存、撤销、预览和导出使用同一时间映射 | 主线实际通过冻结 PNG/MP4 输出与 AAC/独立 PCM 对照。冻结计划前保存重开，实时重置后撤销/重做和释放 Session，输出仍使用原冻结时钟；32 张映射 PNG、4 个实际 MP4、56 组源时钟像素与 4 张移除映射负对照的原始记录保留。 |
| LT-OWNER | 双窗口、双语、焦点与输入命令语义 | 本轮显式执行原先 ignored 的原生用例：1 passed、0 failed、0 ignored；9 张实际截图。主窗不能取消浮窗所属对话框，浮窗自身输入可应用。 |
| LT-LIFECYCLE | 成功、取消、失效、保存重开及撤销 | 原生用例检查取消不应用、失效来源不入队、撤销完整工程恢复；独立进程 PID 11652/49408 重开后严格比较 Rust Project，undoEntries 为 0。冻结导出取消不发布文件；故障默认日志信道与预取第 60 次/恢复重置另有原始专项。 |

## 实际执行与源码身份

#611 精确头 `0bcb22a5`、#613 精确头 `86ee291d` 的八项必需检查均成功并已合入；
PR 按策略跳过的 libav 和发布任务没有计为通过。

主线 `5935e637170455100731a74139cf827e800d0395` 的
[原生 FFmpeg job 127688566129](https://github.com/MAD-Producer/Kilonova/actions/runs/42152024103/job/127688566129)
真实执行带 `ffmpeg,gpu-tests,ai-depth` 的完整媒体测试，其中：

- `frozen_movie_remapped_aac_equals_flat_authored_pcm_after_live_edits`：通过，覆盖
  reverse/nested/remap/freeze，与独立直接三角函数 FIR 参考生成的 PCM 编码结果精确比较。
- `saved_reopened_frozen_png_and_movie_keep_mapped_video_clocks_after_live_edits`：通过。
- `frozen_png_matches_an_unmapped_native_source_oracle_after_live_clock_changes`、
  `scene_timeline_and_native_reader_share_mapped_cfr_and_vfr_source_frames`：均通过。

此前 AAC 参考夹具修正来自 #606，不归为本轮新实现；原失败仍在旧证据包中。
32 条来源核对记录覆盖历史相关文件及主线作业源文件。上述实际原生测试和独立
FIR 参考与当前主线 `e02b1975` 内容相同；冻结输出以归档中最终修正的源码核对，
不误用其早期失败草案的 hash。

新的 Windows 原生用例复用已验证的 FFmpeg 桌面二进制，20.531 秒退出 0。
二进制属于 `051cd85c` 的错误信道合批源码，不冒称其整个树等于 main；实际涉及的
时间对话框、Shell 和原生用例逐文件与 main 对照一致，执行前后 2440 个源码 hash
和可执行文件 hash 均一致。窗口隐藏创建，仍执行真实 Shell/GPU 输入路径；不是
物理 OS 鼠标注入，不外推为全部平台、DPI 或最终用户设备验收。

保存与状态快照对同一 f32 值的 JSON 数字拼写不同，导致最初只读审计的原始 JSON
相等检查拒绝。原生 Rust 用例原本就把两端反序列化为完整 Project 并严格比较，
该断言没有修改。本轮收集器对五个已声明 f32 的背景/相机字段另核对 IEEE 位完全
相同，其余全部字段（包括 f64 时间映射）递归精确相等，没有容差或字段排除。

第一条原生命令误把 `--` 传给测试程序，实际 0 passed、1 ignored，保留但不接受。
随后正确的直接参数调用实际 1 passed、0 failed、0 ignored；不是用退出 0 替代
执行数量核对。旧证据包 212 个清单成员已逐项重新验证长度和 SHA-256，不覆盖历史
83 项通过或原失败，也不把它们称为这次重新执行的整个工作区。

## 证据与边界

历史证据：[输出验收](layer-time-output-current.md)及其 ZIP，SHA-256
`e93da91ba521f26e5bbfafbef86de319fda0a6b0c95e621a39b3bc9d18b270e5`。
新增 [主线与独立重开原始证据](layer-time-main-closure-evidence.zip) 包含完整作业日志、
PR 检查状态、源码身份核对、成功及未执行的命令输出、两进程结果、9 张截图、完整
保存工程和审计脚本。59 个成员、1767615 字节已逐成员回读，SHA-256：
`f9edbba8cfc5bd716183afc4c6afa8a6da533240ee84528da3b5226aa8fad662`。

没有新增产品观察站点、重试、重启或等待期限。正常取消、范围外/缓存 miss 与失效
草稿不作为故障；保留原显式不可用能力。成功的是上述具体主线 FFmpeg job，不是
其后来被新 push 取代的整轮 main workflow；整体发布由 #622 的 runner 修复另验。
