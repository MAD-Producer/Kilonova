# 管线准备终止收尾与同版本效果包替换验收

关联 #64、草稿 #627。基线为 `23c843c2`；本批实际源码由下述命令收据的
2482 个文件 SHA-256 锁定，不把未提交源码身份等同于该基线提交。

## 问题与修正

普通应用中，错误帧可以没有任何等待回复的帧继续任务。原引擎只在帧继续任务
存在时定期醒来领取准备结果；另一个窗口只绘制界面，或捕获提前返回错误后，
准备任务可能已经结束，owner 却睡在命令接收上。主窗先前的 pending 标记又可能
在 Present 返回 Err 后保留，导致无输入时反复提交同一个失败帧。

本批只增加 owner 对既有准备任务的非阻塞领取：没有帧继续任务而准备句柄仍忙时，
沿用 8ms 轮询间隔；准备完成后恢复阻塞接收。领取仍经过 Session 所属线程、
generation、revision 和原有安装检查。没有新线程、任务重试、driver wait、
自动重启或编译失败回退。正常缓存缺失和不支持仍按原协议处理。

后台编译失败的原始 Arc 错误保留在准备状态，准备句柄清零；owner 在 Session
锁外补一条带 request、generation、revision、phase 和 elapsed 的 Error，下一次
实际帧请求继续收到原始错误。Session 访问或 owner 领取中的 panic 则经过现有
catch_panic 边界，发布 worker failure 并终止，未把它改成成功。

Present 的终止错误同时清除普通 pending 标记；曝光屏障原有的 owner 移除逻辑
保留。此项是停止自动重复提交失败帧的功能修正，与日志记录分开验证。没有清除
用户错误、修改帧回复或靠抑制日志隐藏重复请求。

未修改面板、主题、控件、右键菜单和尺寸。DESIGN §0.7 的既有面板问题全部保留，
原因是本批仅处理引擎完成状态与 Shell 调度，不扩大到布局整改。最终原生截图的
准备中／完成后状态栏像素与此前发布的两张裁剪完全相同。

## 最终验证

| 收据 | 结果 | 耗时 |
|---|---|---:|
| preparation-terminal-native-v2 | 三个原生测试通过，0 ignored；Windows/Vulkan | 157.578s，含编译；测试 51.26s |
| preparation-terminal-playback-test-v3 | 播放／曝光屏障回归通过；新增“普通 pending 遇终止错误后不再自动刷帧”前置条件和断言 | 2.125s |
| preparation-terminal-tools-v2 | 原有真实主窗／浮窗工具激活、拖动与形状回归通过 | 5.656s |
| preparation-terminal-normal-build-v2 | 普通程序 FFmpeg、diagnostics、ai-depth 构建通过；36 条既有 warning | 55.718s |
| preparation-terminal-normal-app-v2 | 五个独立普通应用进程全部正常退出 | 20.094s |
| preparation-terminal-normal-regression-v1 | 未修改的原控制器：八次独立普通进程冷／热回归全部通过 | 27.641s |

这些命令的源码身份完全相同，各命令首尾的源码身份也完全相同。没有在编译或
原生测试运行时修改源码；独立控制器、文档和证据归档不属于编译源码集合。

三个原生测试分别验证：原有有界帧回复及取消；新增无帧继续任务的空闲 owner；
原有准备状态栏的 24 组主窗／浮窗、中英文、宽窄尺寸与 DPI 对照，共 52 张 PNG。

新增空闲 owner 用例真实阻塞私有缓存 callback，先确认 PresentPoll 已返回
Ok(false)、帧继续队列为 0、准备句柄为 true，且仅绘制界面的帧可以成功完成。
释放成功和真实 IO 错误两种结果后，不发任何引擎命令、状态查询、帧请求或 UI
输入，只读原子句柄，要求其在原 5 秒断言内清零。两种情况均通过，工程/revision
不变；IO 错误原文进入默认 Info 配置下的文件日志与 Error stderr，stdout 无日志。
这是存储 callback 的实际错误，不冒充驱动内部创建错误。新子进程看门狗仅约束
测试进程，不改变产品期限。

普通应用采用真实 `--mcp-ui` 和可见但不激活的窗口，共享同一隔离账户下、程序
自己写入的驱动缓存；不清驱动全局缓存，不注入 test-only Engine 或 logger：

1. A 包冷启动，实际 cache load=missing。
2. A 包热启动，load=restored，完整 RGBA 与冷启动相同。
3. B 包只改独立编写的 shader 源码，插件 ID、版本、清单和入口完全相同。
   load 仍为 restored，中心色从 `[32,191,64,255]` 变为 `[191,32,127,255]`，
   完整 RGBA 也不同，证明没有错误执行旧 shader。
4. 换回 A，热缓存下完整 RGBA 恢复为原 A，包 hash 与原 A 相同。
5. 非法 shader 在真实 CPU 包加载时被拒绝；默认文件与 stderr 均保留包 hash、
   effect、pass 0、WGSL parse 和缺失符号。空白源图仍正确显示。通用模型接口
   允许表达一个缺包的效果实例，但实际 render_preview 必须返回带插件 ID/hash
   的错误，不能借已有 native cache 返回成功。失败渲染不改变已提交工程，
   owner 确认终止后 preparing=false，准备状态的 error 保留缺包原始原因。
   空闲观察期间 Present Error 数量保持为 1，不再增长；owner 完成错误在文件和
   stderr 中各一次，stdout 每行均可解析为 JSON-RPC。显式撤销后完整 RGBA 与
   失败前相同；随后显式保存再正常关闭。这里的包加载解析失败、缺少已验证
   program 和 native GPU 创建错误是不同边界，不能混称。

每次捕获都要求 effectPipelineBuilds 不增加；包原文件保持不变。A/B 的相同清单、
WGSL 和 msfx 字节、像素、请求、日志及 PID 均留存。首次完整画面的时间作为
原始测量记录，不据单机小样本承诺所有热启动都更快。

另八次普通进程复核原有已添加效果启动、首次／再次添加、历史包、损坏包的
四组冷／热场景，沿用未修改控制器的完整像素、缓存实际恢复、默认日志信道和
退出断言。与上述五次验证使用同一最终源码和普通 exe。

## 红对照、夹具与协议边界

全部首次结果保留，不通过改动既有断言、ignore、预算、产品期限或返回值获得通过。

- `package-identity-normal-app-v1`：新夹具漏写 working_space，默认 Linear，
  所以屏幕 sRGB 值与所写 sRGB 预期不符。仅为新夹具补写 srgb／straight；
  原像素断言、容差和期限全部保留，原控制器与失败仍在归档。
- `package-identity-normal-app-v2`：四个 A/B 进程已成功，但新探针错误假设通用
  模型 insert 会执行插件安装校验。实际模型编辑先提交，帧错误另行报告。
  原探针不改，保留失败；另建控制器验证真正的 render_preview 错误边界，
  不把插入成功叫作 shader 安装成功，也不据此改动产品模型协议。
- `package-identity-render-failure-v1`：渲染失败断言已通过，退出却等待未保存
  工程的正常用户决策，20 秒看门狗终止它。新控制器补齐显式撤销、像素恢复和
  保存；没有扩大原退出期限或绕过用户保存流程。
- `package-identity-render-failure-v2`：新控制器在 worker 完成与 owner 领取之间
  立即断言状态，不能单独证明永久等待。保留该结果；最终控制器在不再提交帧的
  前提下有界等待 owner 确认，仍保留 preparing=false 断言，并额外要求空闲期间
  不产生新的失败帧。对应控制器与此前版本分别归档。
- `preparation-terminal-baseline-v1`：使用修正前真实 `23c843c2` 普通 exe，运行
  带有界同步的同一核心控制器，实际失败于“Idle UI kept submitting the same
  terminally failed frame”。本轮普通程序通过该原断言。旧 exe 的身份来自此前
  runtime receipt；外层命令对当前工作树的源码快照不冒充旧 exe 的编译源码。

最初工具回归和第一轮播放测试也通过并保留，但最终表只使用当前完全相同源码
对应的轮次；不把第一次临时测试源码身份混称为最终源码。
收尾检查又将 owner 终止分支的日志 cause 接入现有 2048 字符有界写入，调用方的
完整错误不变。该改动后的原生、普通构建、普通程序、工具和播放轮次均重新验证，
最终表使用的源码相同；之前成功轮次没有覆盖或冒称最终源码。

## 错误处理处置与剩余范围

| ID | 分类 | 处置与边界 |
|---|---|---|
| 64-T1 | 功能修正 | 空闲 owner 领取已经运行的任务；真实阻塞成功／IO 错误且无命令的原生用例通过 |
| 64-T2 | 功能修正 | 终止错误清除普通 pending；旧普通程序红对照、最终程序不重复提交和显式撤销恢复通过 |
| 64-T3 | 新观察 | owner 领取失败保留 State 原始 Arc，在锁外一次 Error；默认文件、stderr 与 stdout 隔离实测通过 |
| 64-T4 | 既有覆盖 | 编译 worker 的 Info start、原始 cause、线程 error scope 和请求票据沿用；本批不更改 |
| 64-T5 | 正常分支 | 缺失、不支持、取消、健康轮询不新增告警；只在既有任务进行中唤醒，不重启任务 |
| 64-T6 | 协议边界 | 通用模型 insert 与插件安装／实际渲染区分；无帧回复被改成成功，不改变 respond 语义 |
| 64-T7 | 待补证据 | 新增空闲 owner 的 Session 访问错误及 catch_panic 终止分支未强制注入；正常恢复与实际后台编译／存储错误不能替代这两个分支 |
| 64-T8 | 待补范围 | 多视图自身的近期预热、其余格式／包／平台组合仍未全部验收；本机 Vulkan 不代替 DX12／Metal |

其他任务仍在修改 viewer.rs，本批没有编辑该文件。草稿 #627 和 #64 继续保持打开；
本批的普通应用非法源码被 CPU 包加载拒绝，不替代 native GPU 创建错误的普通
程序端到端注入。既有 GPU worker 原生验证有独立证据，但边界不同。

证据包：[pipeline-preparation-terminal-evidence.zip](pipeline-preparation-terminal-evidence.zip)，
812 个成员，22603615 字节，SHA-256：
`7c22d8c8a6a577f2c99987ec4c0438fd475878ff79debeb067d9d57d32622f60`。
逐成员 SHA-256 与 zip CRC 已验证。包含全部原始失败、最终通过、源码身份、
新源码、控制器、默认日志、真实截图和独立编写的包；不包含 exe、DLL、驱动
缓存二进制、用户凭据或外部资产。
