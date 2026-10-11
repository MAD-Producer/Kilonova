# 有界预览资源降级

共享会话仅对 `ResourceRejection` 的已知预算/纹理尺寸拒绝降级；解码、非法数据、shader 或设备错误保留错误路径。图层、关键帧、效果参数和 undo 不受预览质量调整影响。

## 四个候选档位

| level | 图片最长边 | 视频最长边 | 目标尺寸相对现有预览策略 |
| --- | --- | --- | --- |
| 0 | Auto/Balanced/Economy 为 2048，High 为原图 | 原解码帧 | 1 |
| 1 | 1024 | 1024 | 1/2 |
| 2 | 512 | 512 | 1/4 |
| 3 | 256 | 256 | 1/8 |

短边采用整数向上取整且至少 1 像素；工程与图层的几何尺寸保持原值。High 也是预览选择，资源压力时可降级；正式截图、冻结计划和导出使用独立完整质量路径。

现有图片上传会优先淘汰闲置 LRU。拒绝后的重试会释放旧动态视频、矢量、蒙版、合成与效果 scratch，丢弃候选目标，并取消未提交的 encoder、视频确认 token 和 timer slot。效果包资源仍保持已验证身份；不会把内存不足记为失效 shader 或通过删除效果假装成功。

一次请求最多四个候选；图片异步准备可跨请求，已选档位会保留。降级档的图片准备使用有界同步代理路径，不依赖前端修改后补一个重绘回调。到最低档仍被拒绝，冷却 30 秒；未来渲染请求只探测最低档，不重启四档循环。工程编辑、资源集合或画布尺寸变化允许立即探测当前档位；改变时间本身不重置失败次数。

恢复质量同时要求 120 次成功和冷却期到达，每次只升一档；升档失败退回已工作的档位。策略不启动后台计时线程，探测由后续渲染请求触发。

## 保留已完成画面

`PreviewFrameTargets` 最多持有两个 RGBA8 目标，每个沿用 16 MP 上限；新候选成功编码、提交并通过即时健康检查后才成为 front。失败候选不会覆盖 front。交换链图像在候选成功编码后获取，避免拒绝重试反复占用未呈现图像。

最低档也不能满足时，可继续呈现已有 front 与宿主 overlay；`lastPresentedFrame` / `lastPresentedRevision` 保留实际画面的时间与版本。`render` 返回 true 表示呈现了画面，此时可能是保留帧；调用者用 `memoryAdaptation.held` 区分。如果没有已完成画面，返回 false 且在适配状态报告 blocked，不伪造成功帧。

替换工程使 front 失效，避免显示上一个工程。目标交换只重新绑定 presenter 的来源，不重建其渲染管线。记账包含 front 与候选的逻辑容量，不能当作实际驻留显存或统一整帧预算。

## 只读 API

`preview_info.memoryAdaptation`（version 1）包含

- `status`：normal、degraded、blocked
- `requested`：当前候选的 level、imageEdge、videoEdge、targetDivisor
- `actual`、`actualWidth`、`actualHeight`：最近已提交画面的档位和尺寸，尚无画面时为 null
- `requestedWidth` / `requestedHeight`：下一候选目标尺寸
- `failures`、`refusalCount`、`maxLevels`、`rejection`、`nextProbeInMs`
- `held` / `hasLastValidFrame`

原 `preview_info.width/height` 与 `imageResolution` 继续描述请求，保持修改模式后立即查询的契约；实际画面应读取上述 actual 字段。blocked 的建议调用帧率最多 15，正常目标帧率仍由既有耗时/热状态策略决定。

`previewVideoProxyBytes` 与显存台账里的 `cpuVideoPreviewProxyBytes` 单独报告 CPU 视频代理。每个活动实例只缓存一份，使用 Weak 解码帧身份，不额外保留原解码像素。YUV 在原生平面上缩小，保留旋转、色彩矩阵和范围并吸收裁剪色度相位；RGBA 直接缩小。此阶段使用最近邻，细节与时序画质需要后续代理滤波验收；原解码帧和正式上传接口保持不变。

## 验证与范围

Rust 回归覆盖四档耗尽、30 秒冷却、恢复失败回退、旧帧像素保留、双目标复用、正式输出拒绝、shader 错误隔离、72 组 YUV 布局/旋转/色彩元数据、取消后的同帧上传和代理引用释放。Android `PreviewMemoryBackendTest` 经真实 Surface 验证 9 张 2048 图的代理降级，以及大图层效果四档失败后保留旧帧；不操作编辑器 UI。

当前是 [#61](https://github.com/MAD-Producer/motion-studio/issues/61) 的预览策略阶段。来源纹理与各 scratch 仍采用原有分项额度；系统可用显存查询、可配置的统一整帧峰值、真实低端 Android 持续压力与更高质量代理尚需独立验收。本期不能保证任意工程都能在最低档渲染，也不把逻辑检查等同于驱动实际 OOM 处理。
