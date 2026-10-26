# 表达式多行草稿与后台诊断验收（#243）

本轮交付属性所属的多行编辑器：保留既有单行快捷输入和启用开关，通过展开图标或属性右键菜单打开；不新增独立停靠面板，也不增加属性树行数。编辑器提供字素级光标/选择、多行输入、局部撤销/重做、应用内文本剪贴板、行号、纵向滚动、常用片段和随机种子。Enter 换行，Ctrl+Enter 应用，检查只调用既有 SWC 解析/适配器。语法错误显示原始草稿的行、列，点击提示定位。运行错误保留目标和帧的原因，不伪造运行期源码位置。

最终源码为 `29b7001a948f47e6e3ad0c267d2e5598b9f66d35`，包含 main 的素材事务与曲线输入优化及最后的种子/片段焦点修正。核心解析器新增可复用的结构化诊断接口，执行路径保留原错误信息及既有 AST、内存和时限，不增加依赖或自动重试。编辑请求使用既有工程版本校验；每份 UI 回复另有独立凭据，防止迟到结果覆盖新编辑器或新草稿。检查和应用各占一个有界后台任务，轮询不等待 worker。提交应用后关闭编辑器仍处理实际事务结果，按钮与提示明确说明此边界。

## 错误处理与协议审计

| ID | 处置 | 证据与限制 |
|---|---|---|
| EX01 | 既有观察复用 | `ExpressionCheck` 接入封闭命令枚举、panic 回复和命令名称，沿用串行 worker 的 Info start 与 in-flight 发布，完成为 Debug。没有新增 Present 开始/完成日志。 |
| EX02 | 既有观察与新增实际失败验证 | 应用使用 `EditorAction` 与 `SetExpression`。实际 JS 抛错、实际无限循环触发预算错误，均验证文件 Error 与 stderr；stdout 无 Info/Error，草稿与模型保持真实结果。没有放宽预算、超时或测试断言。 |
| EX03 | 正常分支 | 语法错误作为显式检查结果返回，不执行草稿、不修改模型、不作为引擎故障；工程版本失效返回既有正常取消状态。旧检查回复遇到新输入、新编辑器或已关闭编辑器时正常失效。 |
| EX04 | 新增协议观察；防御分支待专项注入 | 检查回复缺字段、无效 UTF-8 边界或应用成功回复缺少目标表达式，记录 Error（操作、阶段、对象、受限原因），同时显示错误，不假报成功。诊断消息限 1024 字符，字节位置只接受原始草稿边界；不记录完整源码。这些防御分支未作为实际宿主故障修复宣称验收，实际 JS/预算故障证据见 EX02。 |
| EX05 | 正常校验 | 8192 字节上限和种子范围错误显示在草稿内。反复超过编辑域上限属于显式输入校验，记录 Debug，不把按住键盘形成的重复拒绝当作后台故障。剪贴板实际不可用/污染等失败沿用既有 Error 信道。 |
| EX06 | 既有观察复用 | 后台回复断开通过既有 `Engine::poll_reply` 记录 Error、结束任务并保留原始错误。没有新增吞 panic、重启线程、业务锁内日志或每帧文件 IO。命令 panic/in-flight 通用契约由已有基础设施承担，本轮不声称单独注入解析器永久阻塞。 |
| EX07 | 新增受控阻塞验证 | 以真实 `TestWait` gate 阻塞串行 worker，先验证该操作已发布 in-flight，再排入语法检查、轮询并继续修改 UI 草稿；释放 gate 后旧检查不覆盖新草稿。应用实际失败恢复后仍可成功应用，关闭编辑器后成功结果仍进入状态并可单步撤销。 |

## 验证记录

结果数量、源码哈希与独立程序信息见 [结构化结果](expression-editor-results.json)；[证据包](expression-editor-evidence.zip) 包含原始成功及失败日志、子进程 stdout/stderr、各阶段截图及 SHA-256 清单。最终检查结果由结果文件记录，较早阶段不冒充集成后的验收。

集成阶段 `ca063746` 的面板全量为 568 项通过、3 项默认忽略；核心 24 项通过、1 项原有默认忽略，UI 控件 56 项通过、12 项原有默认忽略。最后焦点修正只改动表达式弹窗与专项测试，因此核心/UI 组件文件不变，不重复全量；最终源码另跑表达式专项、显式原生 GPU 专项与 FFmpeg 程序构建，具体计数见结果文件。主题消费检查和 `git diff --check` 通过，既有编译 warning 未被宣称清零。

最终源码 `29b7001a` 的表达式专项 8 项通过、1 项原生专项默认忽略；随后显式运行该原生专项，1 项通过并产生 16 张中英文真实 GPU 截图。程序构建成功，保留 21 条既有 warning。独立预览位于 `E:/Dev/aem/artifacts/expression-editor-preview/motion-studio.exe`，SHA-256 为 `b89a78638a1fdf97d47f4166b97d9d9f7a435ecec0dfefd8d53b0b12dec40dce`。该目录携带七个固定 FFmpeg DLL、许可证和构建信息；在 PATH 仅含该目录与 Windows 系统目录时，命令列表与真实 libav 视频解码均成功、stderr 均为空，72 帧源抽样解码四帧 RGBA。证据包保留 155 个原始文件及逐文件哈希，不覆盖早期失败证据。

早期编译失败依次暴露不存在的 PaintList clip API、TextAtlas 可变测量借用、超过样式系统上限的宽度 token、错误 Cargo 包名、标签调用签名和测试工具方法名，均保留原记录。交互回归发现种子框 Enter 同帧再次传入源码区的实际问题；保持原换行断言并修正输入归属。错误子进程最初误用已关闭 engine 的文档凭据，改为读取实际 Shell 状态，保持错误原因断言。

原生第一轮误把禁用菜单项计入 Down 次数；按真实菜单导航跳过禁用项后继续。第二轮抓到主窗口全局 Escape 先关闭整个编辑器的实际缺陷；补齐普通回归并让片段菜单优先消费 Escape。随后验证应用 Edit 菜单位于编辑器之上，复用既有 glyph 遮挡处理，实际 Copy 命令保留正确草稿和会话。原生 stderr 中本机 Epic Vulkan overlay JSON 缺失的加载器日志原样保留，未过滤或压制。

自动化覆盖中英文主窗口、真实原生浮窗、最小 900×600、默认 1600×1000、600×420 浮窗以及 1/1.5/2 倍比例；检查真实 GPU 输出尺寸、菜单、语法诊断、焦点输入、浮窗应用/撤销。单步历史和保存重开使用真实宿主，不以模拟成功回复代替。普通字段仍保持单行 Enter 语义和既有文本命令会话。

截图：[主窗草稿](expression-editor/zh-draft.png)、[属性右键菜单](expression-editor/zh-property-menu.png)、[片段菜单](expression-editor/zh-snippets.png)、[文本 Edit 菜单](expression-editor/zh-text-menu.png)、[错误定位](expression-editor/zh-diagnostic.png)、[已应用的原生浮窗](expression-editor/zh-float.png)、[英文默认窗口](expression-editor/en-default.png)、[2 倍比例](expression-editor/zh-ratio2.png)。

## DESIGN §0.7 逐项处置

原基线行号已随主分支演进移动；以下保留当前实现，不把其他工作已有的整改计作本轮修复。Path/Effect 的独立视觉整改本轮不重新验收。

| 基线项 | 本轮处置 |
|---|---|
| Path 六处字面行步进 | retained with reason：当前行计划已有公共 row/子控件；未修改 Path 布局，独立视觉结论保留。 |
| Path 双列分割 | retained with reason：不修改 Path 字段列宽和行为；本轮只为表达式弹窗使用独立 token。 |
| Path 混合按钮高度 | retained with reason：保留既有命中范围，不为多行编辑器改 Path 控件密度。 |
| Path 自绘数值字段 | retained with reason：当前 Path 渲染不在改动范围，不宣称本轮完成该基线整改。 |
| Path 全宽布尔按钮 | retained with reason：保留既有实现；新编辑器种子为数值字段，没有新增布尔按钮。 |
| Effect 行高及步进 | retained with reason：当前效果行已读 row/group token；未修改效果行计划。 |
| Effect `fx`/`—` 布尔表现 | retained with reason：当前 Checkbox 实现保留，表达式启用开关保持既有 Checkbox。 |
| Effect 全宽枚举按钮 | retained with reason：当前 Dropdown 路径保留；新片段菜单复用 Dropdown，不循环枚举值。 |
| Effect 手动 hover alpha | retained with reason：不修改效果 hover 路径；新窗口使用现有语义色与字段状态。 |
| Effect 缺少层级缩进 | retained with reason：当前 tree.indent 实现保留，不将其计为本轮修复。 |
| Timeline 列坐标与标签 gutter | retained with reason：保持现有列、属性与关键帧命中区；展开图标只占源码已有图标槽，窄处隐藏该入口并保留右键入口。 |
| Timeline snap guide 颜色 | retained with reason：保留兼容基线；未更改吸附颜色或反馈。 |

## 仍需人工验收的边界

本轮原生测试使用生产入口与受控 Input，没有注入前台 OS 键盘/输入法候选窗口；组合输入与确认按键隔离由文本/面板测试覆盖，实际前台 IME 与 macOS/Linux 交互仍需人工验收。剪贴板复用既有应用拥有的文本缓冲区，不声称新增系统剪贴板桥接。运行期诊断没有可靠原始源码映射，只显示真实目标/帧/原因；语法诊断才提供原始行列。对仍打开窗口的失效提交保留草稿；显式工程关闭/切换沿用原有取消交互语义，不承诺跨工程恢复草稿。#243 保持开放，PR 使用 Refs，避免把这些边界冒充完成。

没有引入第三方资产、复制外部图标或修改工程/效果包格式。新增尺寸先写 `dark.json`；`DESIGN.md` 只记录规则。没有以编译、绿色计数或截图存在替代未覆盖的人机验收。
