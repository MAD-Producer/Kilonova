# 视频转换与帧提交

无嵌套合成的交互预览把 YUV→RGB 转换、效果合成和呈现编码到同一个 command encoder，由宿主一次显式 `queue.submit` 提交。多路视频各自保留平面纹理与转换输出，按实例与 PTS 去重。

这项改动减少转换专用的显式提交，仍通过 `queue.write_texture` 上传 CPU YUV 平面；它不是硬解显存直通或零拷贝，也不保证帧时间按提交数同比下降。

## API 与取消

- `Renderer::upload_video_yuv`：原有便利接口，立即提交转换，适用于独立输入、截图/导出与现有嵌套合成路径
- `Renderer::encode_video_yuv`：向调用方 encoder 写入转换，返回 `Option<EncodedVideoUpload>`；命中已提交的同源、同 PTS、同尺寸缓存时返回 `None`
- `EncodedVideoUpload::mark_submitted`：在提交对应 encoder 后消费确认令牌；提交代表已入队，后续读取依赖 GPU 队列顺序，不代表执行完成

顺序为：创建 encoder → 编码视频转换 → 编码合成/呈现 → 提交 encoder → 确认令牌。

发生错误或取消时，丢弃 encoder 与全部令牌。令牌的 Drop 将转换标为取消；即使输入的 PTS 未变，下次上传也会重试。未提交的同一实例不允许再次上传，以防平面纹理或 uniform 被第二帧覆盖。不同实例可以共同编码。

同一 encoder 的生命周期内必须保留令牌。取消后不能再提交旧 encoder；不能在实际提交之前调用 `mark_submitted`。RGBA 回退在取消后仍可重新上传和恢复去重。

## 嵌套合成边界

嵌套子合成目前通过自己的提交执行，并复用部分 GPU 缓冲。将父级视频转换留在尚未提交的 encoder 内，会导致子合成先读取旧视频。直接把所有子 pass 移入父 encoder 还需要处理 uniform 和效果资源的生命周期。

本期宿主在 `scene.nested` 非空时保留独立转换提交。渲染器也会拒绝包含未提交视频转换的嵌套编码，避免产生静默错误。此处没有声称整帧恒为一次提交；嵌套统一编码属于后续工作。

视频旋转、色度布局与裁剪相位、BT.601/709/2020、全范围/有限范围的转换数学与 WGSL 保持不变。VFR 和源时间选择仍由既有解码与帧准备模块负责。

## 诊断与对照

`preview.video` 增加：

| 字段 | 含义 |
| --- | --- |
| `standaloneConversionSubmissions` | 该 renderer 的转换专用显式提交次数 |
| `encodedConversions` | 写入外部 encoder 的转换次数，包含随后取消的尝试 |
| `gpuConversions` | 保留原字段，转换编码总次数 |

这些计数不包括 `queue.write_texture/write_buffer` 的隐式上传、截图回读提交，也不是 GPU 执行完成计数。

```sh
cargo test -p motion-render --locked --test video_yuv --test video_instances --test preview_input --test compositions -- --test-threads=1
cargo run -p motion-render --locked --example video_submit_probe -- target/video-submit-probe.json
```

探针比较 1080p 单视频的原接口与合并接口，预热 8 帧、测量 32 帧，记录 CPU 上传/编码/提交时间、带逐帧 Wait 的墙钟延迟、显式提交数，以及可用时的合成 pass GPU 时间。

GPU 时间不包含 YUV 转换；墙钟时间包含 16 字节计时回读和逐帧等待，不能作为播放吞吐量。测试工程没有解码、效果、嵌套或窗口呈现；结果必须连同适配器、构建模式和测量范围解读。

GPU 测试对原路径和合并路径使用同一套色彩与旋转金图，并覆盖取消重试、多实例不同 PTS、效果读取与 seek、嵌套回退和独立预览输入。
