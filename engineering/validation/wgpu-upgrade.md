# 官方 wgpu / Naga 升级验收

日期：2026-10-17。基线为 `fix/video-rgba-plane-release`，不改动面板呈现。

## 变更与依赖来源

- 删除 `vendor/naga` 和 `[patch.crates-io]`，使用 crates.io 官方 wgpu 30.0.1、Naga 30.0.1。来源与许可证更新到 `ATTRIBUTION.md`，锁文件保留 registry 校验和。
- 桌面启用 Vulkan、DX12、Metal，关闭默认特性，不编译 GLES / WebGL 后端。
- 适配设备、窗口句柄、管线布局、错误作用域、映射、轮询和呈现 API。Android 仅调整共享接口的编译引用，不恢复其 CI 或 GLES 支持。
- `gpu-allocator` 对 windows crate 的宽版本范围需与 wgpu-hal 使用同一 COM 类型；锁定其传递依赖为官方 windows 0.62.2。其他依赖需要的 windows 0.54 保留，不修改上游源码。
- Naga 新版的 WGSL 模板参数扫描会将旧 shader 的 `select(a < b, c > d, condition)` 解释为歧义语法。在 Motion 的 shader 编译入口给 `select` 调用参数加括号，保持表达式语义。已发布的 `.msfx`、包身份和保存字节均不改写，不再维护 Naga 补丁。

## 报错与正常路径审计

| 编号 | 分类 | 处理与证据 |
| --- | --- | --- |
| GPU30-01 | 功能与错误传播 | 适配器请求保留 `RequestAdapterError` 原因链；无后端实例的真实请求失败由 `wgpu_upgrade` 测试验证。 |
| GPU30-02 | 功能与原有观察 | scoped shader 错误保持作用域，轮询失败也不会提前丢弃作用域中的首个错误；独立非法 shader 随后仍可正确渲染。实际 uncaptured error 的默认级别日志测试继续运行。 |
| GPU30-03 | 新观察 | timestamp 的 `get_mapped_range` 错误使用实例内有界计数，保留 slot、sequence 和原始原因，第 60 次 Error，成功映射后复位。真实未映射 buffer 触发 120 次失败、一次成功和第二轮 60 次失败；文件/stderr 各两条 Error，stdout 不含错误记录。测试捕获器采用 info 过滤；桌面正式日志 sink 的验证沿用桌面日志测试。 |
| GPU30-04 | 错误传播 | fallible `Device::poll` 向调用者保留错误；会话分离的 void 清理入口使用原有 logger 记录等待失败。设备丢失导致的等待错误尚未单独注入到分离入口。 |
| GPU30-05 | 正常分支 | 新的 `Occluded` surface 状态返回未呈现，复位连续失败计数；不作为错误记录。实际窗口遮挡/恢复需平台交互验证。 |
| GPU30-06 | 原有观察 | Lost/Outdated/Timeout 保持原有有界退化观察；Validation 返回错误，原始 GPU 细节由 uncaptured 回调保留。 |
| GPU30-07 | 保留与边界 | timestamp 异步 map callback 的旧错误状态及无有效时间戳分支不在此次新观察范围；不将它们标为已修复。 |

## 验证记录

产物日志保存在 `artifacts/workspace-stall/`（本地验收产物，不提交大文件）。

- `wgpu-upgrade-fetch.log`：`cargo fetch --locked` 通过。
- `wgpu-upgrade-check.log`：桌面 workspace all-targets、FFmpeg 特性检查通过；现有桌面警告仍保留。
- `wgpu-upgrade-effects.log`：效果库单元与集成测试通过，包括整库 WGSL/ES300 编译及已发布包身份不变。
- `wgpu-upgrade-timestamp-observation-final.log`：实际映射失败、阈值抑制和成功复位通过。
- `wgpu-upgrade-tests-final.log`：GPU / host / UI / FFmpeg 单元与集成测试 347 项通过，1 项已有外部素材测试跳过。测试尾部的文档测试因并行构建覆盖依赖产物出现 E0463，单独串行复跑并记录，不能将首次整条命令记为成功。
- `wgpu-upgrade-doctests.log`：串行复跑文档测试通过，host 文档用例 1 项通过，其余 3 个 crate 无文档用例。首次 E0463 已通过独立复跑排除。
- 桌面构建与 DX12 复核结果在完成后追加。

本地运行环境为 Windows；Linux / macOS 原生构建和交互依赖 CI，不将本地检查等同于跨平台验收。没有修改现有像素断言以适配新版依赖。机器上现有 Epic Vulkan overlay 注册项错误属于驱动加载环境日志，不能据此宣称运行时完全没有外部错误。
