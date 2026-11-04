# 节点优化的发行构建与双语原生验收

源提交为本地 `2882de24f5f90b79fe7407f900761ceedb8a1a47`，包含数值、连线／预览及取消恢复三批尚未发布增量。本记录不将其作为 main 已交付内容，也不将这些结果转给其他源码。

## 实际运行

使用仓库正式 release 配置、锁定依赖和 FFmpeg，未覆盖优化级别、LTO、codegen units 或 release 调试信息。完整构建成功，耗时 34 分 34 秒；保留实际编译警告，不声称零警告。程序 SHA-256：`63e70c7507c6a85f0fbf8a8fc208cf6abc5e096ad7f4ca9007732edb1e31a365`。

原样提取当前 `tools/ci_desktop.py` 的节点验收段。该文件与已发布 `20f33101` 对应文件相同，验收段 SHA-256 为 `c6a44ed5277cb8bb602beef830ed3479a6aa5215fce3dc35b91623c52e06d6cb`。期限保持 125 秒，全部断言保留；独立可写 TEMP 和 LOCALAPPDATA，默认 MOTION_LOG=info，运行时未覆盖 RUST_MIN_STACK。

| 语言 | 结果 | 完整验收耗时 | stdout / stderr |
| --- | --- | --- | --- |
| 英文 | exit 0，原验收段完成 | 24.985 秒 | 均为空 |
| 中文 | exit 0，原验收段完成 | 26.359 秒 | 均为空 |

实际检查包含主窗和原生浮窗的节点搜索、命名、视图菜单、默认值编辑、连线、诊断抽屉、分组、预览像素变化、撤销及保存重开。完整检查项目以归档的原验收代码为准；这是由应用内部驱动的真实窗口和 GPU 截图检查，**不是操作系统物理鼠标／键盘验收，不是 GPU timestamp 测量或逐项性能基准**。本批没有注入错误来验证 Error stderr，成功时 stderr 为空不能证明失败信道。

两次执行后保留 438 个最终 PNG，manifest 记录其大小与哈希。证据包仅保留中英文主窗／浮窗的概览及菜单共 8 张代表截图；部分共用纹理文件名可能在第二种语言运行时覆盖，因此不把最终文件清单称为两套独立截图档案。每种语言的原验收都在下一种运行前完成。

原始报告、命令、日志、编译配置、警告、图像及哈希见 [manifest](node-release-native-evidence/manifest.json) 和 [证据包](node-release-native-evidence/evidence.zip)。完整原始 PNG 仍在本机 `E:/Dev/aem/artifacts/issue-goal-current/node-restore-geometry/release-native`。

## 合入核验与边界

#602 与 #603 已通过 #603 整组合入 main `77f6145256a6acc0aebe7d29ab4a645322621771`。已核实 `20f33101` 包含父 `2f267650`，且整个 head 五项必需 CI 均成功；合入前主线新增文件变化与本组无重叠，合入后 41 个自身文件 Git blob 全部一致。GitHub 整树只读查询在合并成功后网络超时，改为 24 个目录查询完成验证，没有重放合并操作。主线 run 41635401270 的组合验收尚未完成。

此前 `20f33101` 和父 `c14f20f5` 的 dev 程序在原 125 秒验收中超时，这些失败保留。本次代码与构建配置都不同，不能将耗时改善全部归因于 release 配置，也不能将旧失败改写为通过。

#134 继续开放：结构／端口变化和子图切换等仍可能重建卡片，通用同帧求解上限、物理输入、整帧耗时／峰值及最新 main 完整集成仍待完成。DESIGN §0.7 的 12 项处置沿用 [视口处置表](node-viewport-geometry.md)，本批 retained with reason：未编辑属性行；§12 的 Windows DPI、物理输入和全窗口视觉验收仍 pending verification，抽查代表截图不替代完整清单。

本轮额外检查了本地注册工作树，4 处旧节点草稿有潜在路径重叠，2 个注册目录已不存在，均保留未改动；不以旧时间戳推定其他工作已停止。首次打包因 receipt 与 manifest 的 records 键重名失败，改为保留 native_runs 与归档 records，原错误随包保留；没有重跑程序或修改验收断言来修复打包问题。
