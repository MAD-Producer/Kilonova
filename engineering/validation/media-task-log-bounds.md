# 媒体任务日志字段边界

推进 #138 的错误边界及 #130 的后台任务诊断，不关闭两个合集。基于已合入 #392 的 main `2803721b30ecbde1a6714ac74a6ca2267618f9b2`；开工前检查其他工作树，资源预算和精度工作继续由原分支处理。本次不修改它们的 Session、渲染、分配预算或像素算法。

## 改动与边界

媒体 TaskLog 原先直接输出供应方的完整错误和请求字段。现在请求最多 128、操作及阶段最多 64、原因最多 2048 个 Unicode scalar values；C0/C1 控制字符及 Unicode 行/段分隔符替换为 U+FFFD。超限记录以省略号分隔头尾，保留开头的操作上下文及末尾的原生原因。UTF-8 字节数仍有四倍字符限额的上界。

这是既有观察的载荷治理，不改变任务 ID、任务结果、原生错误、JSON 回复、错误级别、普通取消或完成语义。业务状态中的完整错误仍遵守原响应边界；不会用日志摘要替换它。没有新增观察键、计数器、阈值、重试、线程回收、超时或 Present 日志。只在 TaskLog 构造及终结时处理有界文本，没有逐帧 IO，也没有在业务锁内新增 logger 调用。

保留尾部有助于诊断常见的“上下文: 原生错误”，不保证任意供应方文本中的所有中间原因都能放入摘要。独立 panic hook、其他日志站点和已有任务状态的总体载荷治理不属于本次完成范围。

## 证据审计

| ID | 分类 | 证据及限制 |
| --- | --- | --- |
| MTB-01 | 观察字段边界修复 | Unicode 单元用例覆盖精确边界、超限头尾、空字段、零限额、NUL/CR/LF/TAB/ESC/NEL 与 U+2028/U+2029；合法非 ASCII 内容保留。没有使用字节截断破坏 UTF-8。 |
| MTB-02 | 真实失败与原响应 | 真实 AudioJobs 供应方执行 File::open，缺失文件产生 NotFound；外加含控制字符的 6000 字上下文。任务及 60 次 JSON 状态查询保留完整错误和原请求 ID，默认文件与 Error stderr 只输出一次清理后的有界错误，尾部保留原生 os error 2；stdout 不含记录。该用例未单独为其他供应方生成同样的长错误。 |
| MTB-03 | 既有开始信号与受控阻塞 | 用信道阻止供应方执行 IO，先从实际日志文件读取已 flush 的 Info start，并检查公开任务阶段为 opening；之后才释放并触发真实失败。开始信号不依赖终结记录，Info 不写 stderr。这里验证既有发布顺序，没有新增等待或在途机制。 |
| MTB-04 | 验收负面对照 | 暂时将原因限额 2048 改为 8192，相同默认日志回归退出 101，实际长度 6069、期望 2048。恢复后的生产源文件 SHA-256 为 `39cba3e6458920d0272e52cf586f874483ef20889adcb38264155db1ce403797`；源码字节相同且更新时间戳后重新编译。未放宽既有断言或提交错误变异。 |
| MTB-05 | 正常、取消与 panic | 保留媒体完整回归中实际音视频/包失败、受控阻塞、取消保留原始失败、普通取消不报 Error、panic 有界详情和清理等覆盖。只改变经 TaskLog 输出的文本，不据此声称所有后台线程、独立 panic hook 或所有永不返回调用都已治理。 |
| MTB-06 | 测试读取同步修复 | 首轮 Linux CI run `39566479366` 的 job `119109870623` 在测试第 65 行读取正在追加的日志，实际报 `InvalidData: stream did not contain valid UTF-8`。任务状态先发布、日志随后写入，未同步的文件快照可能截在多字节字符中间。仅将测试快照放在既有文件 sink 锁内读取；保留完整 UTF-8、2048 字符、原始响应、双输出及 stdout 隔离断言，不改生产逻辑、不丢弃非法字节、不增加重试。跨平台修复结果仍需新提交的 CI 验证。 |

## 验证记录

使用同一工作树和专用 target、便携 MSVC、锁定离线依赖，保留 debug assertions；桌面启用 FFmpeg，单任务编译、单线程测试。

- 完整媒体回归：121 通过、0 失败、2 项既有大文件忽略，包含新的 Unicode 边界用例。
- 最终桌面 `logging::`：21 通过、0 失败、0 忽略，34.17 秒，包含新的真实供应方失败以及既有媒体、包、GPU、panic hook、日志阻塞与恢复用例。与完整媒体回归不重叠，合计 142 项通过；不冒充完整桌面回归。
- `cargo check --workspace --exclude motion-android --all-targets --features motion-desktop/ffmpeg --locked --offline -j1`：通过，52.40 秒。
- `cargo build -p motion-desktop --bin motion-studio --features ffmpeg --locked --offline -j1`：通过，22.00 秒，保留 21 条既有二进制警告；桌面测试构建保留 19 条既有警告。
- 最终新文件 rustfmt、差异空白、来源及面板样式消费者守卫通过；远端 CI 尚待提交后验证，不能据本机通过宣称跨平台通过。
- 首轮 Linux CI 暴露 MTB-06 的测试快照竞态后，同步读取修正的本机桌面 `logging::` 重跑：21 通过、0 失败、0 忽略，42.18 秒；日志为 `media-task-bounds-snapshot-logging.log`。只修改测试与本文，既有生产源码及负面对照保持不变；新的跨平台 CI 尚待验证。

本地 Debug 程序另带入经 runtime_payload 验证的 7 个共享 FFmpeg DLL 及原许可证；清除开发环境 FFmpeg 配置、PATH 仅留系统目录后，命令清单仍为有效 JSON，72 帧视频实际解码成功。程序及来源/输入哈希说明保存在 `artifacts/media-task-log-bounds/windows-ffmpeg-debug`，不是 Release 发布。

原始证据在本机 `artifacts/pr-ci-followups/media-task-bounds-*`：`negative.log`、`negative-proof.json`、`media-full.log`、`default-final.log`、`workspace-check.log`、`desktop-build.log`。负向与最终正向日志分开保存，阶段性筛选结果不与完整回归相加计数。

没有新增依赖、素材、可见 UI、面板或工程格式变更；已检查 PROVENANCE、ATTRIBUTION、NOTICE。DESIGN §0.7 的十二项基线均 retained，原因逐项沿用 `host-error-contract.md` 的 DESIGN 表：本次未编辑其面板源码。编译通过不代表这些视觉问题已整改。
