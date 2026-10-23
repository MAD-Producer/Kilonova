# 视频解码线程退出与锁外析构

关联 #131。本批补齐 `Stream::drop` 的退出处置，并修复失败后的原生解码器析构持有状态锁的问题。没有修改面板、媒体格式、依赖、解码重试或取消协议。

## 功能边界

原实现只设置停止标志，随后隐式丢弃 `JoinHandle`：已结束线程的 panic 没有被 join 接收，仍阻塞的线程也没有明确的关闭超时记录。现在先通知停止，再 join 已完成线程；一个 reader 的所有线程共用 50 ms 等待预算。超时逐个明确记录 `disposition=detached`，然后释放句柄所有权。Rust 无法安全终止原生解码调用，超时后其资源仍由工作线程持有，直到调用返回。这不是线程重启、额外解码重试或资源已经释放的证明；OS 调度与同步日志也可能使实际耗时超过等待预算。

正常场景淘汰仍将句柄交给既有 retiring 槽位，不在 Present 热路径加入同步等待或无条件 start/done。正式导出的显式 `finish()` 保留原来的完整 join 协议。

失败分支原来在持有停止／状态查询锁时执行 `decoder = None`，原生析构阻塞会同时卡住 `stop()`。现在先发布错误和完成状态、释放锁、记录原始失败，再析构解码器。原生 `seeks()` 查询也移至锁外。正常 supersession 保留原解码器，不被改成故障或重建。

## 错误处理审计

| 编号 | 分类 | 证据和边界 |
| --- | --- | --- |
| VS-01 | 功能修复和新观察 | `Stream::drop` 接收已完成线程的原始 panic；停止失败与 join 失败分别保留，含 owner、object/source、线程、phase、耗时。正常空集合不记录。 |
| VS-02 | 功能性超时边界和新观察 | `VideoFrames::drop` 对活跃及 retiring 句柄共用等待预算。Info start 在等待前落盘；超时为 Error，明确说明只丢弃句柄、原生资源尚未释放。最多按所属 reader 的既有四个槽位记录，不是每帧累计计数或 60 次阈值站点。 |
| VS-03 | 功能修复 | 控制失败后的实际解码器析构，阻塞期间状态锁可取得、原始错误已记录、in-flight 已清除；不以模拟成功返回代替失败路径。 |
| VS-04 | 既有覆盖复验 | 音频／视频 `Stage` 清理和打包清理已有锁外实现；媒体库单元回归执行受控失败、取消、panic、阻塞及真实暂存清理测试，不重复认领为本批新增实现。 |
| VS-05 | 默认信道 | 新桌面子进程使用生产 logger、默认 Info，实际阻塞解码并触发 reader Drop 超时。父进程检查 Error 文件和 stderr 各一条、Info 只入文件、stdout 隔离；释放原 worker 后确认退出，再以显式新 reader 验证正常像素。该受控 provider 不是 FFmpeg 永久阻塞复现。 |
| VS-06 | 正常／协议分支 | 正常 cache miss、supersession、取消仍遵守原语义；不加入故障重试、自动重启或成功伪装。原日志主因先于可能阻塞的析构，logger 不在状态锁内调用。 |
| VS-07 | 待办 | 不据此关闭 #131：缓存对并发读者的原子发布、移除并重加单个视频层的实际峰值、畸形音频时钟的剩余验收独立保留。reader 超时不是整个应用无后台线程的保证。 |

## 负面对照与测试边界

原始证据位于开发工作区 `E:/Dev/aem/artifacts/pr-ci-followups/`，不把本地日志当作远端 CI 已通过。

- `video-shutdown-before`：相同新增测试在原产品实现上实际 0 通过、2 失败，分别缺少 join panic 原因与显式关闭超时。测试文件 SHA-256 为 `7f1b8245b5e15d67e2bdd9762527c786c4e67083776d270f6db7dfacc98dada4`；修复后的 `after-owned-drop` 同字节测试 2 通过。
- `video-shutdown-before-close-lock`：只将析构位置恢复到旧锁内，其余关闭修复保留，实际 2 通过、1 失败，状态锁不可取得。恢复锁外实现后 `after-close-lock` 相同三项测试全部通过，测试 SHA-256 为 `0987a763cc9969993f39d8e98bad15f0d38514869643d2540dc1975bbe8c721c`。随后只格式化新增文件。
- 首轮 `video-shutdown-after` 保留添加 Drop 后结构更新语法无法移动字段的编译错误；仅将构造器和两个既有测试初始化改为先 Default 再赋值，没有修改断言。
- 首轮完整 `video-shutdown-final-host` 实际 24 通过、1 失败、5 项既有 GPU 忽略。既有“停止后失败与取消”夹具在 Drop 返回之后才释放受控解码，必然新增两条超时 Error，使原本的一条错误断言失败。改为显式调用同一 `stop()`、释放门、等待线程结束再 Drop，原断言和期限全部保留；新的超时专项继续保持门阻塞跨过关闭预算。不是过滤错误、延长预算或降低断言。

最终六阶段收据 `video-shutdown-accepted-validation.json` 已完成，六个 Rust 源文件在执行前后字节不变：

| 实际命令范围 | 结果 |
| --- | --- |
| `motion-host --lib`，`diagnostics`，过滤 `video_frames::` | 25 通过，5 项既有 GPU 忽略 |
| `motion-media --lib` | 92 通过，无失败或忽略；不是全部媒体 integration tests |
| `motion-desktop-media --test libav`，`ffmpeg` | 7 通过，无失败或忽略，包含真实 FFmpeg 回收和像素／PTS 比较 |
| `motion-desktop --bin motion-studio`，`ffmpeg`，过滤 `logging::frame_blend_tests::` | 2 通过，1 项既有 GPU 忽略 |
| 工作区 `--all-targets`，`motion-desktop/ffmpeg,motion-host/diagnostics` | 检查通过 |
| 带 FFmpeg 的 `motion-studio` 产品构建 | 通过 |

以上统一使用 `--no-default-features --locked --offline -j1`；测试另使用 `--test-threads=1`。合计 126 个不同测试通过，无失败，保留 6 项既有 GPU 忽略。重复专项不累加。真实 libav 用例验证既有四槽回收和逐字节像素，不冒称新的永久原生阻塞测试。既有未使用代码等编译警告保留，没有引入过滤或放宽断言。

`video-shutdown-default-output/` 另保存默认 Info 的生产文件、stderr、stdout 和执行收据：超时 Error 文件／stderr 各一条，Info 只入文件，stdout 无产品日志，释放阻塞后原 worker 退出，显式新 reader 正常；文件日志 SHA-256 为 `12048eddbd7c9998370afc57f89d7615139d61842b14164ee86f1a021d08fb9e`。相同源码下实际产品 `--command-list` 返回 55 个 JSON 命令，stderr 为空；产品 SHA-256 为 `5996d2ee6f5eb3572cac6eac9b3d70e29be2b181a3acd042358e6306ff4c37ae`。使用开发环境 FFmpeg DLL PATH 执行，不冒充自包含发行包验收。

本地基线为 main `002fadcd`。随后 main 的 #453、#458 和 #457 路径均与本批七个文件无交叠；没有审查或修改他人 PR。最终合入组合仍须由本 PR 的实际远端 CI 验证，以上不是新 main 组合的完整本地回归。
