# 四种工作精度的普通桌面进程验收（#64）

本批没有修改生产精度、渲染语义或已有断言。验证使用
`ea2ac4738dd8da36ce4215ed7ec37c102de90903` 的普通带 FFmpeg 桌面程序，
exe SHA-256 为 `23578431d5edfd8bc40efe792535c7e8344948a24de96e286e19bae016369849`。
它不包含随后开发的状态栏提示，不能用作该提示的视觉验收。

## 实际过程与结果

通过生产 `--mcp-ui` 入口启动八个独立进程，分别以 `unorm8`、`unorm16`、
`float16`、`float32` 初始化工程。每种精度各做一次冷启动和一次读取真实磁盘缓存
的热启动；每次独立正常退出。未设置 `MOTION_LOG`，窗口可见且不抢占焦点。

| 工作精度 | 冷启动首个完整画面 ms | 热启动首个完整画面 ms | 实际缓存 load |
|---|---:|---:|---|
| unorm8 | 7203 | 3765 | missing → restored |
| unorm16 | 4515 | 3281 | missing → restored |
| float16 | 2860 | 3234 | missing → restored |
| float32 | 3203 | 3469 | missing → restored |

这些是包含初始化、窗口和调度成本的单次观测，不能证明每种精度热启动都会更快，
也不是产品性能门限。完整控制器耗时 38.86 秒。

每种精度的冷、热启动完整 RGBA 相同。unorm8 的两个进程还分别通过正常
`motion_edit/set_working_precision` 依次切到 unorm16、float16、float32、unorm8，
共八次运行中修改；每次等待对应 revision 呈现并核对 installed ticket、无
lookaheadErrors，再与独立以相同精度启动的进程比较完整 RGBA。所有比较通过。
抓取这些画面前后的 `effectPipelineBuilds` 相同，抓图没有补做前台效果管线创建。

## 证据与范围

归档 [pipeline-precision-native-evidence.zip](pipeline-precision-native-evidence.zip)
共 129 个成员，3477070 字节，SHA-256：
`418e92ffd04e1c4672ef6db7305796b995019afb1c45abe3708be7f6e0f4cc0e`。
内含原始命令、首尾源码身份、控制器及其依赖模板、八个 PID 的报告、JSON-RPC
请求与回复、默认日志和 PNG；逐成员 SHA-256 与 zip CRC 已核验。

这是 Windows Vulkan 上同一当前 invert 效果的四种公开工作精度验证，不代表所有
效果、blend、目标格式、包更新和驱动变更的笛卡尔积，不证明 DX12 或其他系统。
CPU 包初始化和后台缓存写入仍有自身成本；没有把它们描述为全部异步或零阻塞。
