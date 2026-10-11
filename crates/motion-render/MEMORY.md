# 逻辑纹理台账

`Renderer::texture_memory_breakdown()` 返回只读、分项的逻辑纹理容量。它按渲染器实际持有的纹理和池容量统计，复用现有资源计数，不进行 GPU 回读，也不改变预算、分配、缓存或降级策略。

## 统计范围

| 分类 | 所有者与口径 |
| --- | --- |
| `staticImages` | 静态图片，排除永久占位图 |
| `videoRgba` | 每个视频实例的 RGBA 输入 / 转换输出 |
| `videoPlanes` | Y 和 UV 平面，按实际平面尺寸与通道数 |
| `nestedCompositions` | 嵌套合成输出，与视频 RGBA 分开 |
| `vectorSources` | 缓存的矢量栅格源 |
| `effectScratch` | 独立尺寸的效果池，普通槽 4 B/px、浮点槽 8 B/px |
| `pluginResources` | 效果资源纹理，复用已有唯一资源计数 |
| `curveLuts` | 当前曲线 LUT，1024 字节 / 张 |
| `vectorRasterScratch` | 常规矢量 4× MSAA 加解析输出，合计 20 B/px |
| `vectorGroupScratch` | 按尺寸与作用域深度共享的分组 MSAA + 解析池 |
| `layerCompositingScratch` | 图层合成累积纹理 |
| `maskScratch` | R8 蒙版池 |
| `maskOutputs` | 按图层持有的最终蒙版输出，与中间池分开 |
| `effectInputSnapshots` | 为其他图层效果输入保留的快照 |
| `matteBlendScratch` | 轨道遮罩与混合输入池，包含它自己的白色占位纹理 |
| `systemTextures` | 主占位图、效果白色占位图和恒等曲线 |

分项互斥。纹理视图、bind group 和对同一池的多次引用不会再加一份容量。分组矢量共享 scratch，但不同图层自己的栅格源仍分别计入。

`total_bytes()` 是上述分类的总和。旧 `texture_bytes()` 保持兼容口径；新台账额外计入效果永久占位图和恒等曲线，共 1028 字节。两项白色占位图来自不同所有者，是不同纹理，不能按相同像素内容合并计数。

这是格式容量，不是驱动的分配、对齐、压缩、驻留或实际物理 VRAM 使用量。GPU buffer、上传 staging、CPU 解码缓存、解码器内部图片、swapchain、UI 和独立预览输入纹理均不在 renderer 台账范围内。当前没有最终帧纹理缓存或 AI 权重/激活池的可测台账。

## 共享后端 API

`preview_info().memoryBreakdown` 为可选对象；未绑定图形设备时为 `null`。

```json
{
  "version": 1,
  "scope": "logical_texture_capacity",
  "rendererBytes": 1032,
  "categories": {},
  "legacyTextureBytes": 4,
  "previewTargetBytes": 16384,
  "previewOwnedTextureBytes": 17416,
  "driverResidentBytes": null,
  "cpuDecodedCacheBytes": null,
  "cpuVideoDecodedCacheBytes": 0
}
```

这里示意的是空 renderer 加一张 64×64 RGBA 预览目标，`categories` 在实际响应中包含完整分项。预览目标由宿主持有，单列后再相加，避免混进 renderer 的资源所有权。

视频 CPU 缓存来自既有 `video.cacheBytes`，并不代表全部 CPU 解码内存；总 CPU 解码缓存与驱动驻留显存保持 `null`。响应也提供 `excludes` 列表。前端可将该数据用于开发者显示，不能将它标成整机“显存占用”或统一硬预算。

## 验证

```sh
cargo test -p motion-render --locked --test texture_memory -- --test-threads=1
```

测试实际创建图片、RGB/YUV 视频、分组矢量、曲线效果和嵌套合成，检查独立分类、共享池、调用方目标排除、重复渲染稳定性与释放行为。

本期完成 [#60](https://github.com/Yukk1o/motion-studio/issues/60) 的纹理所有权台账基础。真实 Adreno/Mali 压测、全部资源与 GPU buffer 台账、每帧原始记录和多场景性能基线仍需后续工作；现有效果成本报告仍是静态规划数据，不能代替耗时测量。
