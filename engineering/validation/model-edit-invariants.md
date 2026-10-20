# 模型编辑不变量与错误信道

关联 #138 的模型编辑不变量子项，基于 main `a3ca3bd91a818fb40e0fa1a8919b2129282c4f8f`。本次不关闭整个 issue。

## 分支协调与范围

开工前核对全部开放 PR、近期提交和工作树中的已跟踪改动。#373 正在修改 `motion-core/src/editor.rs`，所以保留其中效果复制后的 `last().unwrap()`，本次不碰该文件。注册表、缓存锁、渲染遍历、快捷键、相机和界面工作继续由原分支负责；不合并、改基或调度积压 PR 的 CI。

独立分支 `fix/model-edit-invariants` 只处理可返回错误的合成与蒙版编辑路径、导入路径的冗余解包，并验证真实桌面引擎的错误传递。没有界面、布局、样式 token、素材、依赖、公开函数签名或工程格式变更。

## 实现与边界

- 合成图建表直接传播 `composition_view` 的结构化错误，不再 `expect`。预合成使用显式非空首尾匹配；引用创建和设置更新后的内部查找失败改为携带合成、对象或阶段信息的 `Error::Composition`。
- 蒙版编辑在需要已有蒙版的分支直接取得可失败索引；缺失时保留 `Error::Invalid`，消息补充有界数值 ID，例如 `mask does not exist: mask=9`。不会使用默认索引、清空错误或把失败变为成功。
- 排序仍先完整校验长度、唯一性和成员关系。之后按 `Option<usize>` 排序；合法输入的键全为 `Some(position)`，没有缺失键回退，不改变顺序或动画数据。
- 工程导入复用前面已经通过 `ok_or` 检查的 `parent`，保留后续 canonicalize 和选定父目录约束。

合成的被替换 `expect/unwrap` 均有当前构造不变量，本次是防御性错误返回，**没有复现当前合法工程可触发这些内部异常**。不把移除断言等同于修复已复现的用户崩溃，也不把正常分支测试冒充新增内部异常分支的注入验证。低层模型编辑仍由现有 Engine 事务提供批次回滚；本次没有为所有直接模型调用增加事务。

## 观察审计

| ID | 分类 | 证据与限制 |
| --- | --- | --- |
| MEI-01 | 功能边界／上下文改进 | 10 种缺失蒙版编辑直接返回原错误类型并携带 mask=9；直接模型不修改轨道，真实核心批次回滚先前成功的选项修改，错误后仍可恢复原重做分支。没有在模型持锁路径新增 logger。 |
| MEI-02 | 既有覆盖／协议边界 | 桌面真实 Engine 与生产 logging 验证蒙版缺失、非法排序、空预合成及缺失合成；默认 Info 下检查对应请求的 start 先于 Error，Error 进入文件及 stderr，stdout 不含日志、错误正文或完整用户文本。结构化合成错误继续由既有 envelope 保留 code。 |
| MEI-03 | 正常路径 | 60 次真实状态查询不得重复记录这四个故障；错误后引擎仍可编辑，原文件字节、项目、revision 和撤销／重做状态按测试保持。实际拒绝的命令逐条 Error，非 Present 健康状态及正常未命中不新增警告。 |
| MEI-04 | 没有新增观察机制 | 沿用原请求编号、start/in-flight、外层失败日志、文件／stderr 分流及响应协议；不新增计数器、无限上下文、超时、重试或线程重启。因此没有新增 60 次阈值或重置策略需要声明。 |
| MEI-05 | 待处理 | `composition_list/references` 的非失败查询、动画采样、曲线路径中的构造不变量及 #373 所在编辑器文件仍保留既有解包。media/host 的 String 错误迁移及全部渲染错误统一不在本次范围，#138 保持打开。新增合成内部异常分支未做可达输入复现或定向注入，不能声称全部不变量均有故障覆盖。 |

## 验证

使用现有 portable MSVC、locked 依赖、E 盘临时目录和 `target/model-robustness-audit`。仅通过本地环境关闭 debug information 和 incremental，保留 debug assertions；没有放宽现有断言、超时或 ignore 条件。

- 首轮既有 `composition_boundaries`：4 通过，0 失败，0 忽略，执行 0.04 秒。
- 新增核心专项：2 通过，0 失败，0 忽略，执行 0.01 秒。缺失蒙版案例为 10 个子场景，不重复计为 10 个独立测试。
- 原生产蒙版实现配合相同新测试：退出 101，诊断未携带 mask ID，断言失败。不是复现原实现缺失蒙版时发生 panic。
- 去掉排序成员检查的可编译故障变异：退出 101，非法成员顺序被错误接受，回归检出。原生产文件由 finally 原样恢复，SHA-256 `ed056638063d041112c8271c545d8e7d625a501ebb6e59100bf3d4dd66c64dbf`。
- 完整 `cargo test -p motion-model -p motion-core --locked -j 1 -- --test-threads=1`：273 通过、0 失败、1 项既有手动大文档性能测试忽略，含 doctest 共 61 组结果，测试执行合计 57.57 秒，编译 2 分 50 秒。没有增加忽略项。
- 随后将合成图建表改为显式 `HashMap::with_capacity` 循环，以在传播错误时保留预留容量。最终版本复核两个包的库测试及 composition_boundaries、composition_candidate_graphs、compositions、composition_fps、composition_resources、layer_masks、package_transactions、model_edit_errors：57 通过、0 失败、上述同一既有手动性能测试忽略，10 组，执行合计 21.16 秒。与完整回归重叠，不相加计数；完整 273 项发生在这次等价建表调整之前。
- 桌面 `engine::failure::model_edit_tests::invalid_model_edits_keep_history_and_reach_default_error_channels`：1 通过，执行 5.25 秒，编译 1 分 23 秒；19 条既有测试构建警告保留，没有为清理警告改动其他界面代码。
- 同一实际测试程序再次直接运行子场景，保留默认级别原始文件、stderr、stdout 及 summary.json 于 `artifacts/pr-queue/model-edit-invariants-default-output/`。四种原始原因分别在文件和 stderr 各一次，在 stdout 均为零；文件确认 `level=INFO`，未记录私有重命名文本。重复执行仅为保留原始证据，不计作新增测试。
- 来源守卫通过（1553 个受跟踪路径），面板样式消费者、两个新增测试的 rustfmt 与差异空白检查通过。没有修改任何既有测试断言。
- `cargo build -p motion-desktop --features ffmpeg --locked -j 1` 非测试桌面构建通过，26.69 秒，21 条既有构建警告保留。远程 CI 尚未执行，不能记作通过。

本机原始证据保存在 `E:/Dev/aem/artifacts/pr-queue/model-edit-invariants-*.log`，故障变异驱动为同目录 `verify-model-edit-guards.py`。当前不宣称跨平台 CI 已通过。
