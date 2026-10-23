# UI 库的显式 GPU 测试策略

Refs #135。本批只改变测试选择，不改变产品界面、绘制语义、资产、依赖版本或既有像素断言。

## 范围与命令

`motion-ui` 默认启用 `gpu-tests`，保持原来的 GPU 必测行为。没有 GPU 的本地环境可单独运行：

```sh
cargo test -p motion-ui --no-default-features --locked -- --test-threads=1
```

25 项实际需要适配器或设备的测试通过 `cfg_attr` 明确标为 ignored，测试报告给出启用方法；忽略不计为通过。全部测试仍会编译。缓冲区整数溢出、退化图标、非法路径、字形光栅化、字素边界和其余 CPU 用例继续执行。

```sh
cargo test -p motion-ui --locked -- --test-threads=1
cargo test -p motion-ui --no-default-features --features motion-ui/gpu-tests --locked -- --test-threads=1
```

这两种模式执行原 GPU 测试正文。适配器、设备、上传、验证或像素断言失败仍使测试失败，不捕获并吞掉错误，也不因找不到 GPU 自动返回成功。显式运行 `--ignored` 仍是标准 Rust 测试宿主行为。

原生桌面 CI 明确传入 `motion-ui/gpu-tests`，同时保留 `motion-host/diagnostics`；原工作流覆盖检查按逗号解析 feature 列表，同时要求两者。作业、平台、超时、测试线程和其他断言保持。

## 分类

| 项目 | 处置 |
| --- | --- |
| G1：25 项真实 GPU 测试 | 只添加条件 ignore 属性，所有测试正文与断言逐字保持；清单及前后摘要为 `ui-gpu-policy-implementation.json` |
| G2：同文件的 CPU 测试 | 保持执行，没有整文件或整个测试目标的条件编译 |
| G3：CI 必测 | 默认 GPU 行为保持，CI 另外明确启用；原 GPU 获取与失败分支不变 |
| G4：实际模式和失败对照 | CPU 模式 128 通过、25 明确忽略；默认 GPU 模式 153 通过、0 忽略。受控故障在显式 GPU 模式实际失败，恢复后同用例通过；不把忽略或预期失败算为通过 |
| G5：范围限制 | 仅 `motion-ui` 的独立本地命令；不声称整个工作区已经能无 GPU 运行，render、host、desktop 与其他平台的统一策略仍待完成 |
| G6：分支与输入 | 只检查其他分支路径元数据，没有审阅其他 PR。当前其他开放 PR 与这 13 个文件无重叠；历史工作区的既有改动保持。新策略在独立准备工作区完成后按字节核对并入 #440，准备副本恢复，未修改运行中工作区的源文件 |

Cargo 的默认 feature、`--no-default-features` 和 workspace package/feature 选择遵循 [Cargo 官方说明](https://doc.rust-lang.org/cargo/commands/cargo-test.html#feature-selection)。不要把上面的独立包命令扩写成“整个工作区无 GPU”：其他依赖可以重新启用默认 feature，其他包也仍有必需 GPU 的测试。

## 验收

`ui-gpu-policy-source-receipt.json` 冻结实际组合输入，两个模式间没有源文件变化。CPU 日志 SHA-256 为 `61e0d0aec09ffedb22c6a7507bcb531d1ce132ca81976115e80f65139ba6b84a`，GPU 日志为 `3c879e4a290c5f48ae94b99c580dc4535f21446793da27bab1a11682264240af`。两轮共享的 128 项 CPU 测试不重复计数，合计 153 项不同测试；不能相加成 281 项。

真实模式选择通过受控故障另行核验。仅在图标 GPU 测试起点临时添加一条 panic，保留全部既有断言：关闭 feature 时明确报告 1 ignored；使用 CI 相同的 `motion-ui/gpu-tests` 参数，即使同时给出 `--no-default-features`，也执行并以退出码 101 拒绝该故障。原始预期失败日志为 `ui-gpu-policy-control-forced-gpu-rejects.log`，SHA-256 `a1b646284013092e88495424dc9367587b68e84432661ffe581f4621b5f85475`。

全部控制进程结束后按字节恢复原测试，冻结输入摘要逐项一致；显式 feature 下重新运行实际 GPU 图标用例通过，日志 SHA-256 `a5b94af4d106cebbaa8536670cdfeea04bc0d72c5ab1bce15cd7238e12cf12f2`。这不是额外的新测试，也不是靠重跑掩盖失败；临时故障和恢复各自有独立收据，未进入提交。没有关闭物理 GPU，CPU 模式的证据是明确的测试选择和不执行 GPU 测试，不声称验证了所有无硬件平台。

同一最终组合的工作流原有 11 项检查通过。最终桌面七项错误信道、全目标 FFmpeg/diagnostics/gpu-tests 检查、FFmpeg 构建及实际命令列表启动通过，默认 worker 信道有独立原始输出。最终组合收据为 `host-ui-final-combination-results.json`、`host-ui-final-default-output/execution.json` 和 `host-ui-final-product/execution.json`；本批仍不计为整个工作区 GPU 策略完成，不关闭 #135。
