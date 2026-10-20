# 移除内置镜头光与粒子效果

## 用户范围与分支协调

按用户要求移除镜头光效、运动粒子及全部五项粒子预设：lens_flare、particle_emitter、starfield、sparks、dust、snow、energy。节点编辑器与 UI 编辑器组合生成效果明确留待以后，不在本批实现，也不以未完成的组合能力替代现有效果。分支 codex/remove-bundled-scene-effects 最初基于 #368 的 1381cc1；开始前逐项检查当时 15 个开放 PR 的文件清单，没有效果包注册/源文件交集，快照在 artifacts/pr-queue/scene-removal-branch-coordination.json。其他线程正在处理的快捷键集成不修改。

#368 六项平台检查全部成功后，另一会话已将其合入 main（5152033，中文 squash 标题）。发布前核对 5152033 与原基线 1381cc1 的树均为 f6e76c5f20e6398831cac67a2759195ea211b36a，完全一致；仅调整本分支尚未发布的基线，暂存区和工作文件保持原样，验证结果仍对应相同产品源码。原签名提交通过 Git 数据接口按 SHA 精确还原并验证，没有重写远程历史。证据在 artifacts/pr-queue/scene-removal-main-alignment.json，本 PR 直接以 main 为基线。

文件级交集仍存在：#373 也修改 core/src/lib.rs，#365/#362/#357 也修改 shell/mod.rs，#353 也修改 validation_inventory.json。本批在前两处只新增 cfg(test) 夹具/测试模块声明，在清单中只迁移退役生成器与库路径；其他分支的导出、面板模块和图标工具契约均需保留。这些是共享登记文件，不能把“效果实现没有交集”写成“所有文件都没有交集”，后续集成仍须核验。

## 功能边界

- 产品内置库仅保留既有图像效果；两套场景包及五个历史归档不再由生产 builtin 模块嵌入或注册。默认目录保留 95 个独立图像效果。
- 旧版把内置包写入插件目录，因此同时排除这两个退役包 ID 的自动加载与重新导入，防止重启后恢复。文件保留，不删除用户缓存，不重写工程或动画数据；实际请求退役包时返回包含包 ID、版本、哈希和替换提示的错误。旧工程的这些效果不再可用，仍可以删除效果、撤销及保存。
- 正常退役不当作加载故障；读取、损坏归档及其他真实失败继续进入原诊断渠道。独立 ID 的自定义 SDK 场景包仍可安装、重开和解析。
- 保留当前 SDK 的投影、精灵、粒子出生历史、场景与编辑器接口及其行为验证。现有 SDK 渲染种类不等于已完成通用节点 ABI；本批没有新增节点/UI 编辑器功能或新的渲染架构。

## 测试数据与来源

旧场景与粒子目录移到 crates/motion-effects/tests/fixtures，原归档、声明、shader 和第一方页面字节保持原样。加载辅助函数只位于 tests/support，所有测试和显式运行的 SDK 探针改为显式注入，生产默认注册不依赖它们。生成器、浏览器驱动移到 sdk/tests，工具清单及 SDK 路径回归同步更新。没有新依赖、第三方资源或兼容名称；PROVENANCE、ATTRIBUTION、NOTICE 已检查，未发现新增来源违规。

原归档哈希、shader/编辑契约及事务断言保留。archive_identity 原本断言自动注册的部分改为“默认不可用，显式 SDK 注册后仍逐字节相同”，这是此次移除功能的契约变更，另有旧缓存连续启动两次及错误分流的负例，不是为了让失败测试通过而删去断言。

## 错误与降级审计

| ID | 分类 | 处理与证据 |
|---|---|---|
| RETIRE-CACHE | 正常分支 | 合法退役缓存保持原文件，不重新安装，不记录为故障；损坏缓存继续保留诊断及原数据。 |
| RETIRE-RESOLVE | 功能错误边界 | 缺少退役包返回 Err，保持包 ID/版本/hash；实际 engine 请求在默认 Info 日志与 Error stderr 留下相同原始原因、请求号、操作和耗时，stdout 无日志。原生负例通过。 |
| RETIRE-INSTALL | 功能边界 | 在变更目录或注册表前拒绝退役包导入；独立 ID 的用户包为正对照。 |
| RETIRE-DOCUMENT | 数据边界 | 实际旧工程加载、保存、重开、删除效果与一次撤销恢复原始项目对象；错误请求未修改工程，worker 保持可用，缓存字节不变。原生回归通过。 |
| RETIRE-WORKER | 既有覆盖 | 未新增线程、重试、超时、panic 吞噬或观测计数器；原非 Present start/in-flight、Error 分流及健康 Present 静默保持原样。 |
| RETIRE-SDK | 既有能力验证 | 通过测试专用注册继续运行投影、粒子历史、回收、编辑事务与 GPU 回归，不把移除内置效果变成取消底层验收。 |

## 验证进度与限制

效果包完整回归 46 项通过，含五项新增退役回归；首次编译的新断言使用 unwrap_err 要求成功类型 Debug，已改为显式检查 Err，原失败日志保留 scene-removal-effects-first.log。工具 52 项中 51 通过、1 个既有外部 loader 检查跳过（30.442 秒）；随后使用实际 effect_tool 完成五套库（含测试夹具）的清单检查。SDK 五项实际生成、归档哈希、跨平台字节及失效 CLI 检查通过（6.611 秒），没有修改已发布归档。首次差异检查发现删除块留下一个末尾空行，修正后 staged diff 检查通过。

核心完整回归 251 项通过，桌面完整回归 613 项通过、1 项既有忽略（558.03 秒）。新增原生测试在完整桌面运行中通过；此前为提前核验截图单独执行也通过（12.65 秒），两个运行保留不同产物目录。随后在旧工程样例中增加两个实际参数关键帧，再定向执行同一个原生测试通过（16.28 秒），完整对象等值断言验证动画随保存重开及删除/撤销保留。此后改动仅为测试夹具，不重新运行未改变的整套桌面回归。中英文四张 1280×800 实际 PNG 均可解码且非空，中文目录与搜索、英文搜索已查看，目录保留 95 项，搜索退役效果无匹配。产物位于 artifacts/retired-builtin-effects/{preview,native,final-native}。

首次四包组合在链接时遇到磁盘不足 LNK1180，未运行测试，日志 scene-removal-integrated-tests.log 保留。用 Cargo 按包清理确认过的构建缓存后重跑；后续磁盘再次满时，仅清理未使用的旧 wgpu-upgrade-target 的 desktop/core/host/render 构建产物，保留源码、测试日志和其他线程正在使用的 target。重跑中的 core/desktop 均通过，host 两项重新打开会话的测试遗漏显式夹具注册，因预期退役错误失败；已补齐测试会话注册，所有原断言保持。最终完整宿主单元/集成 81 项、宿主文档 1 项、渲染 289 项通过，渲染 1 项既有忽略；日志 scene-removal-host-render-final.log。首次失败仍保留在 scene-removal-integrated-tests-after-clean.log。一次辅助脚本 py_compile 因无磁盘空间失败，释放空间后通过；不记作产品失败。

FFmpeg 开发构建通过（81 秒）；实际 libav 视频解码通过。使用现有打包工具校验运行库清单、架构、哈希和来源，复制 DLL 与许可证形成 artifacts/retired-builtin-effects/run；仅保留系统 PATH、移除 FFMPEG_DIR 后，该目录中的程序仍能实际解码视频。此目录是开发验收构建，不冒充正式发行或发布版验收。额外运行的整套 CLI 截图流程在 60 秒超时，结束桌面完整测试后单独复现仍超时；已保留截图和第二次 stdout/stderr/超时记录，原因待确认，不能归因于并发或宣称整套 CLI 验收通过，没有放宽原检查。

最终来源检查 1536 tracked paths、面板样式消费检查、差异检查及浏览器驱动语法检查通过。远程 CI 尚待完成；未将编译中或等待中的步骤视为通过。本批尚未重新执行历史 HTML 页的完整 Playwright 交互。

## Release 补充验收与分支改名

按用户要求，已发布的分支改名为 `refactor/remove-bundled-scene-effects`；旧 #375 因源分支改名关闭，由 #378 接续。两个 PR 的产品提交均为 d61923101655fc56adbd584eaed4dc9166e576ae，没有重写已发布历史。

旧共享 Release 缓存的第一次链接缺少 libffmpeg_next rlib；仅清理该包后，第二次又缺少 thiserror 的生成文件，均为未能构建，保留 scene-removal-release-build.log 与 scene-removal-release-build-v2.log，不归因于某项未经证实的外部清理。随后使用本批独立 target 和 E 盘临时目录从现有锁定离线依赖完整构建，`cargo build -p motion-desktop --release --features ffmpeg --locked --offline` 通过，耗时 26 分 53 秒，日志 scene-removal-release-isolated.log。

独立产物位于 `artifacts/retired-builtin-effects/release-run`。使用原打包校验器检查架构、运行库清单、哈希及来源，再复制 7 个共享 DLL 与许可证等共 12 个运行库载荷文件；只保留系统 PATH、移除 FFMPEG_DIR，实际 libav 解码通过。中文和英文各执行一次 CLI 冒烟，保留原 CI 的 40 秒限制：中文 5.766 秒、英文 3.485 秒，均退出 0；各生成 53 张实际 PNG，`verify_design_screens` 分别通过，产品 stdout 均为空。两份报告、stdout/stderr 和本地构建摘要与二进制同目录保留。

此结果补齐本地优化构建的中英文基础 CLI 截图验收，不消除原开发构建两次 60 秒超时，也不证明它们的根因已修复。没有放宽超时、修改原断言或把原失败改成成功。节点/属性专用 CLI 的全量组合、跨平台 GPU、全部 DPI 以及远端 CI 仍按各自验收范围判断，不能由这两次基础冒烟替代。

## DESIGN §0.7 与 §12

| 基线项 | 处置 |
|---|---|
| path_controls 六种步距 | retained with reason：不修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：不修改几何布局。 |
| path_controls 按钮/字段高度 | retained with reason：不修改控件尺寸或命中范围。 |
| path_controls 自绘数值底色 | retained with reason：不修改数值编辑。 |
| path_controls 布尔宽按钮 | retained with reason：不修改路径布尔控件。 |
| effect_controls 行高/步距 | retained with reason：仅目录数据改变，不修改绘制行。 |
| effect_controls 布尔按钮 | retained with reason：不修改布尔控件。 |
| effect_controls 枚举按钮 | retained with reason：不修改枚举控件。 |
| effect_controls hover alpha | retained with reason：不修改 hover 样式。 |
| effect_controls 缺少 depth | retained with reason：不修改属性树。 |
| timeline 11 列坐标 | retained with reason：不修改时间轴。 |
| timeline snap guide | retained with reason：不修改吸附提示。 |

本批未写入 panel 颜色/尺寸、token、布局或交互。§12 中本批相关的默认目录与搜索结果已通过实际中英文原生截图核验；其他面板基线、DPI、浮窗和跨平台人工输入不在本次复验范围，不能宣称整个 DESIGN 已整改。开发构建 CLI 超时单独保留为待查项，Release 基础截图验收结果见上。
