# 真实 FFmpeg 解码器回收的资源与像素验证

关联 #131、#135、#68。已有宿主回收测试用替身模拟不能及时响应取消的解码调用；本次在原有 libav 集成测试入口追加真实原生解码器对照，不改变生产调度、错误回复、日志或 UI。

## NVR-01：实际资源与正常背压

复用已归属的 silent-24fps.mp4 测试素材，复制到临时工程后，使用 DesktopPlatform 实际探测、打开四个 FFmpeg 解码器并解出首帧。测试包装器在解码完成后用受控门闩保留真实文件、codec 和解码上下文；它没有模拟或替代 FFmpeg 的解码结果，也不声称复现了卡在 C 函数内部的原生调用。

清空 reader 后，四个仍未返回的解码器须继续计入 retiring；连续 60 个替换请求须明确返回 decoder_retirement 背压，不能新开第五个实例。释放门闩后，finish 实际 join 所有 worker，包装器在销毁原生解码器之后才增加 closed、减少 live。随后八次重新打开、解码与 finish，逐次比较首帧 PTS 和完整 RGBA 字节，检查所有缓存及 retiring 清空。全程峰值四个，最终十二个原生 decoder 已析构、live 为零，并能重命名临时输入文件。

这证明一个 VideoFrames reader 内的回收槽位契约与实际原生析构，不证明多个 reader 的全局峰值、Android MediaCodec、关闭整个应用的线程计数，或驱动永久挂起时的进程退出。重命名是额外 OS 检查，不能据此单独证明所有平台文件句柄均关闭。

## NVR-02：负面对照与最终恢复

临时将生产 admission 条件的 streams + retiring 改为只检查 streams。实际编译后新用例在运行时退出 101、0 通过 1 失败：替换请求没有 decoder_retirement 原因，断言拒绝该错误实现。finally 逐字节恢复 video_frames.rs，SHA-256 为 `66ad92c906efb174c1162045effbb9012fa5c975ee8890ea7b53ba515c501e65`；证据为工作区 native-retirement-negative.json／.log。

恢复后，完整 FFmpeg adapter 37 项与既有 host 回收 1 项全部通过，0 失败、0 忽略；两阶段含编译耗时 66.45／5.62 秒。source receipt 核对每阶段前后全部 Rust／Cargo 源文件不变。既有 libav 测试和断言不变；默认无 FFmpeg 的 PR job 不执行该模块，push 主线的原有 libav 验收入口会包含它，远端原生验收仍待执行。

## NVR-03：观察与处置边界

没有新增观察键、日志、错误处理或阻塞产品操作；门闩只在测试内，保持既有五秒测试等待，不增大任何产品期限、不重试失败、不增加忽略。原有正常 pending／取消继续按正常状态处理，不应作为故障输出。没有新增默认日志信道验收，也不把数量／像素通过当作整个 #131 或 #68 已解决。

没有依赖、第三方素材或渲染语义变更；复用既有 fixture 不修改其字节。DESIGN §0.7 十二项全部 retained，逐项理由沿用 host-error-contract.md；没有相关面板变化或视觉整改声明。

## NVR-04：堆叠分支的最终组合

已同步 #400 主线 d60aaea2，并堆叠到 #402 的 03c6b228，使用更新后的官方 TLS／ZIP 依赖。最终 FFmpeg adapter 37 项与宿主回收 1 项再次全部通过，无失败或忽略；同一源收据还覆盖模型全套 25 项、核心候选编辑 2 项，共 65 项，不与前述重复运行相加。四阶段前后 Rust／Cargo 文件均未改变，日志为 oracle-audit-stack-*。远端 native FFmpeg 仍依赖父 PR 合入后的主线验收，不把默认 PR 编译当作原生设备执行。

## NVR-05：路径修正后的最终回归

同一 PR 随后合批 #138 的路径输入错误修正。原生回收测试和生产视频调度未修改；在该最终组合中 adapter 全套 37、host 回收 1 再次通过，包含其他阶段共 117 项、0 失败／忽略，带 FFmpeg 的桌面构建通过。七阶段全部 Rust／Cargo 哈希一致，详见 vector-path-input-errors.md。历史 65 项不再另行累加；默认 PR 任务仍不运行真实 FFmpeg，远端原生主线验收继续待执行。
