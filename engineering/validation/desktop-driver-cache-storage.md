# 桌面私有驱动缓存接入（#64）

本轮基于 `fa9bfca409e917eb95e629ae3ed9291fc7ef98b9`。此前后台任务接口的
证据见 `driver-cache-worker.md`；本记录只描述新增的桌面目录策略和 Session
接线，不把此前的绿色验收当成本轮结果。PR #627 与 #64 保持未完成。

## 行为与边界

- 桌面正常启动为 Session 配置账户缓存目录：Windows `LOCALAPPDATA`、macOS
  `HOME/Library/Caches`、其余 Unix `XDG_CACHE_HOME` 或 `HOME/.cache`，追加
  `MotionStudio/driver-pipeline-cache-v1`。只接受绝对路径，没有共享临时目录回退。
- 目录来自受信任的进程账户配置，不能由工程、插件或导入参数指定。受信任的
  账户目录及其祖先是明确前置条件；这不是任意文件路径沙箱，也不抵御同账户
  恶意进程。包校验和不是驱动缓存来源的认证机制。
- Windows 新叶目录以受保护 DACL 创建，只允许 owner rights、SYSTEM 和
  Administrators。每次操作重新验证叶目录的 DACL、所有者及 reparse 属性；
  现有不符合目录直接拒绝，不修改权限。Unix 验证账户所有权与 0700，文件
  拒绝符号链接、额外硬链接及组／其他用户写权限。
- 目录建立和验证进入 `run_with_storage` 已发布的状态、Info start、计时、
  panic 与原始 IO 错误边界；配置和命名不做文件 IO。底层错误保留类型与 cause。
- 恢复先于首次实际效果预热，使用 Session 现有 worker。首个完整普通帧或
  曝光帧成功呈现后，只有实际原生效果管线创建数增长才安排保存；至多每
  30 秒一次。正常 Present 不读盘，不为轮询打印日志。
- 有界资源为：worker 1、工作队列 1、预热待入队 1、缓存待入队 1、完成通道
  1；worker 在完成通道满时还可能持有 1 个结果。取消或替换不新建 worker。
- 存储 IO／原生／panic 失败停用该附着生命周期的持久化 IO，保留原因，
  普通预热继续。损坏封装在拒绝后可由之后独立成功的保存覆盖；不是重试失败
  的读取。已有错误不会因后来成功保存而清除。
- 单元测试的通用 Engine 启动不使用开发者真实账户缓存。专门验收必须提供
  独立目录并调用同一生产接线，不能用这条隔离措施声称生产账户路径已验证。
- 调度修正单独说明：普通预览 pending 也阻止自动播放时钟前进。此前只有曝光
  集合参与这条屏障，Windows CI 原有播放研究在普通预热的未完成回复后失败。
  本轮不修改其断言、采样数量或等待预算。

## 本轮审计项

| ID | 分类 | 验证目标／当前证据要求 |
| --- | --- | --- |
| 64S-1 | 功能接线 | 实际 native GPU：同一 worker 恢复、预热、完整像素与检查点，再由独立 Renderer 恢复并比较全部像素 |
| 64S-2 | 新增观察 | 受控阻塞验证 callback 返回前 Reading／preparing 和 Info start；取消后的旧结果不能安装 |
| 64S-3 | 功能边界 | 损坏文件不送 GPU；保留 typed error，独立保存后可恢复，像素保持一致 |
| 64S-4 | 功能与观察 | 真实缺失路径 IO 停用缓存；65 次轮询无重试，普通渲染继续，原始 OS cause 可取 |
| 64S-5 | 新增权限边界 | 实际 Windows DACL 创建／重复验证／拒绝不安全目录；Unix 权限与链接由对应平台测试验证 |
| 64S-6 | 新增观察 | 默认 Info 实际文件、Error stderr、stdout 隔离；storage callback 阻塞前有 start，失败只写一次，原生新请求可恢复 |
| 64S-7 | 现有覆盖回归 | 原有 host 预热、render 缓存与默认日志断言保持原样并重跑；不扩大原来的等待预算 |
| 64S-8 | 功能接线与待验证 | 实际 Session 普通／曝光窗口及 detach/attach 已有专项；实际桌面启动和独立进程 cold/warm 调度／耗时仍待验证，不能用 Session API 测试替代 |

## 仍未完成的 #64 范围

当前／即将使用效果已有限定窗口和实际 native 预热；嵌套或 time-remap 独有
边界、明确可见的准备状态、独立进程首次／再次添加效果阻塞对比，以及完整
平台／格式／生命周期矩阵仍需继续。实际创建次数不等于驱动报告的命中次数。
本轮不修改 UI 样式，不宣称无卡顿或所有固定 GPU 工作已移出前台。

## 原始执行证据

原始日志目录为 `artifacts/issue-goal-current/native-pipeline-preparation`，
命令由同目录 `run.py` 记录完整 argv、HEAD、环境、开始／结束源文件 SHA-256、
退出码与耗时，不把活跃进程或局部输出当成已通过。

- `cache-storage-host`：真实 `motion-host` 预热组 12 passed，0 failed、0 ignored，
  188 filtered。总计 174.688 秒，测试 56.10 秒；2467 个源文件前后不变。
- 增加独立可读取的 GPU／像素／状态证据和实际 Session 窗口专项后，
  `cache-storage-host-surface`：13 passed，0 failed、0 ignored，188 filtered，
  总计 133.265 秒，测试 62.50 秒；2468 个源文件前后不变。
- 实际 NVIDIA GeForce RTX 4070 Laptop GPU / Vulkan：91674 字节快照，保存
  5 次实际效果原生创建后由独立 Renderer 恢复；当前／未来画面完整 RGBA 一致。
  `persistent.json` 记录 3 次存储 callback（冷读、保存、暖读），65 次健康帧
  轮询没有追加存储操作。保存次数不是驱动命中次数或性能指标。
- `session-surface.json` 对普通和曝光两种真实隐藏窗口分别记录 Saved、重新
  附着后的 Restored、原 worker 保留、全部像素相同、工程不变以及 owner 端
  效果管线创建为 0。该测试不涉及实际应用输入派发或物理显示延迟。
- `cache-storage-desktop`：带 FFmpeg、diagnostics、ai-depth 的实际应用测试
  3 passed，0 failed、0 ignored，1834 filtered；编译 6 分 55 秒，测试 10.18 秒，
  总计 427.359 秒；2468 个源文件前后不变。实际 Windows 私有 DACL 创建、重复
  校验和拒绝不安全目录均通过。默认 Info 下，存储策略原始失败在文件与 stderr
  各出现一次；stdout 没有产品 cache 日志；阻塞 callback 返回前已保存 start。
  之后补上不支持缓存的后端分支与普通 pending 播放屏障，其后复验如下。

最终版本的限定复验（各命令开始／结束的 2468 个源文件身份均相同）：

| 原始记录前缀 | 结果 | 总耗时 |
| --- | --- | --- |
| `cache-storage-desktop-playback-fix` | Windows 私有目录／默认日志 3 passed，1834 filtered | 309.922 s |
| `cache-storage-original-paced` | 原始播放研究 1 passed，1836 filtered | 52.578 s |
| `cache-storage-playback-barriers` | 原有播放屏障 3 passed，1834 filtered | 7.579 s |
| `cache-storage-dx12` | 实际 DX12 分支 1 passed，1836 filtered | 8.266 s |
| `cache-storage-original-log-regression` | 原默认缓存日志 1 passed，1836 filtered | 8.953 s |
| `cache-storage-all-targets` | render / host / desktop 全 targets、FFmpeg / diagnostics / ai-depth 检查通过 | 176.031 s |
| `cache-storage-render-regression` | 原 render 缓存组 8 passed，322 filtered | 84.906 s |
| `cache-storage-existing-single-present` | 原 native tools 用例 **失败**，0 passed、1 failed、1836 filtered | 21.109 s |

所有上述测试均无新增 ignore。原始播放研究的 baseline、blur、raster、combined
四种模式，各 5 次预热和 60 次完成，所有相邻完成帧严格递增，工程状态不变。
原有断言、期限和样本数完整保留。耗时只用于记录本机运行，不是性能承诺；
当时机器也存在其他工作树的构建活动。

最终 Vulkan 日志再次验证受控阻塞前的 Info start、真实权限拒绝、Error 两路
各一次、Debug 抑制、stdout 隔离与新显式请求恢复。DX12 的 backend receipt
实测为 NVIDIA RTX 4070 / `Dx12` / `supported=false`：测试该后端的真实 owner
线程错误及之后 Unsupported 正常完成，不把它当作 Vulkan 存储 IO 的验证。
各平台路径差异没有用假造缓存数据抹平。

全 targets 检查的 50 个带源码位置 warning 与旧阶段相比，按文件和消息归并
没有新增；已有 engine `queue` warning 仅因新增启动接线移动行号。未自动修复
或清除其他任务的既有 warning。

## 已保留的 CI 失败

旧提交 `fa9bfca4` 的 Windows run `42213685935` / job `127878571017` 为
1724 passed、12 failed、69 ignored，耗时 2607.33 秒；Linux、macOS Intel、
Rust/GPU 和依赖检查通过。原始失败日志单独保留，不把它称为新提交绿色。

多项原生桌面测试在单次 Present 返回尚未就绪后直接断言已完成，说明预热
轮询与现有原生调用约定仍需继续整合。播放研究另有
`unfinished exposure must hold the playback clock` 失败，对应本轮普通 pending
调度修正。未删除测试、改 ignore、扩大预算或重新运行同一 CI 掩盖首次结果。
本轮仅验证新增接入并定位旧提交回归；这些记录不代表 12 项问题已全部解决。

本机最终源代码复现 `panels::tools::tests::native` 同一失败：
`native.rs:88` 对 30 秒内收到的一次 Present 布尔值作真断言，实际已回复
`Ok(false)`，并非接收超时。完整 stdout/stderr、源码身份和退出码 101 保留；
不将其称为通过的负对照。这条原生调用与异步准备的整合是下一步工作。

## 审计处置

- 64S-1／2／3／4：本轮新增功能和观察按上述 Vulkan 限定样本通过；没有把
  取消、缺失、不支持或过期当成故障，也未用轮询重试实际故障。
- 64S-5：Windows 私有叶目录验证通过；Unix 权限／链接测试已编写但本机未执行，
  Windows reparse 拒绝分支尚缺实际链接夹具，不能据编译声称已验全部权限场景。
- 64S-6：Vulkan 的真实存储失败和 DX12 的真实 owner 错误分别验证；未强制
  触发系统 descriptor/token 释放失败、原生缓存不返回数据或存储 callback panic。
- 64S-7：原有缓存、日志、播放屏障和播放研究按上表通过；单次 Present 原生
  工具回归仍失败。未替换断言、扩大预算、吞错、改返回值或增加恢复重启。
- 64S-8：Session 窗口普通／曝光恢复已验；实际应用独立启动、真实账户目录、
  跨进程 cold/warm、重加效果与完整平台矩阵仍待继续。#64 和草稿 #627 不关闭。

独立证据包为 [desktop-driver-cache-storage-evidence.zip](desktop-driver-cache-storage-evidence.zip)，
102 成员、3325477 字节，SHA-256
`e107e49673cee537411071dbb41737ebf0bad7203a76a6748cd4a81f0938dd51`。
已逐成员校验 SHA-256 与 ZIP CRC。包含 11 条命令的原始结果、每次源文件身份、
实际 GPU／RGBA／窗口／播放记录、两后端默认日志、Windows CI 原始失败及当前／
基线源码。旧四个证据包 SHA-256 均保持不变，没有覆盖首次失败或旧结论。
