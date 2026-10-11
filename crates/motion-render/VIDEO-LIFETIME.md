# 视频资源生命周期核验

本项核对 [#68](https://github.com/Yukk1o/motion-studio/issues/68)，基于主分支 `6571437`（已包含 #92 与 #93）。现有代码已经有活动集回收，本次补充回归验证，不引入另一个 idle/LRU，也不把已有机制描述成新增优化。

## 已核对入口

| 入口 | 所有权与回收 |
| --- | --- |
| 交互预览 | `motion-host/src/session.rs` 的 `render_with_overlay` 在帧准备前调用 `retain_video_instances` |
| PNG 捕获 | `motion-host/src/ops/export.rs` 配置资源、准备精确帧时调用活动集回收；无窗口时的临时 renderer 随调用释放 |
| 嵌套合成 | `retain_video_instances` 递归收集 nested 场景的视频 / 合成实例，区分相同 ID 的两类用途 |
| 工程 / 资产切换 | `configure_assets` 与 `replace_assets` 处理资源目录和纹理身份；现有 `image_working_set` 与 `compositions` 测试覆盖同 ID 重新绑定 |
| 隐藏 / 裁剪 | `Scene::sample` 排除不在片段时段内以及未被引用的隐藏图层；轨道遮罩和效果输入引用的隐藏来源继续采样 |
| 独立预览输入 | `PreviewInputTexture` 只使用一个转换实例；RGBA 输入清除旧视频，YUV 输入清除旧 RGBA，尺寸变化替换目标 |
| 冻结导出计划 | 冻结的采样 / 计划数据不持有这些 renderer 纹理；实际输出平台的运行时仍需分别验收 |

`Renderer::retain_video_instances` 删除失活实例的 RGBA 输出时，也调用 `VideoGpu::remove` 删除对应 YUV 平面。删除 map 中的资源，不代表驱动立即释放仍被已提交命令消费的纹理；WGPU 负责 GPU 引用的执行生命周期。

## 新增回归

- 顺序遍历 60 个同源视频片段，再反向 seek，保持一个活动实例的逻辑容量并验证重复 PTS 去重
- 进入没有活动片段的时段后，RGBA 与平面容量一起回到基线
- 保留被轨道遮罩引用的隐藏视频来源，回收未引用的隐藏视频；移除遮罩引用后释放剩余视频
- 独立输入连续切换 60 次 YUV / RGBA，确认两类源不会累积，并在透明输入后回到一张 1×1 输出

```sh
cargo test -p motion-render --locked --test video_lifetime -- --test-threads=1 --nocapture
```

工程和 YUV 图像由测试生成，不依赖外部视频文件；该测试验证渲染资源生命周期，不验证真实解码器、AHB、Surface 或输入容器。

本地三项测试通过。16×16 合成 YUV 源的片段遍历记录为：基线 4 字节、峰值 1412 字节、空白时段恢复为 4 字节；常驻效果占位图等未计入此旧口径，不能将这些数值当作物理显存。

## 当前证据边界

容量来自既有逻辑纹理计数及 `PreviewInputTexture::memory_bytes`，不是驱动驻留显存。峰值记录不含永久效果占位图、GPU buffer、上传 staging 或外部 Surface；新分项台账另见 #94。

既有 `video_instances`、`video_yuv`、`compositions`、`image_working_set`、`preview_input` 测试提供多实例、旋转、换工程与独立输入覆盖；合并编码的取消重试由 #93 验证。

本次没有发现需要新增 LRU 的活动集回收缺口。真实 Android 解码器与外部 Surface 的驻留峰值、冻结成片输出和内存压力降级仍是独立验收项，因此保留 #68 开放。
