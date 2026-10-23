# 内嵌节点运行时恢复的原生错误（本地完成，远端组合待验收）

继续本人 #138、#135，作为 #440 的后续提交合批，不另开 PR。开工核对 `node_effects.rs` 没有其他工作目录未提交改动，当前 blob 与 main 相同；近期非祖先只有 10 月 21 日旧错误定义提交。桌面 `project_tests.rs` 在另一个工作目录有暂存修改，因此不编辑它；改用本人曝光错误批次的 `composition_tests/plan_error.rs` 注册独立新测试，注册文件及新路径均无其他工作目录改动。占用收据只读取路径/提交元数据，不审阅他人的 PR。

| ID | 分类 | 证据与限制 |
| --- | --- | --- |
| N1 | 原生传播 | `node_effects::restore` 的返回类型从 String 改为宿主 Result，现有 Effects→Context 原因不再因 `?` 转成文字。诊断文字、包校验、注册次序和拒绝条件保持 |
| N2 | 实际公开失败路径 | 合法模型携带 `0000` 内嵌包，真实 ZipArchive 解析产生原生 Zip 错误。通过公开 Session::new / replace_project 验证 Context→Effects→Zip、原 source 包装和既有 Display，而非手工构造一个成功返回 |
| N3 | 原子性与恢复 | 替换失败后完整 engine snapshot、revision、history、root、注册表的 keys/disabled/diagnostics 与所有 package Arc 身份保持；下一次显式合法替换成功，不重试或重启失败请求 |
| N4 | 正常/文本边界 | 纯十六进制校验拒绝仍是 Message；已经注册的匹配包仍直接复用，不因未使用的旧 runtime 内容而新增失败。不把正常命中视为故障 |
| N5 | 既有默认可观测性 | 新桌面用例通过实际 Engine worker、新建工程与文件系统回滚路径，不使用手工 Flight；核对请求 Info start 在 Error 前、原生原因显示到回复/文件/Error stderr、stdout 隔离与健康请求恢复。不添加日志点或改变热路径 |
| N6 | 明确未覆盖 | 不声称解析了所有可能的 ZIP、JSON、着色器或 registry.insert 失败；类型修正保留它们现有的原生错误，但这批实际故障输入只证明 ZIP 解析。尚不替代两个合集其余验收 |

负面对照过程保留全部输入与日志。第一轮新夹具把 Registry 误当 Arc，E0308 编译失败，未执行测试；原日志 SHA-256 `c88a1a878e22fb814d6fcca1e35ea2abf675f844ecd3a5487fb8dbf6bc186083`。修正为比较实际注册表字段与每个 package Arc 身份。第二轮没有创建 Session 所需目录，原生错误测试已触发，但另两项被前置 NotFound 干扰，0 通过/3 失败不能当成三个产品缺陷；原日志 SHA-256 `bbd27b785f7d8d9e3dfffee163fb4ddf1473427f4f323d4256e8d306005da4be`。

创建夹具目录后，在同一旧生产代码实际运行 1 通过、2 失败、0 忽略；两个公开恢复入口均把原生 ZIP 转成 `Message`，正常文字/已注册包对照通过。有效负面对照日志 SHA-256 `6758fbdda5dce84aeb6e339a4d1f33a2fe7f85b81ffed574b46d7efac0c611c8`。没有删除或放宽任何原有产品/回归断言。

首轮修复后三项新原生用例通过；桌面新增用例已从实际 worker 收到 ZIP 原因并写入 Error，但随后把合法 `plugins` 目录当成残留而失败。原日志 SHA-256 `b5b6cc5917092b4959590c90b9f55515beddc584fcf16f730c10c6478fd68ede` 保留；其中子进程与父进程各显示一次同一失败，不能计成两个根因。运行进程终止后，仅将该新测试改为严格比较失败前后的完整排序目录项，不过滤合法项目，也不放宽回滚约束。唯一 Rust 输入差异为新桌面测试 `80ef29fc`→`aeaec852`，记录于 `node-runtime-directory-fixture-only-delta.json`；生产、原生宿主用例、原有断言和 Cargo/WGSL/YAML 不变。

最终四阶段冻结输入为 `node-runtime-final-v2-source-receipt.json`。桌面七项信道回归通过，完整宿主 126 项通过，合计 133 项、0 失败、0 忽略；前三项原生专项已包含在完整宿主，不重复计数，也不累加前批的 123 项。桌面日志 SHA-256 `54911441d06f482671385dc767a24fc4192fae7562be4e6c677ca9bebb8504c0`，宿主日志 SHA-256 `6b6083ff98c2c75188117cde22e575589f982a91b8c481d8b1c1be2e6437bdba`。全工作区全目标 FFmpeg/diagnostics 检查成功，日志 SHA-256 `a75229f38143c4ee3e6570dd5a1e2e77e739c5a979533ad2e552dc1e59b05694`。

独立默认信道留样实际执行新桌面测试二进制 `7638873f8494104da38f97933ef8f57ac2c682a65985f7345348092f35df8461`。请求 2 的 `new_project` 真实失败产生一条文件 Error、一条 stderr Error；Info start 先于错误，回复保留原 ZIP 诊断，stdout 隔离，未泄漏工程/图层文字。文件日志 SHA-256 `54e012d8ccbd00dedbeb8c19ae99246cde5795440bb1016f7956aff5f3e57307`，stderr SHA-256 `8b93fed3fc20ef836a57b5058e3a222457f2760812379599efd0caabf307b1b1`。同进程回滚后合法新请求成功，不重启 worker；不另计为额外测试。完整回复与原始输出见 `node-runtime-default-output/execution.json`。

FFmpeg Debug 产品构建成功；实际新产品 SHA-256 `89d1abdc14d2987af923cff556c0fa6ef706e7713fd7b4945678e3f05bfb849b`，运行 `--command-list` 退出 0、stdout 24475 字节有效 JSON、stderr 为空，运行前后产品和冻结输入不变。此项不冒充 GUI 像素或完整媒体解码验证。启动收据为 `node-runtime-final-product/execution.json`。

复用既有共享 target，不新增构建目录、依赖或素材。没有用户可见 UI 改动。该组合以 #440 原 head `9ff10092` 为父提交，后续快进更新同一 PR，不改写旧提交；新的远端 head 与实际 main 组合仍待独立 CI，两个合集不关闭。
