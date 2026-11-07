# Windows 缓存目录 junction 拒绝的普通应用验证（#64）

本批只补实际平台与默认信道证据，没有修改生产代码、既有测试断言、权限策略或渲染
行为。验证基于 `ea2ac4738dd8da36ce4215ed7ec37c102de90903`，延续
[私有存储接入](desktop-driver-cache-storage.md) 的 64S-5 平台范围。

## 实际场景

两个独立普通桌面进程使用同一带 FFmpeg 的 exe，通过正常窗口与 `--mcp-ui` 启动，
各自隔离 LOCALAPPDATA、工程与工作区；未设置 MOTION_LOG。使用正常 Session 和
缓存 prepare 回调，不注入 IO 结果或替代 logger。

- 健康账户首次 load 为 missing，缓存保持启用，实际写出 69397 字节的 `.cache`。
- 拒绝账户的 `MotionStudio/driver-pipeline-cache-v1` 是 PowerShell 创建的真实
  NTFS junction，指向本验收私有目录。原生元数据确认 reparse tag 为
  `0xA0000003`，FILE_ATTRIBUTE_REPARSE_POINT 存在，解析目标与预设目录完全相同。
  目标内放置一份 sentinel，验证退出后仍只有该文件且原字节不变，没有缓存快照。
- 失败状态保留 `enabled=true`、`disabled=true`、`restored=true`、`load=null`、
  failed restore ticket、原始 lastError 和零次保存；当前管线准备仍 installed，
  没有把缓存失败升级为有效当前画面的失败。抓图产生的新准备请求不重试缓存失败。
- 两进程均成功呈现、抓图并正常退出。当前 64×64 完整 RGBA SHA-256 相同：
  `afc421c2248cb70f37a306262f50d10518d5cc4a8db1dd6dd7c2c9861ecba47d`。

默认文件日志与 stderr 各有且仅有一条链接拒绝 Error，包含 request、Restore 操作、
受限长度的缓存路径、document_generation、project_revision、elapsed_ms 和原始
`prepare storage` 原因。文件中同请求的 Info start 先于失败；stdout 每行均能解析
为 JSON-RPC。状态回复中的错误原因是协议数据，不是日志污染。健康对照没有该错误。

## 证据边界与首次失败

Rust 的 `FileType::is_symlink` 将该 junction 识别为链接，因此实际原因是
`pipeline cache must not follow a symbolic link`，命中通用 reject_link 分支。
这是 Windows 实际 junction 拒绝的证据，**不是**后续 `file_attributes & 0x400`
专门分支的独立执行证明。没有构造未知 reparse tag，也未验证云文件或其他原生类型。

新控制器的四次失败原样保留：PowerShell `-Command` 未按预期传参、生成脚本换行
转义错误、当前 Python 没有 `os.path.isjunction`、误把 junction 预期原因为后续
属性位分支的措辞。前两次未启动拒绝场景，第三次只建立 junction；第四次已实际
拒绝并呈现，随后新检查的错误措辞预期失败。最终使用 `-File` 的显式参数、原生
`st_reparse_tag` 和实际通用拒绝契约，两进程全部通过。没有修改产品以适配验收。

| ID | 分类 | 结果与限制 |
| --- | --- | --- |
| 64J-1 | 既有功能覆盖 | 真实 junction 在任何缓存读写前被拒绝，目标文件和当前完整像素不变 |
| 64J-2 | 既有观察覆盖 | 默认 Info start、文件/Error stderr 各一次、stdout JSON-RPC 隔离通过 |
| 64J-3 | 正常分支 | 健康 missing 不报错，实际产生缓存文件；失败账户同生命周期不重试 |
| 64J-4 | 待验证 | 特定属性位兜底分支、其他 reparse 类型、Unix 实际失败、跨账户/父链竞态仍未证明 |

两次最终过程各约 3110 ms、3203 ms，包含窗口和 MCP；这里只确认完成，不作性能
比较。原有 Epic Vulkan overlay JSON 错误仍保留，未被此次缓存检查清除。

[证据包](windows-cache-junction-evidence.zip)：102 个成员、3494805 字节，SHA-256
`eaf9ad9580f21a002684e2e480d9a6a1c967605d5cb69180e4239fdf9ba1d3f2`。
ZIP CRC 与逐成员 SHA-256 已验证；保留源码、runtime 和真实缓存文件身份，全部
控制器版本、失败命令、最终原生元数据、请求、日志、状态与完整 PNG。exe、DLL、
实际 driver cache 及带 junction 的账户目录不入包。#64 及 PR 草稿继续开放。
