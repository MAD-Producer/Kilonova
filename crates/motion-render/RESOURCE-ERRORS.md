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
