# 发布整数检查策略（#125 阶段交付）

开工核对开放 PR 的文件清单及近期本地分支：有理帧率、跟踪、原生预渲染均由其他分支推进；本轮只修改工作区 release profile、独立的算术策略回归及说明，不改这些分支。工作区基线为 main `6ab57829`。

## 改动

工作区 release 显式 `overflow-checks = true`、`strip = "none"`；原有行号调试信息、thin LTO 与单代码生成分区保留。移除只覆盖 `motion-effects` 的冗余特例。详见[策略说明](../release-profile.md)。

新增算术策略用例用运行时输入检查有符号、无符号的加减乘共六种溢出，正常算术仍保持原值。它必须在实际 release 下运行，不能用 debug 下通过代替；显式关闭最终测试 crate 的溢出检查作为负对照时应失败。没有修改既有断言。

## 当前验收状态

实际 release 下，算术策略 1 项、合成时间映射 3 项、图层时间映射 22 项、节点资源尺寸与总量 2 项，共 28 项不同测试通过。对已生成的 Cargo artifact 逐项检查，所选依赖与测试均开启整数检查；四个测试程序均为 opt-level=3、line-tables-only，并存在非空 PDB。行号配置和 PDB 存在不等于已验证发行包符号分发或跨平台符号化。

负对照只对算术策略的最终测试 crate 显式传入 `-C overflow-checks=off`，源码与依赖不变：用例以退出 101 报告 unsigned add silently wrapped。恢复正常 Cargo 调用后，28 项再次通过，重复运行不另计。负对照的编译命令、实际程序哈希及失败原文保留；没有修改产品 profile、断言或正常发布参数来通过测试。

首次正常 release 编译在第三方 `image` 的 rustc 中以 `STATUS_ACCESS_VIOLATION` 退出，未执行测试；其原始诊断与退出 101 保留。同一发布配置后续编译成功，没有证据证明这次编译器故障由本改动引起，也不宣称修复了编译器。

额外工作流路由测试初次因本机 Python 缺少 PyYAML 未能启动。后来定位到已有工具目录内的 PyYAML 6.0.3，通过命令级 PYTHONPATH 复用，14 项原回归通过；没有安装或修改依赖。`cargo fmt --all --check` 在 Windows 报 os error 206（参数过长），未产生可用的全仓格式差异清单；该运行不算格式验收通过。新增测试模块独立 rustfmt 和 git diff --check 通过。

效果解析器的正常 release 构建也完成：34 项库回归与 3 项循环工作量回归通过，沿用原断言。合计 65 项不同 Rust release 测试和 14 项工作流路由回归；恢复配置后的重复运行不另计。六个最终测试程序的 profile、SHA-256 和 PDB 存在性，以及原始失败记录见[证据清单](release-overflow-policy-evidence/manifest.json)与[原始记录](release-overflow-policy-evidence/evidence.zip)。

#595 的远端检查需以最终提交为准，本地结果不代替跨平台 CI。新增算术策略用例随普通模型测试运行，但现有 CI 的专用 release 调用仍只覆盖 effects；将模型策略用例接入持续 release 门禁仍是 #125 的后续工作，本轮没有修改其他活跃分支占用的工作流。

## 错误与范围

没有新增日志站点、观察计数器、panic 钩子、重试、线程重启或等待期限。现有输入的结构化 `Err` 与预算校验必须继续生效；本策略不能替代它们。没有 UI、素材、依赖或工程格式改动。#125 的全仓 fmt/clippy 与剩余不变量审计仍未完成，保持 issue 打开。
