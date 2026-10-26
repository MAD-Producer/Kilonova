# 稳定属性引用与跨图层拾取验收（#244）

实施前按当前代码核对：`ExpressionTarget` 已有对象、效果实例、参数和分轴的
稳定身份；`thisComp` / `thisLayer` 只提供元信息，`valueAtTime` 只读当前轨道，
并没有可直接复用的跨属性求值。完整核对记录随证据包保留。本轮不增加工程字段、
依赖或外部素材，沿用现有链接图标、主题 token、菜单及宿主事务。

新增 `propertyValue(target, seconds = time)`；ID 使用十进制字符串，避免 u64
经过 JS Number 丢失精度。生成结果仍是可编辑草稿，不建立另一套绑定模型。
完整用法、单位、时间、随机、禁用效果和删除目标规则见
[表达式说明](../../desktop/EXPRESSIONS.md)。

求值使用同一 thread-local QuickJS runtime 的隔离 context；创建子 context
时复用父 context 已持有的运行时锁，不重新借用引擎、不嵌套加锁。编译字节码
使用 Rc 固定存活，直到 runtime 释放后才销毁缓存。依赖共享既有 64 MiB / 512 KiB /
20 ms / 100 ms 限制；不增加线程、放宽预算或通过重试消除错误。

UI 在多行编辑器标题行增加链接按钮，属性右键菜单增加“插入属性引用…”。
拾取期间暂时隐藏弹窗，仅收集该原生窗口实际绘制并裁剪后的属性行，使用现有
accent/border token 显示连接线和目标框。释放只走 TextEditor 的插入、选择和
局部历史；明确应用才发送一次现有 EditorAction。没有新增属性行或缩小命中区。

## 错误处理及边界审计

| ID | 分类及处置 | 验证与限制 |
|---|---|---|
| REF01 | 新功能契约 | 完整 u64、参数转义、重命名/重排、真实保存重开及一次撤销/重做；原生窗口用两个不同图层拾取并检查最终数值。 |
| REF02 | 新功能契约 | 秒与图层偏移、负时间、整向量/分轴、不透明度夹取、效果实例/禁用语义、目标自身随机种子与全局/原型隔离。 |
| REF03 | 原生原因保留 | 缺失目标、无效时间、循环与超时从宿主回调传出；JS catch 不吞原生失败。失败应用不改变原表达式和历史，下一次健康求值可恢复。 |
| REF04 | 实际功能修正 | 初次超时测试实际返回次生 `JavaScript: interrupted`，未满足原预算原因断言；记录中断状态，保留更早的原生失败，避免次生异常覆盖预算原因。原失败日志及原断言均保留。 |
| REF05 | 既有观察复用、增加实际失败验证 | 默认 Info 下真实 EditorAction 缺失对象 999：文件 Error、stderr Error、stdout 隔离，含请求 ID、操作、时间、目标和原始原因。沿用 serial worker 的 start/in-flight、panic 和断开回复边界，没有新 Present 日志。 |
| REF06 | 正常取消/校验 | Esc、失焦、非左键、空白释放、丢失 release、旧工程、其他原生窗口的事件均不写模型；超过草稿字节上限保留源码并显示校验错误，不假报插入成功。正常取消不升级为故障。 |
| REF07 | 有界拒绝 | 描述符 2048 字节、深度 16、每会话查询 1024；深度/查询在下一帧或独立校验重置。循环链最多保留受限深度、每目标 256 字符；采样上下文最多 512 字符，沿用宿主受限错误日志。没有新增高频观察键或每帧文件 IO。 |
| REF08 | 防御观察、未专项注入 | 源码生成的序列化/描述符错误显示在草稿，并沿用 log Error 的受限 cause；当前模型里的正常参数名无法触发这一路。低层分配失败和 panic 沿用已有运行时/宿主边界，没有声称做了 OOM、永久阻塞或原生崩溃注入。 |
| REF09 | 实际输入修正与负面对照 | 面板级测试通过后，把链接按钮的按下/移动/释放改经真实 Shell::present；原实现实际触发 `reference gesture armed background docking`。外层停靠拦截复用表达式覆盖层状态，原断言保留，拒绝背景停靠同时允许拾取完成。旧失败和新成功分别留存。 |
| REF10 | 缓存性能修正与负面对照 | 依赖预编译暴露缓存满 256 项后、热求值入口立即清空缓存的问题；无计时断言的实际求值用例证明缓存从 256 变为 1。清理移至新增源码真正需要槽位时；保持固定 256 上限及字节码存活约束，原失败断言不变。不是通过增大缓存或预算隐藏问题。 |

截图检查还发现拾取提示错误使用不存在的 `panel.padding`，实际回落为零；
已改为现有 `padding.panel` token，并交叉核对新增拾取绘制的所有尺寸 key。
保留早期窄浮窗截图，没有用编译成功代替这项检查。
最后复拍还发现提示矩形下沿留下原状态文字的一部分；复用原生菜单的整 glyph
遮挡，把拾取提示下方的临时覆盖区绘制到窗口底边。只有拾取期间替代状态文字，
不增加新的文本渲染或布局状态，早期重叠截图仍在证据包中。

## 验证及实际截图

最终源码、原始日志、测试计数、程序 SHA 和所有证据的 SHA-256 清单由
`expression-references-results.json` 与 `expression-references-evidence.zip` 记录。
较早的编译/测试/截图阶段单独保留，不冒充最终源码验收。

集成源码 `7dd6d207` 的完整面板回归为 571 项成功、4 项既有手动测试默认忽略；
核心全量为 386 项成功、1 项既有测量默认忽略。随后仅补 Shell 停靠拦截和
拾取提示 token，源码为 `92dfa386`；核心文件未变，保留前述全量结果，另跑
表达式专项 11 项成功、两项显式原生专项分别成功、GPU 表达式像素对照 2 项成功。
最后提示遮挡修正为 `328f2ee9`，只改变拾取提示绘制，再次运行原生引用专项并
构建最终 FFmpeg 程序。
缓存边界修正为 `17fdee8`，保留无计时负面对照，最终重跑核心单元、全部五个
表达式集成测试文件、GPU 输出对照、桌面表达式专项、原生引用专项和 FFmpeg 构建。
独立预览程序以该源码为准，较早的完整面板/核心与旧编辑器专项分别标注对应提交。
各层结果及对应源码分别记录，不把早期计数冒充最后提交的重新全量执行。

最终源码 `17fdee8`：核心单元 28 项成功、1 项既有测量默认忽略；五个表达式
集成文件共 28 项成功；桌面专项 11 项成功、2 项原生专项默认忽略；随后显式
执行引用原生专项 1 项成功，实际 GPU 输出对照 2 项成功。预览程序位于
`E:/Dev/aem/artifacts/expression-references-preview/motion-studio.exe`，SHA-256 为
`fd7c6d70e685c8fbc75d1a04a1f89a023572eeda3142b66ff942d48a6fa35d86`。
目录包含七个 FFmpeg DLL、许可证及构建信息；仅使用该目录与 Windows 系统目录
构成 PATH，命令列表与真实 libav 解码均成功且 stderr 为空，72 帧视频抽样得到
四帧 RGBA。保留 21 条既有程序编译 warning，不宣称全部 warning 已清零。

证据包共 195 个原始文件及逐文件 SHA-256，保留编译失败、超时原因失败、背景
停靠失败和缓存边界失败。原生截图 stderr 中本机缺失 Epic Vulkan overlay JSON
的加载器错误原样保留；上段独立程序测试的 stderr 统计是另一条实际运行记录。

实际截图：[中文菜单](expression-references/zh-reference-menu.png)、
[英文菜单](expression-references/en-reference-menu.png)、
[主窗跨图层拾取](expression-references/zh-reference-drag.png)、
[插入草稿](expression-references/zh-reference-draft.png)、
[浮窗拾取](expression-references/zh-reference-float-drag.png)、
[浮窗应用](expression-references/zh-reference-float-applied.png)。

普通面板回归覆盖主窗、浮窗、语言、取消、事务和默认错误信道。手动显式执行的
原生 GPU 测试经过生产 Shell，生成中英文菜单、拖拽、插入草稿和应用结果；
旧多行编辑器的原生专项另覆盖最小/默认窗口和 1/1.5/2 比例。它们是受控 Input，
没有把未执行的前台 OS 输入法候选窗、物理鼠标或 macOS/Linux 交互称为通过。

## DESIGN §0.7 逐项处置

| 原基线项 | 本轮状态及原因 |
|---|---|
| Path 六处行步进 | retained with reason：未改 Path 布局。 |
| Path 双列分割 | retained with reason：未改 Path 列宽。 |
| Path 混合按钮高度 | retained with reason：保留现有命中区。 |
| Path 自绘数值字段 | retained with reason：未改 Path 字段渲染。 |
| Path 全宽布尔按钮 | retained with reason：未增加或修改布尔控件。 |
| Effect 行高及步进 | retained with reason：只登记现有参数行命中几何，不改行计划。 |
| Effect `fx` / `—` 布尔表现 | retained with reason：保留既有 Checkbox。 |
| Effect 全宽枚举按钮 | retained with reason：保留既有 Dropdown。 |
| Effect hover alpha | retained with reason：未改效果 hover；拾取使用现有语义 token。 |
| Effect 层级缩进 | retained with reason：沿用既有 tree.indent，不增加属性树行。 |
| Timeline 列坐标/gutter | retained with reason：只登记现有属性命中几何，保留列和关键帧命中。 |
| Timeline snap guide 颜色 | retained with reason：未改吸附绘制。 |

本轮不声称修复或重新验收这些独立视觉基线。生成的截图为 Motion Studio
自己渲染的原创测试工程；没有复制第三方界面资产。
