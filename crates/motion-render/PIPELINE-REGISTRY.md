# 注册表更新与已预热管线

PNG 捕获会把会话的效果注册表传给 renderer。原路径每次都重置解析缓存、程序 ID、预览计划、原生管线和资源纹理，导致已经预热的相同效果再次进入冷创建路径。

`set_effect_registry` 现在复用已有注册表身份比较：有序包键、`Arc` 对象身份和禁用集合全部相同，保留上述缓存，只刷新注册表中的非渲染诊断。

不同 `Arc` 即使版本与 hash 相同也会刷新；这是保守处理重新加载的包对象，避免仅凭可写元数据判断内容相同。新增 / 删除包或禁用状态变化也沿用原重置路径。

`Renderer::effect_pipeline_builds()` 和 `preview.effectPipelineBuilds` 记录 renderer 生命周期内成功创建的原生效果管线数。它不包含初始化时的基础合成管线，也不代表编译耗时。

```sh
cargo test -p motion-render --locked --test registry_pipeline_cache --test effect_plan_cache -- --test-threads=1
```

GPU 回归先预热效果，再重复设置相同注册表并截图，确认像素不变、管线创建数不增长；随后重新加载同 hash 包、禁用和恢复效果，确认必要的刷新与输出变化仍发生。

本期处理 [#64](https://github.com/Yukk1o/motion-studio/issues/64) 中的重复冷创建。工程未来片段的预热策略、异步编译、驱动管线落盘缓存及真实设备耗时对照尚未完成，保留该 issue 开放。
