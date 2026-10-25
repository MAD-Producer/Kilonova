# 父级拾取与绑定策略进度

关联 #246。实现前核对远端开放 PR、近期本地分支和根工作树修改；效果库与精度工作保持独立。`feat/parent-picking` 从 #484 的文档提交 58a39ed3 建立，未移动其分支；#484 的完整打包验证继续运行。本文件记录阶段证据，不声明 #246 已完成。

## 已实现的模型能力

新增显式命令 `parent_with_mode`，模式必填且注册 schema 为 `preserve_world` / `preserve_local`。既有 `parent` 命令与保持世界变换语义不变，工程格式和已有 ParentLink 字段不变。

- 保持世界变换：在命令的合成帧计算绑定矩阵，保持该帧的世界矩阵；动画局部轨道不重写、不烘焙。后续帧仍按父级的动画传播，不宣称所有帧世界矩阵均不变。
- 保持局部值：沿用完整局部轨道，将新父级绑定设为单位矩阵；取消父级时清除旧绑定，因此画面可以按新父级改变。这种策略不需要反转父级矩阵，可以绑定零缩放父级。
- 非法层级、自己／后代、缺失父级或摄像机、锁定目标和非法帧保留原始原因；新命令错误携带对象、目标父级、帧及策略。链接验证失败时恢复原链接；批量操作仍通过既有原子事务，一次撤销恢复整批。

## 验证与错误信道

证据根目录为 `E:/Dev/aem/artifacts/pr-ci-followups/`。

`parent-modes-core-third.log`：新增 4 项与原有 parenting 4 项全部通过。验证两种策略的真实 Scene 世界矩阵、动画轨道不变、分数帧采样、取消父级、保存重开、撤销／重做、schema／旧命令封装、自指／循环／缺失／锁定／零缩放和批量中途失败的完整工程不变性。

`parent-policy-desktop-first.log`：带 FFmpeg 的实际桌面测试程序编译通过（既有 22 条 warning 保留），新增默认日志子进程 1 项通过，9.58 秒。`parent-policy-errors-first/` 保留真实零缩放、缺失父级 999、非法帧 999 的失败：三条 Error 同时进入文件和 stderr，包含对象、父级、帧与原始原因；stdout 不泄露，Info start 仅入文件。每次失败的工程与 revision 不变；随后局部绑定成功、一次撤销恢复完整工程，工作器正常关闭。未增加重试、超时、重启或 Present 日志。

前两次核心测试原件保留：新增夹具最初使用 Rust 不接受的 `.005` 和错误的名称参数类型，之后错误地把既有注册器的 InComposition 包装断言为裸 Parent。均按真实 API 修正新增夹具；最终仍严格核对旧命令的合成、对象、父级与帧，没有修改旧实现或放宽既有断言。

## 审计处置与剩余工作

| 项目 | 处置 |
| --- | --- |
| PP-01 功能 | 两种显式绑定策略、原有动画与历史边界已有实现和核心证据。 |
| PP-02 新观察 | 新错误携带对象／父级／帧／策略及原始 cause；不另建日志基础设施。 |
| PP-03 既有观察 | 串行 edit 的 Info start、在途状态、完成清理和默认错误信道由现有工作器提供；默认级别下真实失败与恢复已验证。 |
| PP-04 正常分支 | 取消拾取、无合法目标与过期 UI 快照按正常取消处理，不记作故障；松开丢失时也结束手势，防止后续光标运动提交。 |
| PP-05 待完成 | 当前实现已有下述组件与系统输入证据；远端 CI、官方包和 Windows 150% 系统 DPI 尚待验证。#246 保持开放。 |
| PP-06 来源／视觉 | 无新增依赖、第三方资产、产品渲染语义或基线；复用现有 Link、Checkbox、TextEditor、popup、row、field.number。新增 token 仅保存父级字段已有 26px 命中和 32px 行高，先写 dark.json 再使用。 |

## 搜索、策略与拾取界面

时间轴已有父级单元格位置不变，扩展为带查询框与 Checkbox 的覆盖层；属性面板使用同一 Chooser 与策略。按名称／ID 搜索，过滤多选源、其后代、音频及循环目标；查询最多 128 字符，列表显示至多八行并可滚动。候选层级按冻结的 root／合成／generation／revision／选区缓存；状态改变即取消，不沿用旧目标。属性行数和原命中尺寸不变，关闭时字段透明。

原 Link 图标改为真正拖线：冻结输入窗口和选区，只高亮合法可见目标，释放才发送 ParentWithMode，多选发送一次 EditBatch；取消和跨窗口输入消费关闭帧。没有增加正常 Present 日志或文件 IO。

`parent-chooser-columns-first.log` 保留三项旧夹具失败：新搜索／策略占据菜单头部，旧点击坐标及旧 Parent 命令断言不再符合新入口。夹具改用实际可见行矩形，并严格断言 InComposition、对象、目标、帧和 PreserveWorld；原有世界矩阵／完整工程单步撤销断言未变。Link 的既有“打开菜单”断言按需求升级为“启动拾取且不打开菜单”，混合菜单其余断言保留。

`parent-pick-controls-first.log` 六项通过；随后补属性入口，最终 `parent-picking-regression-final.log` 的 parent_ 定向集 18 项通过，含默认真实错误子进程、属性主窗／浮窗、短窗口、多选和取消。`parent-search-rename-tests.log` 时间轴列 18 项通过，包含主窗／浮窗 Enter 后不启动重命名且立即释放 Ctrl+Z。两集有重叠，不相加计数。带 FFmpeg 的实际程序最终编译见 `parent-picking-bin-final.log`，既有 warning 原样保留。

Windows 外部键鼠原件为 `parent-os-input-driver.py`、对应 steps JSON 和各目录回复；程序只用 MCP 读取状态，绑定、搜索、模式 Checkbox、撤销和保存由 Win32 实际输入进入 winit。英文主窗 `parent-os-main-final/` 验证默认世界策略、立即撤销、多选一次事务／撤销、非法释放不改工程；中文原生时间轴浮窗 `parent-os-float-final/` 验证取消勾选后的单位绑定、多选局部策略、立即撤销和 Esc 后释放不产生编辑。`parent-os-float-persist/` 保存局部绑定后正常退出，独立 --mcp 进程重开完整 project 相等。`verify-parent-os-input.py` 已执行通过，汇总为 `parent-os-input-summary.json`，stdout 全部为协议 JSON。独立运行目录及哈希为 `parent-os-runtime/` 和各 runtime-manifest.json；PATH 仅含 Windows 系统目录和随程序打包 DLL。

首次实际探索 `parent-os-main-first/` 揭示 Enter 同时启动图层重命名，使后续快捷键被名称编辑器消费。只修功能入口：父级覆盖层持有键盘时禁止进入时间轴重命名；没有重试用户命令、改快捷键或放宽断言。初始现象、日志及截图保留，最终两窗系统输入均验证立即撤销。

实际 OS 窗口 DPI 为 96（100%）；主窗 1280×850、浮窗 1100×450。88px 短窗口为组件命中／裁剪证据，不能冒称系统最小窗验收。Windows 150% 系统 DPI、系统 IME 候选提交、Info 面板纹理／呈现基线对比和官方发行包仍待验证；独立 Debug 加载不等于 Release 包验收。

## DESIGN §0.7 基线逐项处置

| 基线项 | 处置／原因 |
| --- | --- |
| path 六个纵向步进 | retained：未改路径属性行。 |
| path 两列拆分 | retained：未改路径字段分栏。 |
| path 混合按钮高度 | retained：未缩小路径命中范围。 |
| path 自绘数字字段 | retained：未改数字编辑。 |
| path 全宽布尔按钮 | retained：本次新选项使用 Checkbox，旧路径开关保持原样。 |
| effect 行步进 | retained：未改效果行。 |
| effect 布尔文本按钮 | retained：未改效果控件。 |
| effect 全宽枚举按钮 | retained：未改效果枚举。 |
| effect 手工 hover alpha | retained：未改效果悬停。 |
| effect 缺层级缩进 | retained：未改效果树。 |
| timeline 既有列坐标 | retained：原列与 Link 命中范围不变，只扩展父级动作；既有布局与列冲突测试继续通过。 |
| timeline 黄色吸附线 | retained：未改吸附反馈；新拾取线只使用既有 focus 色与 border 尺寸。 |
