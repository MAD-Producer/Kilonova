# 工程切换失败的目录回滚

日期：2026-10-17。关联 #132 的存储子项；基于已合入的官方 wgpu/Naga 升级继续开发，不包含 #317 的 FFmpeg 分发改动，不修改可见 UI。

## 行为与所有权

新建工程在创建 `project-*` 后，保存或 `replace_project` 失败会残留目录；导入在解包发布 `import-*` 后，Engine 创建、保存旧工程或替换失败也会残留目录。现在只为本次成功创建/导入的目录登记所有权，完整切换成功后立即解除登记，失败则回收。已有目录创建冲突不会取得所有权，正常缺失无需报告为故障。

回收守卫位于 `with_session` 外，普通失败与 unwind 都在会话注册表锁释放后处理文件系统。删除前核对解析后的绝对目录仍是所记录规范工程库的原名直接子目录；拒绝符号链接/junction 改向或越界，保留被替换为普通文件的内容。没有遍历工程库清理历史目录，没有后台重试或线程重启。并发敌意进程在校验与删除之间替换路径的竞态不属于这里已证明的保证。

## 错误审计

| 编号 | 分类 | 证据与边界 |
| --- | --- | --- |
| PSR-01 | 功能修复 | 实际桌面 engine worker 接收模型有效但嵌入 runtime archive 损坏的工程；两次新建、两次导入均由真实 `replace_project` 拒绝，旧工程模型/revision/history/磁盘内容不变，没有新建、导入或 staging 残留。成功新建后的目录保留，随后失败导入也不会删除它。 |
| PSR-02 | 原有错误信道 | 上述五次真实失败经原有 respond 与生产日志 sink 返回；默认 info 的文件和 Error stderr 均保留原始 archive encoding 原因、request、operation 和耗时。Info start 在对应错误前；stdout 不含日志，工程私有名称不进入日志。没有在业务锁内新增 logger 调用。 |
| PSR-03 | 新错误组合 | 普通回滚失败保留主要错误，并追加 `phase=project_cleanup`、目录和清理原因。实际用普通文件替换目标使 `remove_dir_all` 失败，断言主要错误在前、文件字节保留；普通返回沿用 PSR-02 的信道。未向真实 engine 注入这个目标替换竞态。 |
| PSR-04 | 新 unwind 观察 | 正常清理后原始 panic payload 不变。另一个独立子进程制造真实清理 IO 失败，info 过滤的测试 sink 验证一条 Error 写入文件和原始 stderr、stdout 隔离，并在记录时验证业务 mutex 已释放。该 sink 为测试捕获器；生产 sink 的普通错误输出由 PSR-02 实测，未将自定义捕获器称作生产 logger。 |
| PSR-05 | 正常/既有路径 | 成功切换立即解除目录所有权；未创建目录的失败、已移除目录不是回收故障。旧工程保存策略、会话采样、命令回复、取消和 Present 热路径不变。 |
| PSR-06 | 待验证/范围外 | Unix 符号链接替换用例已添加，Windows 本地不能执行。Windows junction 特例尚未单独执行；目录 fsync/断电持久性仍未完成，现有临时文件清理守卫与版本上限守卫不重复实现。#132 保持打开。 |

## 验证

Windows portable MSVC；使用独立 `artifacts/workspace-stall/wgpu-upgrade-target`，关闭 debug information 和 incremental，最多两个编译任务。

- `project-storage-host-tests-observation.log`：5 项新宿主清理用例通过，含实际 IO 失败、越界保护、成功提交、panic 及清理日志。
- `project-storage-desktop-tests-final.log`：1 项新桌面子进程测试通过，内部通过真实 worker 执行五次失败切换和一次成功新建，并验证生产文件/stderr/stdout 信道。
- `project-storage-regression.log`：core 单元 15 项通过、1 项已有手动性能测试忽略；存储集成 2 项通过；host 全部 31 项通过（已含上述 5 项）。合计与新桌面用例共 49 个不同测试通过，1 项已有忽略，不重复累加专项复跑。
- 首轮桌面测试的 fixture 未声明 node source 要求的格式 17，先于替换阶段被模型拒绝；已修正 fixture 格式，保留原断言，首次失败不记为成功。日志 `project-storage-desktop-tests.log`。
- 首轮新增 unwind 捕获器用了受 libtest 捕获的 `eprintln!`，stderr 断言失败；修正捕获器为原始 stderr 写入后通过，没有减弱断言。日志 `project-storage-host-tests-final.log` 与成功复跑日志分别保留。

不新增依赖、第三方资产或工程格式。现有桌面警告保留，新增的测试专用 Path import 已移入测试模块，未保留新的非测试 unused import。跨平台 CI 和真实窗口/媒体替换的组合压力测试不能由本次无窗口 worker 回归代替。
