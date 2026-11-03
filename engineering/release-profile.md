# 发布构建的整数与诊断策略

工作区的 `[profile.release]` 显式开启 `overflow-checks = true`。普通整数加减乘法在发布版与开发版均检查溢出，不能依赖 release 默认的回绕行为绕过尺寸、时间、索引或资源预算约束。原来仅对 `motion-effects` 的配置收敛到工作区；该 crate 的行为不变。没有新增 crate 特例。

这项检查是缺陷暴露的最后一道保护，不是外部输入验证。可失败的解析、ID 分配、资源尺寸和预算运算仍应使用受检运算与现有结构化 `Err`；确实需要模运算的算法应显式使用 `wrapping_*`。不要捕获并忽略溢出 panic，也不要通过关掉检查、扩大预算或重试掩盖问题。panic 仍由现有应用钩子记录；本次没有改变日志、错误回复或进程恢复协议。

保留已有 `debug = "line-tables-only"`，并显式选择 `strip = "none"`。发布构建保留编译器生成的行号诊断信息，不为减小文件体积自动剥离它；现有 thin LTO 与单代码生成分区不变。Cargo 不剥离信息并不保证每个平台的分离符号文件已经随安装包分发，符号归档与崩溃符号化需要单独验证。

桌面发布、库与工作区中的 Rust 依赖继承该 profile；显式的调用方或环境配置仍可能覆盖 Cargo 设置。本地验收应记录实际产物的 Cargo profile，而不是只检查 TOML 文本。Android 的停止维护范围不变，见[桌面维护范围](desktop-only-ci.md)。

本策略对应 #125 的 release 配置项；全仓 fmt/clippy 门禁和剩余不变量审计另行推进，不能据此关闭整个 issue。验证记录见[发布整数边界验收](validation/release-overflow-policy.md)。
