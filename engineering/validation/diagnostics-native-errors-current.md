# 诊断报告导出的原生错误

关联 #138，与查看器错误修复合批。稳定源码编译和本地 19 项不同测试均已通过；
最终提交的远端 CI 待执行，不关闭 #138。

## 范围与归属

开工核对开放 PR 及所有工作树的 `diagnostics.rs`、诊断子目录、`main.rs` 未提交状态。
仅六个旧工作树修改了 main，均保持原样；没有其他活跃的诊断错误信道修复。
实现位于独立的 `fix/viewer-capture-errors`，未修改正在执行 #619 全量回归的工作树。

复用桌面已有的可克隆 `engine::Error`，使 reporter 的 export/checkpoint 回复保留
共享 Host 原因。文件系统操作保留 `std::io::Error`；JSON 编码保留带原阶段文字的
`serde_json::Error`。最终 UI 提示才调用 Display；既有队列繁忙/停止文案不变。
无依赖、资产、排版、预算、重试或线程重启变化。

| ID | 分类 | 验收与边界 |
| --- | --- | --- |
| D-IO | 原生原因传播修复 | 创建父目录、写入/同步/重命名的 IO 不再转成 String；真实创建父目录拒绝为 AlreadyExists / OS 183，重命名拒绝为 PermissionDenied / OS 5。未冒称磁盘写满或 fsync 硬件失败已注入。 |
| D-JSON | 原生原因传播修复 | 编码错误以既有 Host Context→Json 保存；当前 report 由 Value 构成，未构造不可达的编码失败。 |
| D-REPLY | 信道类型修复 | 实际 IO 回复保留 OS code 和 Error source；clone 共享同一 Host 原因，同文案普通文字不具备原生类型，已执行通过。 |
| D-OBS | 沿用既有观察，补实际检查 | 默认 Info start 在真实 reporter 进入受控阻塞前可见；两个实际错误分别进入文件和 stderr，stdout 隔离，已执行通过。没有新增热路径日志。 |
| D-RECOVER | 功能保持 | 原文件/目录、临时文件清理、记录内容、错误计数、健康导出与 checkpoint 均验证通过；没有增加自动重试。 |
| D-TEXT | 明确保留的边界 | 上传端点未配置、队列繁忙/停止及报告字节上限为自身验证消息；Sentry 发送语义不变。最终 UI Display 是显示边界。 |

唯一既有测试适配是把报告上限的错误显式 Display 后继续执行原 `contains("byte budget")`
断言，预期、期限、忽略和字节上限均未改。新用例有独立子进程看门狗；没有借此调整
既有测试或生产等待。DESIGN §0.7 的视觉整改项全部保留，本批不声称视觉整改验收。

## 本地验收

带 FFmpeg 的桌面测试编译退出 0（284.703 秒）。源码清单在构建、全部执行阶段前后
保持一致，可执行文件 SHA-256 在整个验证前后保持一致。

- 完整 `diagnostics::` 模块 15 passed、0 failed、0 ignored，包含新增真实 reporter
  用例及原有报告预算、争用、默认日志、Sentry 同意/频率和关闭覆盖。
- 同一最终二进制的查看器原生故障、既有截图上下文、精度截图和多视图检查，各
  1 passed、0 failed、0 ignored，共 4 项；不重复累加前一证据包的同名通过阶段。

原有 `native_test_evidence` 锁中毒用例会输出其受控 panic，测试自身通过；原始 stderr
保留，不将它删掉或误报为此次新增的文件系统故障。两次真实文件系统拒绝的默认
信道独立位于 `native-child/`，只据该子进程核对两条 Error，不声称整个模块 stderr 为空。

证据包 `diagnostics-native-errors-evidence.zip` 保留稳定源码和原始执行记录、实际子进程
日志/原生 OS 分类、恢复报告、最终查看器 PNG、完整测试名称清单及前后源码。
远端当前提交 CI、JSON 不可达编码失败及未注入的硬件故障保持上述边界；#138 其他
提供方继续开放。

证据包 54 个成员、1,834,405 字节，已逐成员回读，SHA-256：
`2c7155afe7d48ed0360a244c77198e16282902cab61d43ecbc5ceaed3c9fd22b`。
