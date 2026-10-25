# 编译故障恢复验收的帧完成顺序

代码提交：`b395bf6c25c2c62eb7564f633f8ef6550ff7d5e4`。

## 原始失败与处理边界

主分支 `302a15bf` 的 [run 40175526135](https://github.com/MAD-Producer/Kilonova/actions/runs/40175526135) 有两个独立失败：

- macOS ARM，job `121120268297`：`logging::pointwise_compile_tests::compiler_failures_reach_default_file_and_stderr_after_unlock_without_retries` 在恢复阶段连续提交帧，触发 `too many pending render allocation encoder scopes`。原始日志定位在测试的恢复闭包末端 `unwrap()`，不是故障注入断言。
- macOS Intel，job `121120268293`：发布包和验收产物上传均报 `Failed to CreateArtifact: Unable to make request: ENOTFOUND`。该 job 的编译、单元测试、FFmpeg 打包和提取后的解码均已成功。此项属于外部网络失败，本次不改变上传策略；下一次完整 main 发布仍需绿色验证。

`Renderer::draw` 提交 GPU 工作后返回，编码器资源在完成回调后回收。测试原来连续提交 66 个恢复帧，最后才调用一次 `read_target`；Metal 无头运行未及时推进完成回调时，会耗尽生产中现有的 64 个编码器准入名额。

测试现在在首次恢复帧及后续 65 帧分别执行真实回读与非空像素断言，再提交下一帧。`read_target` 使用既有的 GPU 完成等待与映射流程。此验收测量编译缓存恢复及错误信道，不要求连续堆积未完成的帧。

## 未削弱的契约

- 便携解析和原生校验两个真实故障模式、各 70 次重复失败及缓存构建次数断言全部保留。
- 编译器在宿主锁内不记录故障、锁释放后只记录一次 Error、默认 Info 开始记录、文件及 stderr、stdout 隔离断言全部保留。
- 首帧后的管线构建和便携编译次数必须保持不变；原有最终回读和像素断言仍保留，另增加逐帧真实像素断言。
- 生产编码器上限、预算、错误分支、超时和重试策略均未修改。
- 无面板绘制、布局、字体、图标、第三方依赖或资产改动。`DESIGN.md` §0.7 的既有面板整改项保留，原因是本次只修正 GPU 测试夹具的提交/完成顺序；不宣称完成任何样式整改。

## 本地验证

Windows x86_64，真实 Vulkan 设备，源码来自上述代码提交：

1. `cargo test -p motion-desktop --features gpu-tests --bin motion-studio logging::pointwise_compile_tests::compiler_failures_reach_default_file_and_stderr_after_unlock_without_retries -- --exact --nocapture --test-threads=1`：通过。两个子进程分别完成 66 个恢复帧，仍执行原有故障、去重、锁外和默认输出信道断言。总运行 41.92 秒。
2. `cargo test -p motion-render --lib --features gpu-tests renderer::admission_tests::frame_blend -- --test-threads=1`：4 项通过。现有编码器拒绝、保留资源、完成输出身份和真实像素恢复的断言均未修改，证明此次夹具同步没有放宽生产准入。
3. 独立检查原始日志：每个故障模式的编译 Error 在文件/stderr 各一次、stdout 零次；便携编译 Info 开始记录各两次。保留本机 Epic Vulkan overlay 缺失 JSON 的驱动原始日志，不滤掉后冒充无错误运行。
4. `git diff --check` 通过。

结构化结果见 [compiler-acceptance-drain-results.json](compiler-acceptance-drain-results.json)。原始 main 失败、构建输出和两个子进程的默认信道记录见 [证据包](data/compiler-acceptance-drain-b395bf6c.zip)，54,841 字节，SHA-256 `b72ab988ece62983b745a40ae7394f580b18899628076ee8392e4c4614d5690d`，ZIP CRC 校验通过。

## 观测审计与剩余工作

| 项目 | 处理 | 证据与界限 |
|---|---|---|
| 编译故障 Error 与原因 | 既有覆盖，保留 | 两种真实故障，文件和 stderr 各一次；保留对象、效果、session 和原始原因 |
| 锁外记录与默认 Info 开始 | 既有覆盖，保留 | 原断言及原始子进程日志 |
| 正常恢复 | 夹具功能修正 | 每帧真实回读；不增加恢复生产路径或重试 |
| 编码器数量拒绝 | 既有功能边界，保留 | `frame_blend` 四项真实 GPU 验收；未扩大上限 |
| macOS ARM 完整发布 | 待验证 | 本机不是 Metal；需要合并后的 main ARM job，Windows 通过不能替代 |
| Intel 上传 ENOTFOUND | 外部失败，待验证 | 保留原始日志，需要新 main 发布实际上传成功 |

本阶段不宣称主分支发布已修好；以新的完整发布结果作为最终证据。
