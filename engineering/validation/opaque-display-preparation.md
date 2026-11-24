# 显示传递声明与工作空间不透明合成

本批继续 #83，在 [显示码值传输](managed-display-transport.md) 上补齐窗口呈现前的
两个条件：明确的传递函数声明，以及显示变换之前的工作空间黑底合成。
这里仍未接通普通窗口、多视图和导出，不能据此关闭 #83。

## 契约与实现

`ColorSignal.transfer` 是必填字段，取值为 `linear`、`srgb` 或 `config_defined`。
OCIO 的 `encoding` 是分类标签，不能据此推断传递函数；`config_defined` 表示
所选、已核验内容身份的 OCIO 程序负责该端点变换，不代表系统窗口接受该码值。
没有缺失字段默认值、旧版本迁移或显示名称启发式。本 PR 尚未发布的 OCIO
工程结构同步更新两处 Rust 夹具，原有工程版本字段保持不变。

宿主只在工作区为线性、全范围、预乘 Alpha，且显示声明为全范围 sRGB、
Rec.709 原色、D65 白点和 display-referred 时，额外准备 `SurfaceDisplay` 程序。
它与原有保留 Alpha 的 `Display` 程序分开缓存、计量并原子发布。
不符合声明时 `surface_display()` 为正常能力缺失 `None`，显式显示端点仍存在；
消费者不得把它解释为允许旧 sRGB 路径回退。

不透明程序把预乘工作 RGB 合成到工作黑底后，再执行原生 WorkingToDisplay。
Alpha 置为一；零 Alpha 的隐藏 RGB 规范为黑色，避免泄露不可见颜色。
这一操作融合进已有 shader，不增加目标、额外图像 pass、文件读取或像素回读。
保留 Alpha 的显示和输出程序不改变语义，新模式参与缓存键，不复用旧程序身份。
有管理颜色的创作采样同时要求工作声明 `transfer=linear`。

上述声明是工程的显式输入约束，不是对任意 OCIO 配置数学内容的自动推断。
实际 wgpu surface 的色彩空间、格式、Alpha 模式与 OS 显示链仍须在宿主消费者
单独核验；本层不声称已支持监视器 ICC、HDR 或普通窗口播放。

## 验证与错误审计

原始输出：`artifacts/issue-goal-current/color-surface-contract/`。

| ID | 性质 | 证据和边界 |
| --- | --- | --- |
| ODP-01 | 新格式契约 | 三种显式传递值往返，缺失/未知值拒绝；完整工程快照与 undo/redo 保持；core 色彩契约 12 项通过 |
| ODP-02 | 原生准备 | 三份 ACES 1.3/2.0/2.1 配置现场准备独立四程序；移走配置后仍保留程序并采样，重新准备保留真实资源错误；显式恢复成功 |
| ODP-03 | 正常能力分支 | ConfigDefined、Linear、P3 原色、D60 白点四种不匹配声明均不生成 SurfaceDisplay，不推断或回退；原三程序和先前准备结果不变 |
| ODP-04 | 真实 CPU/GPU | 原生 CPU 独立计算工作黑底结果；48 个 Float32 像素沿用绝对 1e-4、相对 2e-4 容差，Alpha 精确为一；含零 Alpha 隐藏颜色、负值和高光 |
| ODP-05 | 实际 SDR8 | 四种 RGBA/BGRA UNORM/sRGB 附件的 192 个像素与上述 CPU 结果最终量化比较，允许 1 字节；Alpha 均为 255 |
| ODP-06 | 实际负对照 | 把旧保留 Alpha 的实际 GPU 程序代替不透明程序，12 个 RGB 像素违反同一参考；不是仅用 Alpha 不同证明失败。错误阶段/直通 Alpha 输入也明确拒绝 |
| ODP-07 | 全套 GPU | 补齐既有 Linux/macOS 捕获样本目录后，18 项全部通过；保留首次环境缺失的失败，不改断言或像素参考 |
| ODP-08 | 宿主回归 | 色彩管理 2 项、准备集成 22 项、并发 3 项通过；随后对三配置各增加 Srgb/ConfigDefined 非线性工作声明拒绝，原始 working 准入错误、观察复位和旧结果不变的定向复验通过 |
| ODP-09 | 新阶段观察与修复 | 真实默认 sink 的阻塞验收先暴露独立调用的 Info 缓冲缺陷；修复即时路径在 TLS 借用释放后的刷新，Session 内仍延后到锁外。同一测试随后通过：provider 阻塞时文件已含 Info start 与 SurfaceDisplay 阶段，在途为真；IO/编译两种真实故障保留原因、阶段、耗时，恰有两条 Error 同时进入文件及 stderr，stdout 隔离；原工程与旧准备结果不变，显式恢复成功；不匹配传递声明的正常 None 没有增加 Error |
| ODP-10 | 复用回归与强化 | 宿主色彩 lib 5 项通过，覆盖原始 decode panic、原因 UTF-8 边界、实际原生准备、程序/载荷容量和 Session 锁外记录四种子进程；本次给测试 sink 的 flush 同样加上注册表、Session 与 TLS 无持锁断言，旧阈值与断言保持 |
| ODP-11 | 工具与来源 | 最终 CI 工具 144 项，143 通过、一项既有可选跳过；70 项库存与 6401 个跟踪路径的来源门禁通过；未增加依赖或第三方素材 |
| ODP-12 | 待完成 | 普通窗口有效帧事务、surface 资格、多视图、导出和其余受管消费者仍未接入；3D LUT、完整 ACES 2 及 OS/HDR 资格不由本批建立 |

首次新 GPU 夹具漏写 `mut renderer`，首次宿主编译夹具未穷举新增 Target；
均仅修正夹具，保留 `opaque-display-first.txt`、`native-surface-contract-first.txt`。
全套首次输出 `full-color-image-gpu.txt` 有 17 项通过、1 项缺少
`MOTION_TEST_OCIO_CAPTURED_CI` 而失败。设置原捕获样本目录后输出
`full-color-image-gpu-second.txt` 为 18 项通过，没有跳过或改写该测试。

`default-surface-channels-first.txt` 保留真实缺陷：在途状态已发布，但默认
BufWriter 的 Info 尚未落盘，阻塞期间的同一断言失败；该次子进程的完整
file/stdout/stderr 另存在 `default-log-channels-first/surface-preparation/`。
修复的是生产观察路径，没有删除、放宽或移动阻塞期间的磁盘可见性断言。

远端 run 42473898696 对应提交 `1e0ac121`，不覆盖本批尚未推送的改动。
在它结束前保留该 CI，不以新推送取消正在执行的原生资格。

最终输出另包括 `native-surface-contract-final.txt`、
`host-color-observation-and-capacity.txt`、`default-surface-channels-second.txt`。
本批源码、首次失败、修复后输出、Linux/macOS 原捕获样本副本、四份不可变
数值参考身份与运行时文件/许可身份由同目录 `archive.py` 收为
`opaque-display-evidence.zip`，逐项哈希和整个归档收据保存在本地。
