# MP4 音频时钟进入解码器前的校验

关联 #131、#135 与 #138。分支从已合入 #404 的 main 776c1b31 开始；开工时检查远端 PR 和本地工作树，#405 的形状参数工作在另一工作树进行。验证前同步 #405 的 b1dc9329，作为堆叠 PR 的基线。这里不改面板、渲染预算、素材、依赖或编解码器源码。

## ACP-01：实际故障与功能修正

现有 MP4 零时间基检查在 Symphonia 初始化之后。将仓库已有合成音调的 mdhd 时间基改为零，从公开 decode_audio 入口会在第三方 TimeBase 构造时 panic，尚未到达项目的拒绝。audio-clock-crop-corrected-before.log 记录原实现四项通过、一项因该 panic 失败；不是模拟错误或只检查守卫代码。

现在先执行有界 MP4 元数据读取，检查 mvhd 及所有轨道的 mdhd，再进入第三方初始化。未选择的轨道也要检查，因为解析器会为全部轨道构造时钟。错误携带固定 atom 名和轨道序号，经既有 Result 传播；不捕获后吞掉 panic、不补默认时钟、不重试或重启。读取结果继续用于选中音轨的编辑列表，不再从磁盘读取第二份 moov。保留 16 MiB 元数据、4096 个顶层/子级 atom 和 32 个轨道的限额；遍历媒体大块只 seek 读取头，不读完整音视频载荷。

第三方会使用后出现的重复头；只验证第一个头会留下绕过。单值 mvhd、mdia、mdhd、edts、elst 以及顶层 moov 出现重复时明确拒绝。版本 0/1 的时钟按各自偏移读取，其他版本拒绝。这里针对现有 ftyp 开头的 MP4 音频路径，没有宣称穷尽所有第三方解析器的任意输入 panic。

## ACP-02：已有拒绝、正常裁剪和测试修正

64 位 elst 的时长/起点乘采样率溢出，以及输出范围内两小时时间戳空隙超过缓存限额，原实现已经返回错误，本轮补实际输入覆盖，不记为新产品修复。正常时间戳空隙、重叠与时码舍入保持原有逐样本断言。

新用例初稿错误假定原 stts 有两项，实际 fixture 只有一项；修正测试构造，不修改产品。第二次执行还错误要求拒绝编辑范围外的大空隙，原实现合法裁剪并输出两秒静音。保留两次原始失败日志，将该情况明确作为正常裁剪的精确帧数/静音字节验证，另构造真正覆盖长空隙的输出范围验证预算拒绝。没有修改已有断言、增加忽略或放宽期限。原始日志分别为 audio-clock-before.log、audio-clock-fixture-corrected-before.log；成功载荷曾被 unwrap_err 打印，后续诊断改为只显示帧数，原文件保留。

原 issue 将当前 TimeBase.numer 描述为 u64，但已锁定的 Symphonia 0.5.5 字段是 u32；MP4 使用 numer=1，采样率受 192 kHz 限制。因此不能用这些文件声称已复现原文所述 160 位 packet 积溢出。checked_mul 继续保留为纵深防御，本次真实缺陷是进入这些守卫之前的零时钟 panic。

## ACP-03：错误信道与观察边界

桌面用例经真实 Engine 命令、文件打开、AudioJobs 解码 worker 与 media_status 验证错误；任务须为 failed 并保留 atom/轨道信息，不得落入 worker panic 文案。释放任务后，同一进程重新探测原始正常音频，要求 96000 个采样帧。默认 Info 文件、Error stderr、stdout 隔离和 Info start 先于错误均作实际输出断言；不直接设置 Flight，不用伪造成功解码结果。

没有新增 logger、观察键、计数器或锁内记录，也没有修改任务状态/回复协议、缓存发布、线程退出或故障期限。既有受控阻塞与锁外记录覆盖仍在 task_logging 与 media-error-boundaries.md；本测试不额外声称完整的阻塞时序、解码器永久挂起或全应用退出验收。

## ACP-04：验证状态与剩余范围

完整本机验证为媒体 129、核心音频 13、新桌面信道 1、既有真实媒体故障信道 1，共 144 项通过、0 失败；保留两项原有大文件忽略。带 FFmpeg 的非测试桌面构建通过。五阶段各自及其间全部 Rust/Cargo 源哈希保持不变，记录为 audio-clock-final-results.json、-source-receipt.json 和各阶段原始日志。与其他工作树串行使用共享编译目录，切换前清理 motion-model 构建缓存。

首次完整测试通过后，额外保留默认原始输出时，本批新增的整份状态相等断言失败。实际读回并逐字段比较原始输出，唯一差异是 `/desktop/diagnostics/droppedRecords` 从 0 增加到 1；该计数在诊断 try_lock 竞争时正常递增，既非工程修改，也不应被清零隐藏。本批新增测试现要求该计数单调不减，在比较用的副本中单独规范化此一字段，再保留其余完整状态的相等断言。未修改产品诊断、错误返回、旧测试断言或任何期限。失败的 file/stderr/stdout 和 state-differences.json 保留于 audio-clock-default-output/，不将该次执行记为通过。

最终新增信道实际重验通过，耗时 38.73 秒（含编译）；audio-clock-test-final-results.json 和对应源码收据确认只有本批新增 audio_clock_tests.rs 与完整五阶段收据不同，其余 Rust/Cargo 字节一致。另直接执行同一个最终测试二进制保留 audio-clock-default-final-output/：默认 Info、同一原始任务错误在文件和 stderr 各一条、stdout 隔离，随后正常探测得到 96000 帧。该重跑替换此前同一个测试，不增加 144 的独立总数。完整收据关系为 audio-clock-final-verification.json。远端 exact-head CI 尚待结果，不用本机构建替代跨平台验证。

#131 的缓存并发可见性、持久性和全局解码器生命周期仍未完成，#135/#138 其他覆盖及错误接口也继续跟踪。没有新增资产，合成音调 fixture 字节不变，测试只在临时目录改写容器元数据；PROVENANCE、ATTRIBUTION、NOTICE 已检查。DESIGN §0.7 十二项全部 retained，原因是没有修改对应面板；不新增视觉整改或截图验收声明。
