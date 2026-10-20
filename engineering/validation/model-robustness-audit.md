# 数据模型健壮性合集复核

关联 #132；这是逐项证据复核与契约测试补充，不是关闭合集的证明。基线 `352bd2f7`，未改生产行为、错误回复、日志策略或界面。

## 分支协调

开始前核对本地 worktree、近期分支及开放 PR 的文件范围。#344 正在修改格式版本、采样与宿主 session，#349 修改属性筛选，#350 补多相机验收，#348 修改菱形绘制；另外保留配置隔离与节点手势分支的工作。本分支只新增独立测试文件、说明导入探测的既有契约，避免重复这些实现。未来版本用 `u32::MAX`，不与 #344 的下一版本号抢占同一测试假设。

## 逐项结论

| ID / issue 子项 | 分类与结论 | 证据及剩余限制 |
| --- | --- | --- |
| MR-01 / 1 候选引用图 | 既有功能覆盖；新增契约测试 | `composition_candidate_graphs.rs` 在共享菱形、重复引用、逆序库和根展开 1023 的图上，把每个 source/target 的候选结果与实际 Engine 编辑逐一比较。根新增一个叶子可到 1024，共享底部新增则实例化 512 个叶子而拒绝；失败后项目、revision、undo 不变。没有据此声称修复了未复现的 memo 缺陷。 |
| MR-02 / 2 关键帧目的地冲突 | 既有修复与失败路径覆盖 | core/model `key_collisions` 验证 move/copy 占用拒绝，标量、向量与效果路径；core 验证存盘字节、revision、history。参见 `keyframe-collisions.md`。本次未改碰撞规则。 |
| MR-03 / 3 预合成容量 | 既有修复与失败路径覆盖 | `composition_boundaries`、`composition_resources` 覆盖包含主合成的容量上限、最后槽位、批量失败回滚、磁盘和历史不变。参见 `composition-input-boundaries.md`。 |
| MR-04 / 4 帧率与时间映射 | 既有修复与失败路径覆盖 | `composition_frame_mapping`、`composition_fps`、`composition_boundaries` 覆盖零父/子 fps、非有限输入、溢出、混合帧率与负/小数时间，空音频分支仍验证零 fps。参见 `composition-frame-mapping.md`。 |
| MR-05 / 5 插件禁用配置损坏 | 既有恢复和协议诊断；默认日志待补 | `registry_recovery` / `registry_failures` 真实损坏、截断/超限、目录替代与恢复测试通过；保留文件和 bundled 插件，诊断经效果列表的 `errors` 返回。但 `Registry::load` 将恢复原因保存到 diagnostics，Session 构造未明确发布默认级别日志；本次没有证明默认文件日志、Error stderr 和 stdout 隔离，因此不能标记为可观测性完成。后续应在业务锁外发布，不能直接在 Registry 内补 logger。缺失配置是正常分支。 |
| MR-06 / 6 保存发布与清理 | 既有平台实现及失败路径覆盖；平台限制保留 | `storage_publication`、`storage_failure_cleanup` 与宿主 `cleanup_tests` 覆盖真实 Windows 发布拒绝、原始原因、旧字节、临时文件、目录所有权、回滚次级错误、panic 清理和锁外日志。Unix 目录同步实现已存在；本机没有验证发布后目录 fsync 失败，也没有硬件断电试验。参见 `project-save-publication.md` 的 PSP-06、`project-switch-rollback.md`。 |
| MR-07 / 7 格式版本 | 既有校验；新增存储边界测试 | `format_version_boundaries.rs` 验证 0 与 `u32::MAX` 被 validate、migrate、load、save 拒绝；失败加载保留源字节，失败保存保留已存有效项目且可重开，不留临时文件。既有 `video_layers` 迁移测试同时通过。 |
| MR-08 / 8 外部图片路径 | 明确功能/权限边界；新增契约测试 | `image_info` 是注册前调用者选择的候选文件的只读、限额元数据探测，桌面导入随后才复制并注册素材。新增测试验证外部 PNG 可探测但不注册、不生成代理、不复制/修改源或工程；`prepare_image` 仍拒绝绝对外部路径。缺失/无效图片返回原有错误。不是放宽已注册资产的路径检查。 |

MR-01/07/08 共新增 5 项测试；没有新增日志观察，也没有改变任何既有断言。MR-05 的默认日志缺口仍需处理；本 PR 不使用关闭 #132 的关键字。这里的“覆盖”不代表所有异常均有默认日志或所有平台均已验收。

## 本机验证

Windows，使用现有 `artifacts/multiple-camera-layers/env.ps1` 环境；独立 `CARGO_TARGET_DIR=E:/Dev/aem/target/model-robustness-audit`，避免共享编译产物相互干扰。

```powershell
cargo test -p motion-core --locked --test composition_candidate_graphs --test composition_fps --test composition_resources --test key_collisions --test storage_failure_cleanup --test storage_publication -- --test-threads=1
cargo test -p motion-host --locked --test image_candidate_contract -- --test-threads=1
cargo test -p motion-effects --locked --test registry_failures --test registry_recovery -- --test-threads=1
cargo test -p motion-core --locked --test format_version_boundaries --test video_layers -- --test-threads=1
cargo test -p motion-model --locked --test composition_boundaries --test composition_frame_mapping --test key_collisions -- --test-threads=1
cargo test -p motion-host --locked --lib cleanup_tests -- --test-threads=1
```

结果依次为 20、2、4、4、12、5 项通过，共 47 项，无失败/忽略。日志为 `artifacts/property-selection-context/model-robustness-{core,image,registry,format,model,cleanup}.log`。初次宿主 cleanup 过滤器误写为 `ops::project_cleanup_tests`，实际运行 0 项，不计入结果；改为 `cleanup_tests` 后真实运行 5 项通过。检查文件位置时一次读取了不存在的 `ops/project/cleanup_tests.rs`，随后以 `rg --files` 定位到 `ops/project_cleanup_tests.rs`，没有据读取失败改动代码。

当前结果是 Windows 的专项验证；本 PR 的跨平台结果以 CI 为准。无用户可见 UI、依赖或第三方素材变化；测试图片为代码自行生成。
