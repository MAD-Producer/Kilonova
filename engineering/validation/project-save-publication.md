# 工程保存的目录持久化确认

日期：2026-10-17。关联 #132 的存储子项，堆叠于 #318 的工程切换目录回滚。不修改用户可见 UI，不新增依赖、第三方资产或工程格式。

## 行为与边界

保留现有的模型/资源校验、临时文件写入、文件 `sync_all` 和临时文件清理。在替换前只解析一次工程根目录；Unix 提前打开该目录，替换成功后对目录执行 `sync_all`。Windows 使用同目录 `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`；不启用跨卷复制、重启延迟替换或失败重试。规范路径和 UTF-16 调用支持 Unicode 与扩展长度路径。

原始 IO 错误作为 `Error::Storage` 的 source 保留，显示阶段、路径及 `published`。替换失败时为 `published=false`；Unix 替换已成功但目录同步失败时返回 `published=true`，不会假称保存未发生，也不尝试撤回已发布内容。库层返回错误，沿用宿主命令回复和桌面生产日志信道，没有新增锁内 logger 调用。

这确认的是工程 JSON 文件的发布，不承诺新建祖先目录、素材文件、网络文件系统或硬件缓存的断电保证。Windows write-through 请求与 Unix 目录同步的语义以平台和文件系统为准；进程退出/杀进程不能代替真实断电模拟。

来源：[fsync 的目录项要求](https://man7.org/linux/man-pages/man2/fsync.2.html)、[Microsoft MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)。实现独立编写，没有复制第三方实现。

## 错误审计

| 编号 | 分类 | 证据与限制 |
| --- | --- | --- |
| PSP-01 | 功能修复 | Unix 在 rename 后同步预先打开的目录；Windows 在文件同步后请求 write-through 替换。现有格式、保存校验与资产路径规则保留。Windows 两次保存/重开、Unicode 目录和超过 260 字符的规范路径用例通过。 |
| PSP-02 | 新错误上下文 | 用已有目标目录制造真实系统替换失败，断言 `phase=publish`、`published=false`、规范目标路径、原始 OS 错误及 Error source 链；目标内文件保留，临时文件回收。没有用伪造 Err 代替系统调用失败。 |
| PSP-03 | 既有信道实测 | 真正启动桌面 Engine worker，以目标目录替换工程 JSON 后发出 save 请求；默认 info 的文件与 Error stderr 保留同一完整回复、原始系统原因、request、operation、耗时，Info start 位于错误前，stdout 无日志。工程状态/历史不变，worker 可继续接收显式请求；修复测试目标后下一次 save 成功。 |
| PSP-04 | Windows 文件锁边界 | 打开旧 JSON 时不授予 FILE_SHARE_DELETE，真实替换被系统拒绝；旧字节保持、临时文件回收。释放测试句柄后下一次显式 save 成功；产品没有自动重试。 |
| PSP-05 | 原有/正常路径 | 已发布临时文件不存在是正常状态。原有临时文件 Drop 清理与已有包清理不在此重写。现有 cleanup helper 的 Warn/锁内调用风险属于遗留审计项，不能视为本次完成的观察覆盖。 |
| PSP-06 | 待验证 | Unix 原生执行、目录同步失败后的 `published=true` 实际故障、物理/虚拟块设备断电模拟尚未在本地验证。新建祖先目录及素材持久性不在保证内。#132 的其他条目和整体验收继续保留。 |

## 本地验证

Windows portable MSVC；独立 `artifacts/workspace-stall/wgpu-upgrade-target`，debug information/incremental 关闭，最多两个编译任务。

- `durable-project-save-publication-final.log`：4 项新保存发布测试及 2 项已有临时文件回归通过。
- `durable-project-save-worker-final.log`：新真实保存失败/日志用例与既有工程切换回滚用例共 2 项通过。
- `durable-project-save-regression.log`：core 单元 15 项通过、1 项既有手动性能用例忽略；打包事务 4 项、视频/版本上限 3 项、host 单元 31 项通过。
- `durable-project-save-lifecycle.log`：12 项工程保存/新建/打开/导入/关闭生命周期用例通过。与上述专项合计 73 个不同测试通过、1 项既有忽略，专项复跑不重复计数；跨平台结果以该提交的 CI 为准。
- 首轮新文件锁测试把 Windows 原始错误过度限定为 SHARING_VIOLATION(32)，实际 MoveFileExW 返回 ACCESS_DENIED(5)。已纠正这个新测试的错误假设，明确检查这两个系统拒绝代码；旧字节、临时文件、Error 阶段与显式恢复断言不变。首次失败日志 `durable-project-save-publication-tests.log` 保留。
- 首轮代码产生的 Windows unused `fs` import 已加平台条件后消除。首轮桌面测试命令误用二进制名作为包名，Cargo 拒绝执行；修正为 `-p motion-desktop --bin motion-studio` 后实际运行通过。现有桌面 17 项编译警告未在此更改。
