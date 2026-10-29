# 插件编辑器 M3 体验补完：组件、教程与语言辅助

基线 `ebc6dfeb`，分支 `feat/plugin-editor`。仅修改插件作者工作区、分析用声明导出、
主题 token 和文档；没有修改节点编辑器、宿主面板或最终 shader 编译/像素语义。
来源与许可见根目录 ATTRIBUTION.md、PROVENANCE.md、NOTICE。

## 变更与边界

- 组件库：七类部件使用同一 `Preview` 和 `Metrics`；缩略图按真实测量边界绘制结构，
  去掉缩小后不可读的微型文字，悬停示例保留实际控件绘制。
  提供名称、用途、悬停大示例及清晰的空容器提示。示例参数不随拖拽写入用户布局。
  实际控件命中尺寸保持原样，拖动仍走原来的事务与插入目标。
- 层次：结构/组件/参数库、画布、布局检查器分区；窄逻辑面板在参数和 WGSL 之间切换。
  所有新尺寸先写入 dark.json，复用现有文字、边框、悬停与选择色。
- 教程：软件内“使用教程”页 / F1，快速上手、布局交互、代码辅助和排错三个章节；
  按步骤进入真实编辑页。外部完整版本见 [使用教程](../host-api/plugin-editor-tutorial.md)。
- WGSL：可选本机 wgsl-analyzer，异步 stdio LSP；真实宿主 ABI 和公共参数辅助函数组成
  未落盘的虚拟文档。补全、作者/只读宿主定义定位、版本化 pull/push 诊断、UTF-16 映射。
  补全文本复用原生文本编辑事务；拒绝越出作者源码、过期光标、片段和附加编辑。
  新草稿不能被旧回复覆盖；最终校验仍走现有 Naga/预算/GPU。
- 上游限制：固定 `2026-09-30 / 659b96821595a83cbc06abf844461eb736caaab5` 的
  [hover 实现](https://github.com/wgsl-analyzer/wgsl-analyzer/blob/659b96821595a83cbc06abf844461eb736caaab5/crates/ide/src/hover.rs)
  明确返回空。编辑器显示无说明并建议查看定义，不宣称已经提供类型悬停。
  没有引入自动安装、隐式下载、语言服务重启或新的网络服务。

## 验收记录

本轮产物目录：`artifacts/plugin-editor-m3-refinement`。最终运行与源码哈希、日志和截图
整理在本页同名 evidence 目录。Windows 100% 比例、真实 Vulkan GPU 与官方语言服务。

| ID | 检查 | 状态与证据 |
| --- | --- | --- |
| G1 | 真实组件缩略图、悬停大示例、分组拖入与跨组移动 | 通过；native-final 两张组件截图，原生拖入/移动/分栏断言未降低 |
| G2 | F1 教程不改工程、可返回真实编辑页 | 通过；native-final/guide-quickstart.png；断言打开前后工程完全相同 |
| L1 | 官方未修改服务器的参数补全、生成声明定义、作者诊断行 | 通过；plugin-tests-final.log 的真实服务器检查；空 hover 按上游实际能力验收 |
| L2 | Ctrl+Space 与 Enter 通过原生文本控件应用补全 | 通过；native-final/wgsl-completion.png，应用后源码逐字相同，证明只替换目标标识符 |
| L3 | 中文/补充平面字符 UTF-16 映射；越界、陈旧、附加编辑拒绝 | 通过；protocol_bounds 与 completion_maps 两项 |
| E1 | 缺失程序、过大协议消息、真实服务错误、不可返回请求 | 通过；language-failures-verified 的原始文件/控制台输出 |
| E2 | Info start 在阻塞前持久化、flight 可见、Error 文件+stderr、stdout 隔离 | 通过；受控阻塞服务真实进入 handler 后检查已落盘 start；日志不进入 stdout |
| E3 | 普通关闭与 Windows 突然退出时的子进程清理 | 通过；同一受控测试绕过全部 Rust 析构后退出，再等待真实服务进程终止；同目录重复通过 |
| R1 | M2/M3 插件回归含真实 GPU、保存、撤销、关闭 | 16/16 通过，28.60 秒；见 plugin-tests-final.log；错误前后 shader 指纹相同 |
| B1 | FFmpeg 产品构建、主题消费与作者模块格式检查 | 通过；产品29个既有告警、测试26个既有告警；未将绿色构建视为视觉验收 |
| W1 | 实际独立窗口启动、宽窄切换、预览与关闭 | 通过；runtime-native/report.json：正常退出0、缺失工程退出1，两者 stdout 空；缺失源错误文件与 stderr 一致 |

实际窗口通过已知子进程 HWND 发送 Win32 客户区消息，截图来自屏幕捕获，未裁造布局。
首次激活窗口的点击可能只完成激活，最终脚本先完成激活再点击组件页。
原生 harness 另行验证 F1；真实桌面 F1 被本机截图工具全局占用，所以窗口截图通过顶部教程页进入，
不将那张外部取色浮层截图作为产品证据。宽窄窗口教程与代码截图均已目视检查。
正常窗口 stderr 保留本机残留 Epic Vulkan overlay JSON 的 loader Error，未过滤或宣称已修复。
作者模块递归格式检查与 compile.rs 检查通过。扩大检查到 shader.rs 时发现既有格式差异，
涉及 import、循环边界、输出包装和 store_policy_tests；本轮只在其新增一个已格式化的
`authoring_prelude`，未顺带改写这些无关代码。保留 format-check.log，不声称全仓格式通过。

## 失败与修正保留

1. 首次真实 LSP 测试使用了不含盘符的 Windows `file:` URI，服务器拒绝为“url is not a file”。
   改为平台有效的虚拟 URI，并让服务器使用临时工作目录，避免误发现仓库的 Cargo 工作区。
   同时按实际服务器能力接入 `textDocument/diagnostic`，不依赖已被该版本替代的 push 诊断。
2. 新写的首次 hover 验收误以为服务器会返回类型；核对固定上游源码后确认其实现为空。
   修正的是本轮新增测试对上游能力的错误假设，保留空值边界和原始失败日志；补全、定义、
   诊断仍必须由真实服务器给出。没有改动任何原有产品断言来制造通过。
3. 故障夹具首次独立运行缺少 MSVC 链接器环境，`rustc` 无法链接。加载仓库现有
   `artifacts/multiple-camera-layers/env.ps1` 后，同一检查通过；夹具源码未降低断言。
4. 首次界面批次发现新增尺寸 token 超过现有 512 上限，7 项检查因此失败。
   修正 token 值和基于 token 的组合尺寸；没有放宽主题验证器或原有 UI 断言。
5. 故障测试复用目录时，旧 block.blocked 被误认为本次请求已进入服务端，提前检查 start。
   显式删除当前夹具标记（仅 NotFound 正常）；原始时序断言保留。同目录重复运行通过。
   失败的隔离测试进程退出还暴露异步清理来不及执行、服务进程持有管道的问题；
   核对路径后手动终止该次自建 block.exe（PID 73732），没有操作其他程序。
   Windows 会话现使用不继承的 KILL_ON_JOB_CLOSE Job Object；普通关闭仍主动 kill/wait。
   受控验收特意 forget Client 并 process::exit，确认真实服务也被回收；不增加重试或重启。
   API 依据为 [Microsoft Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)。
6. 组件库从文字列表改为缩略卡片后，旧 Split 拖拽坐标不再命中分栏；
   改为从真实组件矩形取中点，保留插入、比例和取消恢复断言。

## 错误处理审计

| ID | 类型 | 处理与证据边界 |
| --- | --- | --- |
| O1 | 新观察 | 初始化和每个语言请求先发布 flight，再 Info start + flush，之后进入进程/协议 IO；普通完成 Debug |
| O2 | 新观察 | 启动、读取、协议、请求拒绝与异常退出保留阶段、请求、revision、耗时及原始原因；不打印文档或完整协议载荷 |
| O3 | 新观察 | 请求队列 1 格、消息队列 8 格、消息 1 MiB、头 8 KiB、诊断 32 条、每条原因/展示文字有上限 |
| O4 | 作用域说明 | stderr 字节计数为每个服务进程的生命周期计数，饱和累加、进程退出即销毁；不把字节数等同于时间或持续退化次数 |
| O5 | 正常分支 | 未配置、空 hover、正常空结果、旧 revision、忙时背压及未协商的服务端方法不算故障；协议请求仍返回明确响应 |
| O6 | 功能边界 | 分析进程与编译 worker 分离；失败不清空草稿、不改参数或旧 GPU。关闭会话由独立进程所有者终止并回收对应服务；Windows Job Object 额外覆盖宿主突然退出，此项为独立资源生命周期修复 |
| O7 | 既有覆盖 | 保存/编译/旧 GPU 保留沿用 M3，默认日志及全局 panic hook 沿用仓库基础设施 |
| O8 | 待验证 | 新语言线程内部 panic、启动线程失败、管道写阻塞期间关闭、POSIX 突然退出清理尚无完整受控矩阵；不将现有成功计数等同全部覆盖 |

## DESIGN §0.7 逐项处置

| 基线项 | 本轮处置 |
| --- | --- |
| path_controls 六处行步进 | retained：本轮未编辑该面板 |
| path_controls 两列分割 | retained：本轮未编辑该面板 |
| path_controls 混合按钮高度 | retained：本轮未编辑该面板 |
| path_controls 自绘数值背景 | retained：本轮未编辑该面板 |
| path_controls 布尔按钮 | retained：本轮未编辑该面板 |
| effect_controls 行高/步进 | retained：本轮未编辑该面板 |
| effect_controls fx 开关 | retained：本轮未编辑该面板 |
| effect_controls 枚举按钮 | retained：本轮未编辑该面板 |
| effect_controls hover alpha | retained：本轮未编辑该面板 |
| effect_controls 树层次 | retained：本轮未编辑该面板 |
| timeline 表头与列坐标 | retained：本轮未编辑时间轴，保持交互基线 |
| timeline 吸附线颜色 | retained：本轮未编辑时间轴 |

软件内真实实例连接、主/浮动宿主窗口仍属于 M4；没有包装成已经完成。
真实 OS DPI/IME、多平台、长名称完整矩阵仍待验收。语言服务器自动分发、完整 IDE、
重命名/格式化/代码操作/片段占位符等不在本轮交付范围。
