# 桌面测试与构建分阶段验收

## 原始失败

#604 的 `5d56e61` 对应 run 41762984153，Windows job 126355283861。
完整桌面单测步骤成功，其中主程序测试为 1698 passed、0 failed、69 ignored；
随后 release 配置检查尚在编译依赖时，整个 job 达到原有 90 分钟上限而被取消。
GitHub annotation 明确记录 `The job has exceeded the maximum execution time of 1h30m0s`。
这不是通过重跑已知失败或放宽测试来处理的失败。

## 变更及边界

将原有桌面单测、Windows GPU 输出、各平台内存查询和相应失败证据上传移入
`desktop-tests`；原 `desktop` 保留 release 配置检查、FFmpeg、打包和原生发行验证，
依赖整个 `desktop-tests` 矩阵成功。原 Desktop build 检查名称和发布依赖保持。
两个阶段均沿用原平台矩阵、环境、90/60 分钟时限、串行测试参数及全部特性。
失败或取消的测试不允许构建、发布通过。没有修改产品、Rust 断言或正常缓存语义。

工作流策略测试只调整已搬迁步骤的所属 job 和其独立上传名称；原有命令、特性、
产物目录与条件断言保留。新增测试检查阶段依赖、无失败忽略、矩阵及预算。
原所有桌面 step 字典与两个阶段的合集逐项比较，未丢失任何步骤；其他 jobs 完全相同。

代价：重复 runner 初始化、恢复缓存，并且构建需等待整个测试矩阵。
不声称总运行时间更短；新的远程运行仍须验证各阶段能在时限内完成。
测试产物在其产生的 runner 独立上传，不依赖跨 job 本地目录共享。

## 本地证据

工作流策略 15 项通过；发布打包及工具回归见附带原始日志。
actionlint 退出 0。五种内存中变异均被拒绝：移除测试依赖、always 绕过失败、
忽略测试错误、平台矩阵不同、延长预算。没有据此声称执行了远程 GPU 测试。
原失败 run 保留在 GitHub；本地完整下载日志位于
`artifacts/issue-goal-current/node-input-ci-current/windows-full.log`。

可移植本地结果、逐项保留证明及 SHA-256 在
`desktop-ci-stages-evidence/evidence.zip` 与 `manifest.json`。

此次只改 CI，不新增产品错误处理或 UI 样式；DESIGN §0.7 面板整改项均按原状保留。
#134 的原生物理输入、通用布局求解、整帧耗时与内存峰值等剩余验收不因此完成。
