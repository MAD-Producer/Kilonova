# 编辑器依赖边界的错误对照

关联 #135 第一项及其“人为在 motion-core 中加入 wgpu 时边界测试失败”的验收要求。基于 #389 最终源码 `63869ac1`；该提交已由 #389 四项必需 CI 验证并合入 main `26b834f7`。

主线已有完整 metadata 图、UI/desktop 检查、media→render 禁令及带上下文的失败信息。本次没有重写边界算法，也不把既有修复当成新实现；只补先前缺失的真实错误对照证据。

## 实际操作与结果

在本任务独立工作树中，先按字节保存 `crates/motion-core/Cargo.toml` 和 `Cargo.lock`，临时向 core 的生产 dependencies 增加 `wgpu.workspace = true`。复用仓库已声明及已归属的官方 wgpu 30.0.1，不增加新的第三方包或素材。

先运行 `cargo metadata --offline --format-version 1` 更新受控锁文件，并独立确认返回的 core package 确有 kind=null 的直接 wgpu 依赖；避免让 --locked 的锁文件拒绝冒充架构断言失败。随后执行：

```powershell
cargo test -p motion-model --test dependency_boundaries --locked --offline -j1 -- --test-threads=1
```

同一现有测试以 cargo exit 101 失败，明确断言 **`core depends on wgpu`**；测试执行 0.69 秒。日志 `artifacts/pr-ci-followups/dependency-boundary-wgpu-control.log`，metadata stderr 为 `dependency-boundary-control-metadata-stderr.log`。这不是伪造 JSON 或在测试里直接构造预期错误。

脚本在 finally 中逐字节恢复 manifest 与 lock，恢复后用相同命令执行现有测试：**1 项通过、0 失败、0 忽略**，0.70 秒，日志 `dependency-boundary-wgpu-restored.log`。结构化摘要为 `dependency-boundary-wgpu-control.json`。最终两个文件的 git diff 为空，不提交故意增加的依赖或锁文件改动，没有修改断言、返回值或失败退出码。

Windows portable MSVC；固定离线依赖，单编译任务及单测试线程。使用同一工作树和 `target/host-error-contract-verified` 缓存，完成前不并行改动源码。

## 完成边界

本项补齐依赖门禁的一个实际故障对照。与 [原生窗口输入验收](native-window-input.md) 的 15 项不同测试合计 **16 项通过**；负向对照和专项复跑不重复累加。这里没有新增产品日志观察、运行时功能、UI 样式、资源或依赖，不以这个静态测试声称默认错误信道或整个 #135 均完成。DESIGN §0.7 十二项既有基线全部 retained，原因是没有面板和可见 UI 改动。
