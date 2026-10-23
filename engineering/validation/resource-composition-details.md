# 图像与矢量资源的原生合成错误详情

关联 #138、#135。基线 `0fbde349e27dd302ae0cfb748f32743f79e3640a`，分支 `fix/resource-composition-error-details`。

## 功能边界

宿主已经能从直接 Model、Media、Frozen 和 Render 包装中读取 `CompositionError`，但 `ImageResource::Model` / `Shared` 和 `RenderError::VectorGeometry(Model)` 尚未遍历。它们保留了真实错误对象，却使类型化回复的 `error_detail` 变成 null。

本批增加这两条遍历。图像 Shared 逐层借用原对象，不复制、解析或更改错误文字；渲染的 Context、Diagnostic、Shared、Encoding 继续沿原有路径访问原因。只认原生 Composition 变体，不从显示文字猜测类型。清理失败仍以 primary 为准，取消和其他普通分支的处置不变。

这是类型化 API 回复的功能修正，没有新增日志观察、故障注入、重试、线程重启或可见 UI 样式，也没有改变像素、预算、期限、依赖或素材。桌面 worker 的 String 回复和旧前缀适配器仍待迁移；本批不宣称关闭 #138。

## 验证与处置

| 编号 | 分类与证据 |
| --- | --- |
| R1 | 新回归：实际 Project 缺失合成查询产生 Model 原因，在公开图像/矢量错误构造器和标准 envelope/dispatch 边界检查详情。包装图由测试构造，不冒充真实解码器或 GPU 故障路径 |
| R2 | 新回归：图像 Shared 反复借用 60 次仍为相同 detail 地址，强引用数量不增长；消费 dispatch 后释放包装，最终原对象释放。这不是日志计数阈值测试 |
| R3 | 新回归：外观相同的 Invalid 文本、Shared 文本以及只有 secondary 含合成错误时均不能产生 primary 合成详情 |
| R4 | 原行为负面对照：最终四项测试在修改生产代码前真实执行，1 通过、3 失败、0 忽略，覆盖图像直接/共享和矢量包装；原始日志 SHA-256 `9ed7d35e5be6be1c285837b7b8264d90cd239192943f9e4b24010d752b4b4e8f`。首次三项布局结果另外保留，没有修改既有断言以通过验收 |
| R5 | 既有观察：实际 Session/Engine 的默认 Info start、Error 文件与 stderr、stdout 隔离和恢复已由 #440 验证；本批不将其记为新增观察，也不将本批构造包装测试当作端到端 worker 证明 |
| R6 | 归属：开工核对 102 个当前布局工作树，两个源文件没有未提交占用。视频 worker 文件另有分支修改，本批避开该文件；没有审阅或改动其他 PR |
| R7 | 本地通过：完整宿主 143 项通过、0 失败、0 忽略，包含新增四项；原始日志 SHA-256 `79624400dea013ff258a78cc7a4a674287fb89335bd75934ca9c79882cf77e9e`。专项、循环与重复执行不另计。最终 PR/主线 CI 仍待验证 |

只复用现有共享 Cargo target；不新增构建缓存目录。#132 的保存中断测试由独立 #445 承载，真正掉电边界未因本批扩大。

全工作区全目标 FFmpeg/diagnostics/gpu-tests 检查通过（日志 SHA-256 `453925a54cb6aa546494e1f486e49458a73fe45cdac88830bba9b4a9767e16ce`），FFmpeg 桌面构建通过（`1ffc7c6a14c9d28fafa06ea415af8e12131f024224759363f4fba5bbf8f00f75`）。实际新程序 `--command-list` 返回 24,475 字节 JSON、stderr 为空、退出 0；可执行文件 SHA-256 `a3c052d23f2a4252f218eb3f502600ce08370a0d41bb11d040efe6342bfa604d`。编译和启动前后冻结源码一致，没有声称启动校验覆盖可见 GUI 或完整媒体播放。

保留既有 render/graph 和桌面编译警告；桌面构建仍报 19 项警告，不称零警告。没有放宽断言、预算、期限或忽略规则。跨平台和合入后的实际组合由各自 CI 验证，不能借用 #440 旧输入的结果。
