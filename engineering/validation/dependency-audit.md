# 依赖漏洞审计门禁

关联 #125。基于 main `7a61233a`，与 #400 的渲染资源预算分支无代码范围重叠。本记录按实际证据分类，编译与远端门禁未完成前不宣称本增量已经验收或合入。

## DA-01：真实审计与依赖修正

原始主线 Cargo.lock 经官方 cargo-audit 0.22.2 和 RustSec 数据库 `4b587f00996ebe5c05a73eaef105ef241e876414` 审计，返回 10 条漏洞记录，退出 1。没有把报告当作告警放行，也没有设置 advisory ignore。

修正 rustls 0.23.45 → 0.23.46、rustls-webpki 0.103.15 → 0.103.16、webpki-roots 与 webpki-root-certs 1.0.9 → 1.0.10，以及 zip 2.4.2 → 9.0.5。五处直接 zip 依赖一并更新；Cargo.lock 保留精确版本和包校验和。升级后的真实报告为 0 条漏洞，退出 0。

zip 9 的文件名读取变为可失败接口。效果包与工程包读取使用原生 `ZipError` 的 `?` 返回，不用默认文件名或忽略非法条目恢复。原有路径、符号链接、重复名称、大小及解压预算检查保留。没有改写已有包、工程 fixture 或像素基线；包字节身份、导出及事务验证仍须通过。

## DA-02：门禁和正常信息边界

独立的 Dependency audit job 审计整个锁文件，不按当前操作系统或架构过滤，因此包含 Android 等未编译目标的锁定依赖；这不代表 Android 得到编译或设备验收。每次 CI 用标准 Git 拉取当前 RustSec 数据库，工具不执行隐式 fetch。下载工具固定版本及官方 SHA-256，失败不重试、不换源、不继续执行未验证字节。每次记录数据库 Git revision、锁文件 SHA-256、原生退出码、完整 JSON 与 stderr。

工具检查已知 RustSec 漏洞；上游 informational/unmaintained 信息保留于报告与收据，不冒充已经修复。`--no-yanked` 明确不做 crates.io 撤回状态扫描，避免把该扫描误报为已完成。原生非零退出码保留，即使 JSON 表示零漏洞也不能成功；零退出码但报告包含漏洞同样不能成功。数据库、报告或安装失败均失败，stdout 保持为空；超时保留已产生的原始输出，不新增重试。

失败和成功均上传固定三个报告文件，保留七天；不上传工具二进制、数据库或凭证。发布任务的 needs 加入审计 job，阻止审计失败后的发布；原有测试／构建依赖保留。工具仅用于 CI，不随产品发布，来源与许可证见 ATTRIBUTION.md。

## DA-03：真实失败对照与待验证项

用相同脚本审计真实主线旧锁文件返回 10 条漏洞、退出 1；审计当前锁文件为零、退出 0；指向不存在的数据库退出 1 并保留 Git 原始失败，三个进程的 stdout 均为空。证据位于开发工作区 `artifacts/dependency-audit/controls.json` 及对应报告。这些是真实审计器对照，另四项 Python 单元测试只证明包装器的故障边界，不代替实际扫描。

首次包升级编译失败暴露 `ZipFile::name` 的 API 变化，原始 15 条编译错误与源收据保存在 `initial-workspace-check.log`、`initial-results.json` 和 `initial-source-receipt.json`。两处调用适配后，workspace all-targets（含 FFmpeg）检查通过；效果包全套 46、工程包事务 4、图导出 1、媒体全套 123，共 174 项通过、0 失败、2 项既有大文件忽略。Rust／Cargo 源收据在各阶段前后均保持一致；不修改包身份断言、数据基线、预算、重试或等待条件。

初次 CI 工具回归发现新增工具未登记 inventory、本地 Python 缺 PyYAML；已登记并在独立虚拟环境安装与 CI 相同的 PyYAML 6.0.3。随后严格 job 集合断言发现新增 job 未列入旧清单，已精确扩展预期集合并新增审计必须执行、不得 continue-on-error、报告上传和发布依赖断言，保留其他检查。最终工具集 70 项通过、1 项既有跳过；没有删除首次失败日志。实际固定下载／校验／安装的 Windows 工具执行也通过，当前锁文件为零漏洞。完整记录位于 `results.json`、`ci-tools-final.log`、`installer-current/` 与逐阶段日志。

带 FFmpeg 的桌面构建通过，不将本地审计或包装器单测当成完整发布验收。informational/unmaintained 报告仍保留，不因漏洞数为零而声称所有依赖均持续维护。工具超时输出保留目前仅有实现，未以实际阻塞审计进程验收。

原 head 6653fe5f 的 run 39614319284 中，Dependency audit 实际成功；下载 artifact 12351563163，核对固定三份报告、原生退出 0、零漏洞及 Cargo.lock SHA-256 与本地完全一致，数据库 revision 同上。这覆盖 Linux 工具安装、真实审计和远端上传。Rust/GPU、Linux 与 macOS 通过，Windows 桌面两项原生测试超时，PR 尚不能合并；原始失败、测试驱动消息处理修正及剩余采样 Present 根因见 native-capture-message-pump.md。不把审计成功冒充整体 CI 已通过。

本次没有新增产品日志事件、观察键、计数器、UI 或渲染语义；文件名失败沿既有原生错误返回，完整默认日志通道未因这项依赖升级单独重验。DESIGN §0.7 十二项全部 retained，此次没有修改对应面板；逐项理由沿用 host-error-contract.md，不声称视觉验收。

#125 的全仓格式／clippy 门禁、发布配置策略等其余要求继续开放。本次不通过批量格式化干扰其他正在开发的分支，也不把依赖审计增量当成整个 issue 已完成。
