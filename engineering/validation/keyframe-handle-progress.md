# 时间曲线单端点编辑：实施及验收进度

本轮接续 #260，工作树基线为 `e4e079a2`，延续
[自动时间贝塞尔阶段](keyframe-automatic-progress.md)。本页不代表整个 issue 完成。
开始前已检查本地分支和远端 PR；#575 的预渲染由其他作者推进，未修改其分支。

## 功能修正

`KeyframeHandle` 命令只携带选定控制点。执行时读取当前区间，保留另一端点的实际控制点，
避免拖动携带的旧区间快照覆盖另一个编辑。`frame` 是区间首键的合成时间；
`incoming=true` 编辑区间末键的入手柄，否则编辑首键的出手柄。

属性和效果的图形手柄都改走该命令。只取消所选端点所属键的自动策略，冻结该键另一个
相邻区间的当前形状；邻键的自动策略继续有效。属性图已有的联动手柄分别发送同一键的
另一端点命令，保留既有 Alt 行为。显式整段曲线编辑命令仍有其原来的范围。

手柄命令可以参与现有拖动事务，并按目标层计入历史预算。第一轮回归发现命令被错误地
标为整合成事务，导致拖动预览被拒绝；已修正，并保留失败输出和取消／提交回归。

新增效果输入矩阵又发现：同一路径的工程代次改变后，旧手柄仍保留。效果目标现在同时
校验工程代次，旧交互取消一次，不在新工程继续写入。该变化也覆盖共用目标身份的
效果数值字段、关键帧拖动及菜单；正常工程帧推进不会改变代次。

## 证据及观察边界

| ID | 分类 | 实测结果及限制 |
| --- | --- | --- |
| H1 | 新增模型／核心验证 | `keyframe_clipboard` 69 项通过，其中本轮 8 项。覆盖端点所属键、邻键自动状态、应用时读取对端、所有现有通道类型、拆分轴、锁定／非法帧／非法控制点／离散类型拒绝、非活动合成寻址及注册表解析 |
| H2 | 新增历史验证 | 1 项通过。12 次连续预览不重算历史序列化费用，提交只计目标层；取消、一次撤销及重做后工程和历史预算准确 |
| H3 | 新增交互验证 | 效果输入用例的 32 个组合通过：中英文 × 主窗／浮窗布局 × 入／出手柄 × 释放／Esc／失焦／工程代次变化。直接命中生产绘制出的手柄区域，再执行真实核心命令；取消后再次绘制不重复取消 |
| H4 | 既有桌面覆盖 | 手柄 4 项、效果时间轴 14 项、时间轴快捷键 55 项通过，共 73 项；其中 H3 已计入效果 14 项，不重复计数。时间轴另有 1 项 ignored，未计为通过。既有手柄、F9、右键缓动、焦点和占用键预期均保留 |
| H5 | 新增默认错误信道验证 | 真实 worker 子进程 1 项通过。错误包含请求 ID、`editor_action`、`curve_handle`、对象、帧及原始原因；默认 Info 文件有 handler 前 start，Error 同时到文件及 stderr，stdout 无日志。失败后工程／revision／历史不变，正确编辑及撤销仍可用，未泄露测试层名称 |
| H6 | 新增持久化验证 | H1 中一次真实保存重开后完整工程相等，并比较 701 个时间采样点；没有把它宣称为实际视频导出或 GPU 像素验收 |
| N1 | 正常分支 | Esc、失焦、工程替换属于正常取消，沿用一次历史回滚，不新增告警、重试或线程重启；健康 Present 不增加日志 |
| O1 | 已有观察复用 | 命令错误走既有事务及 worker 信道；手柄命令生成失败复用有界 `observe!`，携带目标上下文。其计数沿用现有固定站点的进程生命周期策略，健康兄弟路径不清除该站点证据；没有新增动态观察键 |
| B1 | 构建 | Windows FFmpeg/GPU 测试构建最终通过（1 分钟），保留 27 项既有告警。新增字段漏补一个旧测试构造器导致的首轮编译失败已保留；补 false 未改原断言 |
| F1 | 原始失败 | 保留首次核心测试 4 通过／3 失败、效果输入 13 通过／1 失败及构造器／测试编译错误。除上述两项功能缺陷外，新测试还修正了缺少插件依赖声明的夹具、借用和导入错误，以及线性控制点一 ULP 的精确预期；未改生产数值算法或降低既有断言 |

本轮共 **144 项不同测试通过**：核心 69、历史 1、桌面 73、worker 1。
上述主窗／浮窗是生产布局与输入分发测试，**不是操作系统原生输入或真实 GPU 截图**。
没有修改面板样式、行高、列布局、颜色或命中尺寸；未新增依赖、资产或效果包。
DESIGN §0.7 十二项沿用自动时间贝塞尔阶段逐项列明的 retained 处置，本轮不声称整改。

原始输出和程序身份见 [证据清单](keyframe-handle-evidence/manifest.json)。构建及测试命令为：

```text
cargo test -p motion-core --test keyframe_clipboard --jobs 1 --locked --offline -- --test-threads=1
cargo test -p motion-core --lib handle_gestures --jobs 1 --locked --offline -- --test-threads=1
cargo rustc -p motion-desktop --jobs 1 --bin motion-studio --profile test --features ffmpeg,gpu-tests --locked --offline -- -C debuginfo=0 -C link-arg=/DEBUG:NONE
motion_studio-9c5cdc7b4281ddf0.exe panels::graph_handles:: --test-threads=1 --nocapture
motion_studio-9c5cdc7b4281ddf0.exe panels::effect_timeline::tests:: --test-threads=1 --nocapture
motion_studio-9c5cdc7b4281ddf0.exe panels::timeline_shortcuts_tests:: --test-threads=1 --nocapture
motion_studio-9c5cdc7b4281ddf0.exe engine::animation_action_tests::single_handle_fault_is_atomic_and_reaches_default_error_channels --exact --test-threads=1 --nocapture
```

本机显式指定已有 target、可写 TEMP/TMP、MSVC 及 FFmpeg 路径，worker 测试的
`MOTION_HANDLE_KEYFRAME_ERROR_ARTIFACTS` 指向原始输出目录。未通过修改超时、
重试、断言范围或生产日志级别来获得通过。

## #260 剩余工作

自动类型的菜单入口、精确／混合状态和能力禁用、Roving、原生操作系统输入、实际截图、
渲染及冻结导出一致性与完整跨平台 CI 仍未完成。未关闭 #260，未将本阶段当作完整 UI 交付。
本轮没有新增 panic、阻塞或锁恢复机制；新 handler 对这些情形复用既有 worker 观察，
未新增单独的受控阻塞验收，不能用 H5 的错误路径结果替代这些系统级覆盖。
