# 视频资源回收与在途预算验收

对应 #68。基线为 main `26c1d2d2bc83c8073d85a200a7cc78cd6284d4f4`。
本分支只增加回归和记录；不改变回收算法、缓存策略、渲染语义、日志信道、
既有断言、预算或等待期限，没有面板、依赖及资产变更。

## 范围与入口

| 项目 | 当前实现与本次处置 |
| --- | --- |
| 常规交互预览 | `motion-host/src/session.rs` 在媒体准备前调用 `retain_video_instances`；静态入口核对，实际窗口并非本批新增验收。 |
| 曝光预览与捕获 | `render_sampling::ExposureMedia::prepare` 先清失活视频，再准备准确的接收时钟；正常异步未就绪不是故障。 |
| 静帧捕获 | `ops::export::capture` 在准确视频准备前后保留活动实例；沿用既有 GPU 像素回归。 |
| 冻结电影 | `motion-desktop-media/src/movie.rs` 普通路径逐帧回收，曝光路径复用 `ExposureMedia`；新增真实 FFmpeg/GPU 的直接与嵌套导出、处理中途取消、重开和共享预算回落控制。 |
| 嵌套与同源实例 | `Renderer::retain_video_instances` 递归收集实例；既有 `video_instances` 验证不同源时间不互相覆盖，新增嵌套电影独立解码核对颜色。 |
| 独立输入 | `PreviewInputTexture` 独立拥有输出和平面；既有 `video_lifetime` 含 60 次 YUV/RGBA 输入切换与透明重置。 |
| 工程/资产切换 | `clear_assets` 与 `replace_assets` 释放/更换缓存所有权；新压力用例在提交前清除缓存，验证真实转换 token 仍占用预算。 |
| 隐藏/裁剪/删除 | 活动集依据已经采样的场景；既有遍历与反向 seek、空档和隐藏 matte 回归验证失活释放。没有新增 UI 行为。 |

## 统计与边界

纹理账本按 `VideoRgba` 和 `VideoPlanes` 记录应用拥有的纹理身份及逻辑容量。
全局 `AllocationBudget` 的 `Video` 类同时包括两者，另外保留 uniform 类；
它覆盖已登记分配及在途保留，但不是驱动驻留显存、分配器页开销或系统总显存。
不能用清空后的纹理账本冒充 GPU 已经完成。

新压力用例使用三个独立编码器，每个 2×2 转换为 RGBA 16、Y/UV 6、uniform 48 字节。
先核对三类纹理的身份数量与容量，再移除全部缓存所有权。
这时三个未提交 token 必须继续占用 210 字节；第四次输出分配必须准确拒绝且无残留。
取消一个编码器/token 仅先释放其 16 字节输出，真正提交并等待队列后才释放其余 54 字节。
另外两个未提交转换仍各占 70 字节，只有分别提交及完成后才能回到系统基线。
20 批覆盖 60 个 clip 和每个编码器位置的取消，不依赖“GPU 应当还没执行”的墙钟猜测。

外部解码 surface 引用当前没有接入这条链：`motion_host::video_frame::VideoPixels`
只有 `Yuv(Yuv420Frame)` 和 `Rgba(Vec<u8>)`。本批不声称验证未实现的外部 GPU 互操作，
也不把 wgpu 呈现 Surface 算成解码 surface。该能力仍应随独立互操作任务验收。

没有实测证据支持保留冷视频纹理，继续使用已经存在的活动集删除，不引入 idle/LRU。
正常缓存未命中、主动取消和替换不新增错误日志；本批不改变生产错误处理。

## 已执行与待验收

- 首轮压力控制真实运行：1 通过、0 失败、0 忽略；系统基线 1,344,592、
  峰值 1,344,802、最终 1,344,592 字节。期间格式化/新增测试文件，
  全树快照不同，保留为首轮记录，不能作为最终源码冻结验收。
- 负对照只在隔离工作树暂时移除上传 token 的分配保留，既有测试逻辑不变。
  实际运行 0 通过、1 失败，明确在清缓存后的容量断言拒绝缺失保留的实现。
  之后逐字节恢复 `video_gpu.rs`；SHA-256 为
  `13286e36ed17288412f7acd6917486f39fea4007d3851aa704ff0c775825c1c2`。
- 原生电影首轮是新夹具的临时值借用编译错误，没有执行产品测试。
  第二轮实际取消和预算归零已经通过，但夹具把 Session 创建的正常 plugins 目录
  计入输出清理检查，导致目录断言失败；将电影放到独立 outputs 目录后，
  仍逐项严格比较该目录完整内容，不忽略文件或临时输出。
- 第三轮原生电影：1 通过、0 失败、0 忽略，源码前后快照一致。
  直接/嵌套两种工程各执行一次中途取消、一次完整 60 帧导出；
  取消均已处理超过 20 次进度回报，保留旧输出。完整输出各解码 9 个源时间，
  两轮 H.264/YUV420 在夹具预先固定的 16 级容差内，实际通道最大偏差 1。
  每次返回的所有类别占用、分配数和编码器数均为 0。
- 渲染 7 组共 31 通过、0 失败、0 忽略：submission_memory 2、texture_memory 6、
  video_allocation_budget 2、video_instances 5、video_lifetime 8、新压力控制 1、video_yuv 7。
  包括同 PTS 不重复上传、不同 PTS 实例像素、活动/隐藏 matte、反向 seek/空档、
  独立输入切换、RGBA/YUV 交接、缓存别名和真实提交后的释放，源码前后快照一致。
- 宿主 3 组共 15 通过、0 失败、0 忽略：preview_inputs 11、preview_memory 3、
  video_retirement 1，源码前后快照一致。包括真实请求入口、取消/重置后像素恢复、
  精确源时钟和保留中的原生解码器容量。
- 合计 47 个不同测试通过，不重复累加首轮压力或负对照，
  不冒称完整 workspace 回归；当前提交跨平台 CI 与主线原生 FFmpeg 门禁尚待验证。
  #68 保持打开，待合入和相关门禁后核对关闭。

PR 的现有 Desktop tests 不启用 FFmpeg，因此不会执行上面的原生电影用例。
本地命令显式启用 `--features ffmpeg`，实际执行而非条件编译成零测试；
合入后的现有主线 FFmpeg 验收命令也会覆盖它。本批不修改工作流或扩大 PR 构建矩阵。

```text
cargo test --locked -p motion-render --test video_retirement_pressure \
  --test video_allocation_budget --test video_lifetime --test video_instances \
  --test video_yuv --test texture_memory --test submission_memory -- --test-threads=1 --nocapture
cargo test --locked -p motion-host --test preview_memory --test preview_inputs \
  --test video_retirement -- --test-threads=1 --nocapture
cargo test --locked -p motion-desktop-media --features ffmpeg \
  --test video_retirement_export -- --test-threads=1 --nocapture
```

原始证据保存在工作区 `artifacts/issue-goal-current/video-resource-retirement/`：
每次命令、环境、源码前后 SHA-256、退出码、耗时、stdout 和 stderr 分开保存。
`mutation-restoration.json` 记录负对照及恢复身份；首轮与失败记录不覆盖。

可携带的 [原始验证归档](video-resource-retirement-evidence.zip) 共 30 个成员，
逐成员 SHA-256 已读回核对；整体 SHA-256：
`70a7fb5186e814db8441b31725604dfa45685950905150620e161ce9bcf79502`。
