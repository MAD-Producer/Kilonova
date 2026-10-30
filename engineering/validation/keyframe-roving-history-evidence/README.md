# 历史预算证据

范围和剩余产品边界见[阶段说明](../keyframe-roving-history-progress.md)。原始失败输出、
最终成功输出和源码哈希均保留。没有新 UI、GPU 像素或视频编码验收；不得用这批核心
证据替代前端验收。core 单元日志的 injected panic 是原有回滚负对照，终态为通过。

```powershell
cargo test -p motion-model -p motion-core --lib spatial --jobs 1 --locked --offline -- --nocapture --test-threads=1
cargo test -p motion-core --lib --jobs 1 --locked --offline -- --nocapture --test-threads=1
cargo test -p motion-model --test spatial_hot_path --test spatial_track --test spatial_schedule -p motion-core --test spatial_keyframes --jobs 1 --locked --offline -- --nocapture --test-threads=1
```

Windows 需准备 MSVC 环境并使用可创建临时目录的 TEMP／TMP；本机复用
`E:/Dev/aem/.tools/review-564/target`。所有记录对应 manifest 中的源码基线与哈希。
预算按真实分配容量计量并保守包含 Arc 头，对齐及 allocator 元数据的边界见阶段说明，
不宣称限制进程总 RSS。实际测试验证预算两侧、共享身份、独立重建、回收、删除、
分叉、各合成和摄影机、手势、错误原子性及原有 JSON 计量。
