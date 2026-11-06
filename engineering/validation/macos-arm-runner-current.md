# 桌面 ARM runner 迁移验收

关联 #125；这份修复不宣称完成整个桌面发布议题。

## 故障证据

主线 `72e8ef41bff397f8580bbd185861eb2860e56ff6` 的
[run 42151181603 / job 127674530027](https://github.com/MAD-Producer/Kilonova/actions/runs/42151181603/job/127674530027)
在分配 runner 前以 cancelled 结束，runner_name 为空且没有执行步骤。
检查注解的原始原因是 `The configuration 'macos-14-arm64-us-default' is not supported`。
这是失效的 runner 配置，不是测试通过，也不是常规的新提交取消旧任务。

[GitHub 官方退役公告](https://github.blog/changelog/2026-10-01-github-actions-macos-14-runner-image-retirement/)
规定 macOS 14 于 2026-11-02 退役，并列出 `macos-15` 为可用 ARM 替代标签。

## 变更与边界

只把桌面测试、桌面构建矩阵中的两个 ARM runner 标签从 `macos-14` 改为
`macos-15`。平台名、aarch64 编译目标、二进制名、60 分钟预算、依赖准备、
原生测试、构建与发布顺序、触发条件和权限均保留。已有预算测试的数据行同步
改为实际使用的 runner；预算值与断言保持不变。

开始前核对了 open PR 和本地工作树；没有并行的 ARM runner 修复。
两份旧工作树中的未提交工作流改动保持原样。
主线旧的 FFmpeg tracking 测试帧率类型错误已经由 #606 修复，本 PR 不重复修改。

## 本地验证

- 使用 CI 已固定的 PyYAML 6.0.3，从已有独立工具目录加载，没有添加项目依赖。
  初次运行因本地 Python 未加载该目录而报 `ModuleNotFoundError: yaml`，没有执行测试；
  设置进程级 PYTHONPATH 后才运行下述验证。
- 已有 `tools/test_ci_workflow.py` 全部 15 项通过。
- 逐项求值两个作业在 pull_request / push / workflow_dispatch 下的六种矩阵，
  验证只有非 PR 的 ARM runner 标签改变。
- 把两个标签还原后，完整解析工作流与修改前相等；没有改变其他门禁、权限、命令或预算。
- 验证前后跟踪源码散列相同。稀疏工作树中的成员 Cargo.toml 从当前提交原样读出，
  供现有工作区策略测试检查；没有修改清单。

原始 job、check、注解、测试输出、源码散列、六种矩阵比较和验证脚本见
[证据归档](data/macos-arm-runner-v1.zip)。归档含 12 个成员，23755 bytes，
SHA-256 `4717ea0395fcf39fe3933d47f7d2531899c0b9d28254bed8b79ed2475505b0a3`；
归档已回读并逐项核对散列。

## 尚待远端验证

PR 路径仍只运行 Linux、Windows、macOS Intel 三个平台；本地策略测试不能替代
ARM 实机执行。合并后的 main 必须实际分配 macos-15 ARM runner 并完成测试、
构建与 FFmpeg 发布链，才能宣称本次发布阻断已经得到端到端验证。
当前记录不把排队、cancelled、skipped 或未执行的作业算作成功。
