# 视频图层移除与重加的解码器峰值

关联 #131，接续视频线程关闭修复 #459。本批不修改 UI、媒体格式、依赖或实际原生解码时限。

## 复现与功能修复

旧实现只有 reader 总计四个活跃／退出中线程的上限。移除一个图层后，旧线程尚未释放原生解码器时，空余槽位仍允许同一视频重新创建解码器。实际 FFmpeg 文件验证同一对象 ID 和替换对象 ID 两种重加，原实现的 live 数量均从 1 变成 2。

现在 retiring 句柄保留对象、来源、输入路径、轨道及退出起点。没有活动 stream 的新请求，在同一 reader 内遇到相同输入路径／轨道仍退出中时，返回既有 `pending / decoder_retirement` 调度背压，直到旧线程完成析构并被 join 接收。输入路径由宿主传入的工程根与已验证素材相对路径组成；实际 Session 使用规范化工程根。没有加入文件 IO、全局路径扫描或依赖对象 ID 相等的判断。

两个已活动的同源实例保持独立；不同输入源仍可使用剩余槽位。退出超时不被伪装为资源已经释放，正式导出仍保留完整 join。`#459` 的 reader Drop 现在也能输出已保留的 retiring 对象与来源，代替未知 ID。

## 错误与观察分类

| 编号 | 分类 | 验证及边界 |
| --- | --- | --- |
| VR-S01 | 功能修复 | 真 FFmpeg 原生对象打开并完成首帧后受控阻塞。真实 Project 移除图层并执行空 Scene 准备，再重加同 ID／不同 ID；整个重开生命周期同源 live／peak 均为 1。释放后逐字节像素与 PTS 相同，finish 后 live 为 0，文件可重命名。不是 C 解码函数永久阻塞复现。 |
| VR-S02 | 正常路径 | 两个正常活动的同源实例 live 为 2，均得到原像素；不同输入源可在旧源退出时打开。保留四槽总预算和既有曝光／预取／取消语义，不加入全局单实例限制或自动重试。 |
| VR-S03 | 新有界观察 | 每个 retiring worker 持有一个饱和计数器，属于该 worker 的退出等待事件。请求在第一次等待仅 Debug，第 60 次 Error 一次，后续抑制；join 删除该项，后续退出事件重新计数。不会将 60 次称为一秒；这不是跨整个 reader 生命周期累计的错误计数。最多受既有四槽预算约束，无按历史对象增长的观察表。 |
| VR-S04 | 新状态与既有信道 | `retiringWorkers` 返回对象／来源、phase、耗时和完成状态；pending 返回阻塞对象／来源。默认生产 logger 子进程验证 59／60／120 次、两次退出事件的恢复重置，Error 文件／stderr 数量及上下文，Info／Debug 策略、stdout 隔离，恢复后的像素与原生 worker 退出。受控 provider 不冒称原生 FFmpeg 错误。 |
| VR-S05 | 既有错误协议 | stop 与 join 的原始失败保持；退休线程 panic 的既有断言不变。正常 supersession、不再需要的旧请求和空集合不被当作失败，没有修改 panic hook、respond 或工作线程重启策略。 |
| VR-S06 | 范围限制 | 这是单 reader 的图层生命周期约束，不是独立预览／导出 reader 的全局去重或整个应用退出证明。不同 reader 可合法持有不同时间位置的解码器。本批不据此关闭 #131 的缓存发布与其余音频时钟验收。 |

## 原始证据

本地记录位于 `E:/Dev/aem/artifacts/pr-ci-followups/`：

- 首轮 `video-retirement-before` 是新增测试模块引用路径错误，编译失败，不作为产品负面对照；只修正 `#[path]`，没有修改产品逻辑。
- `video-retirement-before-native` 在原产品代码实际运行 2 通过、2 失败；两项失败均为真实 live 数量 2，期望 1。负对照日志 SHA-256 为 `8447b9b66fedb6bc01f9a1b1667b0085605148100dc6b373503bbabb3e97a72c`。
- `video-retirement-after-native` 同字节测试 4 通过；测试源 SHA-256 为 `caae748899a91dbeb6361500f308e8c665736f7ba7f15469f37493b90a7e4057`。之后仅格式化新增测试文件。已有测试只补齐新增字段与内部 retiring 类型的构造，原断言和期限不变。

首轮完整回归 `video-retirement-final` 中，宿主 25 和 libav 11 通过；新增默认信道测试的 stderr 错误总数断言失败。原始 stderr 已含两条预期 Error，但新夹具错误地只按文件的带时间戳形式匹配。只修正这个新增断言以同时识别 stderr 行首 `ERROR `；产品逻辑、预期次数和上下文断言不变，原始输出保留。

最终冻结七个 Rust 源文件的 `video-retirement-accepted-validation.json` 五阶段均通过：宿主视频 25、真实 libav 11、桌面日志 3，共 39 个不同测试，0 失败，保留 6 项既有 GPU 忽略；工作区全目标检查和带 FFmpeg 产品构建通过。统一使用 `--no-default-features --locked --offline -j1`，测试使用 `--test-threads=1`。重复专项不累加，也不冒称整个工作区完整测试。

另存 `video-retirement-default-output/` 的默认生产文件、stderr、stdout 与执行收据。两次持续退出等待的 Error 文件／stderr 各两条，Info 只入文件，Debug 不可见，stdout 无产品日志；同一 reader 恢复正常像素，三次实际 worker 均退出。文件日志 SHA-256 为 `80ec07a55a623620775c4983f0115d1cd9d932bca02c735f81d11c97deb7168b`。实际产品启动返回 55 个 JSON 命令，stderr 为空，产品 SHA-256 为 `74b4bc1549de592b2b0a6ac94e85ec87e97d780ea6c86bf42d2b3003510c01f4`。使用开发 FFmpeg DLL PATH，不冒称自包含发行包验收。

本地基线为 #459 的 `42eba381`；相关原实现负面对照和源码收据均保留。其他线程的活跃媒体内存分支仅涉及缓存观测，未修改或审查其 PR；本批只给其既有测试夹具补齐内部 path 字段。最终组合及原生 main CI 尚须独立验证，不据本地结果宣布整个 #131 完成。
