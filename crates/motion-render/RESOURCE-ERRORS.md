# 资源拒绝的后端契约

`RenderError::resource_rejection()` 返回实际检查产生的结构化拒绝，普通输入错误、图像解码、IO、shader 校验与 GPU 错误不会按文字自动归类。

- `ResourceRejection::Budget`：`scope`、`required_bytes`、`budget_bytes`
- `ResourceRejection::TextureDimensions`：`width`、`height`、`max_dimension`

序列化使用 `kind` 区分类型，字段为 camelCase。`scope` 对应已有检查的记账范围；它不是物理显存，也不表示各范围互斥或可直接相加。

本期覆盖来源纹理与资源贴图、活动图片预检查、视频平面与转换输出、嵌套合成、效果输入快照、蒙版输出、矢量源与 MSAA、track matte 和混合 accumulator 的既有预算拒绝。效果计划也记录 scratch、mask 与 effect 合计、矢量源、mask 输出、调整图层 accumulator 及已检查的纹理维度拒绝。

`PlanBuilder::build_checked` / `build_preview_checked` 返回 `RenderError`；原 `build` / `build_preview` 的字符串接口仍可使用。`last_resource_rejection` 只描述当前失败构建的原因；成功、预览缓存命中、注册表替换和已转为诊断的失败不会遗留错误标签。既有 `last_scratch_request` 仍可观察被拒绝、随后旁路的候选池。

设备资源图片的预算或尺寸不足不再写入 `program_errors` 当作 shader 损坏。恢复资源空间后，同一时间点可重新准备效果。真正的 shader 管线失败仍走原有失效程序与预览诊断路径。

显示文字、预算额度、像素运算和工程格式保持原有契约。嵌套子帧添加诊断上下文时保留资源拒绝类型。

此提交是 [#61](https://github.com/Yukk1o/motion-studio/issues/61) 的前置步骤，没有实现自动重试、质量降级、统一整帧峰值额度或系统显存查询。宿主当前仍把许多结果转成字符串；后续需要在转换前保存拒绝信息，并在只读预览 API 返回实际质量和原因。CPU 图片解码器与其他仅返回字符串的输入路径不能由此宣称全部已类型化；既有非严格效果计划仍可能把部分资源拒绝转为预览诊断，也需要在降级策略接入时明确处理。

复现

```sh
cargo test -p motion-render --locked --test resource_rejections -- --test-threads=1
cargo test -p motion-render --locked --lib resource_rejection_tests -- --test-threads=1
```

测试用实际效果计划区分 scratch 字节预算与设备尺寸，验证拒绝后的普通错误、成功与缓存命中不复用旧标签；实际 GPU 图片预检查在上传前拒绝大工作集，再成功准备较小代理，包含“预算”字样的元数据错误仍保留普通输入类型；GPU 效果测试仅注入逻辑计数模拟压力，避免为故障测试分配 128 MiB，释放压力后的截图与新 renderer 的结果一致；嵌套子帧拒绝添加合成上下文后仍可读取原类型并重试

## 矢量与生成器一致性

非分组矢量同尺寸共享的 raster scratch 是 4×MSAA RGBA8 的 16 字节/像素加 resolve RGBA8 的 4 字节/像素，共 20。每图层的 RGBA8 vector source 另计 4 字节/像素；不能再将 scratch 改为 24，否则会重复计算。测试从实际纹理台账读取尺寸、格式、采样数、身份和容量，覆盖分组/非分组及共享/不同尺寸。

`VectorWorkingSet` 在任何矢量上传和缓存命中分支之前检查完整候选工作集，包括已驻留来源、视频平面和插件资源。下调策略后，缓存命中也必须拒绝超预算工作集；拒绝不替换矢量纹理或覆盖已呈现的目标。测试核验实际额度刚好成功、少 1 字节拒绝及恢复额度重试。

蒙版和矢量三角形输出逐次发射前检查剩余额度，同时约束原始顶点/三角形缓冲与裁剪后的可见输出。缓存命中也消耗帧额度。该约束覆盖输出缓冲，不等于 Lyon 所有内部临时数据或整个进程内存均被同一额度限制。最大 expansion 和最大节点数按实际几何判断；共线且低复杂度的合法图形不会仅因参数乘积大就被拒绝。

场景样本标记屏幕空间生成器，渲染、拾取和轮廓复用同一平面尺寸与矩阵约定。逻辑 `three_d`、节点/图层矩阵和发射器姿态仍保留。生成器作为 3D run 的合成边界；关闭效果或包装器 opacity 为零时恢复原有几何。GPU 回归用实际非透明生成像素验证 UV，并验证重复采样的像素一致。

```sh
cargo test -p motion-render --locked --test vector_budget_alignment -- --test-threads=1
cargo test -p motion-render --locked --lib -- --test-threads=1
```
