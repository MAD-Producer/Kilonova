# 关键帧类型菜单：实施及验收进度

本轮接续 #260，基线为 `b737e585`，延续
[时间曲线单端点编辑阶段](keyframe-handle-progress.md)。开始前已核对分支及远端 PR，
其他作者在 #575 推进的预渲染未纳入本分支。本文不是整个 issue 的完成证明。

## 功能与范围

动画菜单增加自动贝塞尔的启用、冻结及精确／混合状态。普通时间插值明确标为
“出侧插值”，自动类型标为“时间约束”，避免把整个区间冒充某一个键的入／出类型。
已有线性、保持和缓动操作继续使用原命令。冻结保留当前两个相邻区间的取样，
不增加键或修改属性值；没有把冻结后的最后一个键显示成不存在的出侧曲线。

只读能力查询复用模型的键、邻键及有效曲线校验。数值和明确的拆分轴可以启用；
手动空间路径、离散参数、几何、颜色传递曲线、速度／弹性曲线及单键轨道禁用。
混合选择必须全部支持，拒绝不会部分改写工程。能力结果按工程身份、代次、修订及
选择缓存，只保留一个结果；缺失版本信息或坏快照不会被缓存成健康的禁用结果。

新增可重映射的 `Ctrl+Alt+F9`：全部已自动时冻结，否则启用。主窗和浮窗分发共用命令；
文本、IME、重复键、失焦、只读、活动手势及过期选择不能发起编辑。既有自定义映射
占用此组合时保留原动作，新默认组合保持未绑定。菜单仅在当前快捷键实际对应的行
显示提示。F9、Shift+F9 和 Ctrl+Shift+F9 的既有行为不变。

回归发现旧属性选择只绑定路径和合成，同路径重开后仍可命中旧键；现在同时校验工程
代次。这是单独的功能修正，不是日志变化。工程切换是正常过期，不新增错误告警。

## 验证与观察

本轮结果和原始输出见[证据清单](keyframe-type-menu-evidence/manifest.json)。
菜单、快捷键、模型、默认错误信道和实际 GPU 截图的最终数量在清单中分别记录。

最终共 **112 项不同测试通过**：模型 8、动画菜单 19、快捷键 12、菜单导航 5、
时间轴 55、属性控件 12、显式执行的原生截图 1。菜单套件默认忽略的截图用例已单独
执行并计入 1 项；时间轴另有 1 项 ignored，未计为通过。Windows FFmpeg/GPU
测试构建通过，保留 27 项既有告警。样式消费者检查及 `git diff --check` 通过。

实际截图为 1280×960、1 倍 DPI。已检查全部八种状态，完整英文菜单没有被截断；
局部图仅裁切原始截图。原生 stderr 保留本机 Epic Vulkan overlay 注册项缺失的
既有加载器错误；窗口挂接、截图及正常关闭完成，没有静默清除这份环境证据。
文件日志还保留 fontdb 跳过本机 `mstmc.ttf` 损坏字体面的 Warn；菜单实际文字可见，
本轮没有修改系统字体或声称完成所有字体／字形覆盖验收。

| ID | 分类 | 范围及边界 |
| --- | --- | --- |
| T1 | 新增模型验证 | 只读查询与真实启用／冻结接受条件相同，覆盖多键、缺失键、线性、自动、保持、单键、速度、弹性及非有限值；查询前后作者存储相等 |
| T2 | 新增菜单／快捷键验证 | 中英文、主窗／浮窗输入分发、选定键范围、一次撤销、冻结逐样本一致、线性／保持／手动／自动／混合状态、离散参数禁用、缓存失效及输入优先级 |
| T3 | 新增默认错误信道验证 | 真实子进程不设置 MOTION_LOG；非法类型及乱序帧快照分别观察 120 次，固定站点仅 Warn 首次、Error 第 60 次；Error 同时到文件与 stderr，stdout 没有日志。健康能力查询不追加故障记录 |
| T4 | 已有行为回归 | 保留动画菜单、快捷键、菜单导航、时间轴及属性控件原断言；没有为了通过验收改写既有预期 |
| T5 | 新增原生截图 | 真实原生窗口、worker 与 GPU，覆盖中英文线性／自动／混合／冻结后手动状态。输入由生产 Input 驱动；不宣称是操作系统鼠标／键盘注入验收 |
| O1 | 已有观察复用 | 新站点为 animation.types.keys／frames／ease／track／validity；固定键、有界上下文及饱和计数沿用面板 observer 的进程生命周期策略。健康兄弟行不清除该站点证据，无新动态观察键 |
| N1 | 正常分支 | 不支持的类型、空选择、键已删除、工程代次改变、缺少可缓存版本信息均按原能力／取消语义处理，不作为故障；健康 Present 不新增日志或文件 IO |
| B1 | 功能／协议边界 | 编辑继续走既有 EditorAction 及 worker 观察信道，沿用既有事务原子性和历史预算；没有新增线程、超时、重试、阻塞或 panic 恢复机制 |
| F1 | 原始失败 | 保留缓存可见性导致的编译失败；保留临时目录拒绝访问及旧属性选择未校验代次的失败。前者使用已有可写 TEMP/TMP 复验，后者修正生产选择身份，未降低断言。真实解析器发现默认键位文档缺少新组合后，按 CONTRIBUTING.md 再生成目录；首次未设置 FFmpeg PATH 的程序启动退出也单独记录 |

未新增依赖、资产、效果包或渲染颜色语义。新增菜单行使用已有菜单绘制、尺寸、
勾选、禁用和快捷键样式，不新增颜色、尺寸或 token；属性树的一属性一行契约不变。

DESIGN §0.7 基线逐项处置如下；本轮不声称整改这些已有问题。

| 基线项 | 处置与理由 |
| --- | --- |
| 路径六处纵向步进 | retained：未修改路径行 |
| 路径双列分割 | retained：未修改路径列 |
| 路径按钮／字段高度 | retained：未修改控件尺寸 |
| 路径手工数字背景 | retained：未修改数字控件 |
| 路径布尔全宽按钮 | retained：未修改路径布尔入口 |
| 效果行高／步进 | retained：未修改效果属性行 |
| 效果布尔文本按钮 | retained：未修改布尔控件 |
| 效果枚举全宽按钮 | retained：未修改枚举控件 |
| 效果手工 hover alpha | retained：未修改 hover 绘制 |
| 效果树层级 | retained：未修改效果树布局 |
| 时间轴十一列坐标 | retained：未修改列坐标和命中 |
| 时间轴吸附线颜色 | retained：未修改吸附反馈 |

本机沿用已有 target、MSVC、FFmpeg 及可写 TEMP/TMP。最终命令为：

```text
cargo test -p motion-model --test automatic_bezier --jobs 1 --locked --offline -- --test-threads=1
cargo rustc -p motion-desktop --jobs 1 --bin motion-studio --profile test --features ffmpeg,gpu-tests --locked --offline -- -C debuginfo=0 -C link-arg=/DEBUG:NONE
motion_studio-9c5cdc7b4281ddf0.exe panels::animation_menu:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe shortcuts:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe panels::menus::navigation::tests:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe panels::timeline_shortcuts_tests:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe panels::animation_controls:: --nocapture --test-threads=1
motion_studio-9c5cdc7b4281ddf0.exe panels::animation_menu::type_state::native_tests::native_type_menu_captures_exact_mixed_and_manual_states_in_both_languages --exact --ignored --nocapture --test-threads=1
```

`MOTION_TYPE_MENU_ERROR_ARTIFACTS` 和 `MOTION_TYPE_MENU_NATIVE_ARTIFACTS` 分别指向
本轮错误及截图目录；两项子进程均显式移除 MOTION_LOG。截图没有计作实际视频导出。

## 仍待完成

#260 在本阶段的 Roving、实际导出／渲染一致性、操作系统输入与完整浮窗原生验收、最小窗口、
DPI 及跨平台 CI 仍未完成。实际 GPU 菜单截图不替代这些验收，不能据此关闭 issue。

后续已补齐已实现自动类型的[真实编码输出验证](keyframe-automatic-export.md)，以及
Roving 的[空间计算基础](keyframe-spatial-timing-progress.md)。它们各自标明源码基线、
证据和剩余范围，不把本阶段未执行的工作追记为已通过。
