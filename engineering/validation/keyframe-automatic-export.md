# 自动时间贝塞尔：主线组合与真实导出验收

本轮基线为 `46809334`：在 `2dc1cac9` 上并入已全绿合并的 #566，保留类型菜单状态及
插件作者入口。#260 仍打开；本文仅证明已实现自动类型的取样、存储及实际编码输出。

## 实测结果

Windows FFmpeg/GPU 桌面组合构建通过，保留 27 项既有告警。组合后重新执行：动画菜单
19、时间轴快捷键 55、插件基础 6、原生 GPU 菜单 1，共 **81 项不同桌面测试通过**。
菜单套件中默认忽略的截图用例已显式运行，时间轴另 1 项 ignored 没有计入通过。
插件基础 6 项不是 #566 的完整原生宿主隔离验收；后者以该 PR 的全绿提交及既有记录为证。

另构建媒体测试程序并显式执行 1 项真实 GPU／FFmpeg 导出用例，12 秒内导出 **7 段
MP4、共 42 帧**，随后用真实解码器逐帧读取：

| ID | 分类 | 结果及范围 |
| --- | --- | --- |
| X1 | 独立数值参照 | 不等长两区间、选中中间键；用独立 Hermite 表达式和物理速率 `4.8` 验证 10 个整数／小数时刻，而非以生产 sampler 自身作为唯一参照 |
| X2 | 存储与固定化 | 完整工程保存重开相等；关闭自动策略后 501 个采样点精确相等；一次撤销恢复作者元数据，之后依次撤销回初始工程且无剩余历史 |
| X3 | 实际正向输出 | 自动类型、保存重开、固定成手动曲线三段 MP4 的全部解码 RGBA 字节相等，不只比较模型元数据、文件大小或首尾帧 |
| X4 | 真实负对照 | 线性、保持、编辑未选中邻键及删除图层四段对照，在区间内部均产生超过 64 个不同字节的帧；删除后所有帧均无红色图层。修改邻键时中间键的值与自动策略保持不变 |
| X5 | 数值到像素 | 自动输出每帧红色区域中心与独立数值参照偏差不超过 1 像素；这是 64×32 的有损编码／栅格容差，不宣称亚像素误差为零 |
| X6 | 冻结及生命周期 | 冻结源在 live 编辑、撤销、删除后保持原工程；全部导出均在 live 和重开 Session 关闭之后执行。每段严格 6 帧，时间戳为 `[0,33333,66667,100000,133333,166667]` 微秒，总长 200000 微秒 |
| O1 | 已有观察覆盖 | 本轮新增的是验收，没有增加生产错误分支、日志、超时、线程恢复或重试。命令默认文件／stderr／stdout 及失败原子性沿用此前自动／手柄阶段的真实 worker 证据，不用本测试成功输出替代故障信道验收 |
| N1 | 正常分支 | 健康导出不增加 Present 热路径日志。raw stderr 保留 FFmpeg 既有编码器 profile／frame skip 提示和 MP4 封装输出；没有把这些提示清掉来获得通过 |

本轮合计 **82 项不同测试通过**。导出用例标为手动 GPU/FFmpeg 验收，已显式执行；
没有把常规套件中的 ignored 当作通过。媒体测试构建保留 2 项既有 fixture dead-code
告警，另有渲染依赖 2 项既有告警。

实际 MP4、42 张原始解码 PNG、作者工程、raw stdout/stderr、组合截图和程序 SHA-256
见[证据清单](keyframe-automatic-export-evidence/manifest.json)。原生截图以生产 Input 驱动，
不是 OS 输入注入。抽查组合后的中文混合状态；本轮不声称新增样式整改或全 DPI 验收。

## 命令与边界

```text
cargo rustc -p motion-desktop --jobs 1 --bin motion-studio --profile test --features ffmpeg,gpu-tests --locked --offline -- -C debuginfo=0 -C link-arg=/DEBUG:NONE
motion_studio-9c5cdc7b4281ddf0.exe panels::animation_menu:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe panels::timeline_shortcuts_tests:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe plugin_editor::tests:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe panels::animation_menu::type_state::native_tests::native_type_menu_captures_exact_mixed_and_manual_states_in_both_languages --exact --ignored --nocapture --test-threads=1
cargo rustc -p motion-desktop-media --jobs 1 --lib --profile test --features ffmpeg,gpu-tests --locked --offline -- -C debuginfo=0 -C link-arg=/DEBUG:NONE
motion_desktop_media-aedcfed3b8fd3b8a.exe movie::automatic_timing_tests::automatic_timing_survives_reopen_freeze_and_live_edits_in_actual_encoded_output --exact --ignored --nocapture --test-threads=1
```

本机指定已有 target、MSVC、FFmpeg、LIBCLANG_PATH 和可写 TEMP/TMP。导出证据根目录由
`MOTION_AUTOMATIC_MOVIE_ARTIFACTS` 指定。未修改已有断言、超时、生产数值算法或资源预算。

本轮没有改面板绘制、样式、行高、列、颜色、命中范围、依赖、第三方资产或效果包。
DESIGN §0.7 十二项沿用[类型菜单阶段](keyframe-type-menu-progress.md)的逐项 retained
处置；菜单截图沿用原 token。§12 的 OS 输入、最小窗口和 DPI 仍待验收。

Roving 的有效时间、作者身份和空间弧长边界见[剩余实现审计](keyframe-roving-boundaries.md)。
本轮真实输出证明不覆盖该尚未实现的类型，也没有覆盖所有参数通道、空间路径或编码配置。
