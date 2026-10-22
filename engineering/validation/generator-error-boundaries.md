# CPU 生成器错误边界（本地完成，远端待验证）

继续处理 #138、#135，基于本人 #421 的 `74c1eb56`，使用独立 `fix/native-generator-errors` 分支。开工前检查 138 个本地分支的候选路径及提交元数据：scene_generator 和新增用例没有近期重叠，effect_plan 与资源预算分支有重叠，因此后者只修改生成器错误包装，不改资源策略、尺寸、预算、像素或编码算法。没有审阅他人的 PR，也没有编辑其他任务的工作目录。

## 原始证据与功能边界

真实原库的公开 `scene_generator::validate_settings` 接受可公开构造的 `SampledEffect`。当 rate 参数位于 32 项固定值数组以外时，真实调用在索引 32 处 panic；健康输入成功，原有非法设置拒绝正常返回。独立探针使用 catch_unwind 记录原始 panic，不改变生产行为；原 stdout、stderr、输入与库 SHA 在 `artifacts/pr-ci-followups/generator-error-baseline/`。库 SHA 为 `3008e54263bc30a6c14270ba0ff6a8c4ec3365d471561c4d671cacc57d76edbd`，不能把这次公开畸形对象调用表述成正常 GUI 已能构造或复现的崩溃。

| 编号 | 分类、实际变化 | 限制 |
|---|---|---|
| G1 | 功能修复：参数索引使用可失败查询；缺失和越界均返回原生 RenderError，不伪造参数值 | 仅覆盖生成器自己的查询，不声称 SampledEffect 全部消费者均已审计 |
| G2 | 原生错误传播：SceneSettings 验证保留 motion_effects::Error；settings/generate 包装带图层、效果实例、阶段和实际帧 | 正常裁剪、零出生数、源层未相交等仍返回原来的正常结果 |
| G3 | 功能边界：effect_plan 使用带 source 的诊断包装，保留完整原因链 | 没有修改预算、预检顺序、缓存/计划算法和渲染输出 |
| G4 | 既有失败回滚保持：generate 失败只截断本次追加输出；新增用例验证此前输出保留及健康恢复 | 不添加生产重试、重启或 panic 捕获 |
| G5 | 既有协议边界：particle_history_error 和 particle_emitter::generate 仍为文本，在明确旧边界转换到 RenderError | 不冒充这两个内部模块已全部类型化；其他 CPU 子模块仍待独立核验 |
| G6 | 功能修复：遮挡 AlphaImage 拒绝零尺寸、像素不足、乘法溢出和非有限采样坐标；保留 asset ID、生成器所有者和实际帧的原生原因链 | 超长 backing 像素保持原先接受行为；没有伪造全透明/全不透明结果。双线性运算顺序保持，索引使用已验证的 usize 范围 |

## 默认信道与观察

没有新增日志调用、观察键、计数器或每帧 IO。CPU helper 的 Err 由既有 planner、host、worker/respond 传播；健康 Present 的日志策略保持。新增原生子进程用例拟从实际公开生成器拒绝开始，检查原生 Effects/Invalid 身份、宿主包装、生产 respond/mpsc 回复、默认 Info 文件、Error stderr 与 stdout 隔离，并检查健康恢复。该用例使用手动 Flight，不能冒充完整 worker 调度、start/in-flight 发布或受控阻塞验收。

## 发布草稿时的验证记录与剩余验收

- 第一轮新集成测试因错误原因遍历辅助函数缺少显式借用生命周期而编译失败；原日志及源码不变收据保留，仅修正测试签名。
- 第二轮 1 项通过、2 项失败：新正例沿用归档 SDK 的默认大范围星场，128 像素测试画布中没有可见输出，触发新测试原有的非空前提断言。没有删改该断言，改为在真实 SDK 允许范围内明确设置零范围、零速度、固定出生率与可见尺寸，继续执行原来的输出相等、不变性及原生原因断言。未改变生产参数默认值或出生算法。
- 修正后 3 项集成测试实际通过：四个越界位置返回上下文；实际 planner 的坏设置保留原生 Effects 原因；先前输出与工程不变，修复输入后恢复完全相同的精灵数组。专项源码在运行期间保持，后续全套中不重复计数。
- 工作区所有目标、完整渲染、桌面默认回复信道、FFmpeg 产品构建与默认原始日志留样继续验收；不能把前述 3 项当作全部验收。
- 继续对同一公开 generate 边界执行真实探针，确认 AlphaImage 空像素、短像素、零宽、零高分别 panic，共四次；健康输入前后均成功，源码与依赖在探针期间不变。原始工件在 `artifacts/pr-ci-followups/generator-alpha-baseline/`。没有把正常 GUI 可达性当作已证明。当前回归正在运行，Rust/Cargo 输入保持冻结；该缺口尚未修复，将在当前流水线终止后补上，不把本轮阶段性通过称作整个生成器边界已完成。
- 上述第一轮完整流水线只完成工作区所有目标检查；渲染日志停在 preview_pipeline 用例，之后工具句柄消失，CIM 两次核对原 Python/Cargo 进程均不存在，未产生终端结果。未将已输出的部分通过计为全套成功；源码在进程消失时与输入收据一致。原因未证实，原日志及 `generator-error-regression-interrupted.json` 保留。确认没有该工作目录的运行进程后才追加 G6，并为新源码重新验证。
- G6 前已保存真实健康公开 generate 输出：4 种图片 backing、11 个位置、3 种遮挡半径，共 132 组，精灵各浮点字段原始 bits 及统计量共 19460 字节，SHA `58aae57ae017261e63455f45d662af53bd7425571741c1e7a6b1be4ea2165cf1`。旧库、源码、依赖收据不变。该工件是 CPU 精灵数据对照，不是全部像素基线重跑；新实现逐字节对照尚待执行。
- 新增真实公开入口用例覆盖前述四种 panic 输入、极大尺寸及非有限 UV；验证返回原生 Invalid、asset/owner/frame 上下文、输入与此前输出不变，以及恢复完全相同的非空精灵。未相交源层不要求 alpha 缓存，仍是正常分支。实际默认信道将覆盖两种设置拒绝和六种 alpha 拒绝，共八个原因；目前这些新增验证尚待完成。
- G6 追加后的五项生成器集成测试实际全部通过（41.172 秒，源码收据不变），包含新增图片与非有限坐标拒绝、既有 planner 原因和输出恢复。随后独立新旧原生库的 132 组健康输出 19460 字节逐字节相同，SHA 仍为 `58aae57ae017261e63455f45d662af53bd7425571741c1e7a6b1be4ea2165cf1`；不是用新实现计算预期值。新旧库及二进制 SHA、原始 stdout/stderr、输入收据分别保留在 generator-alpha-healthy-baseline/fixed。五项专项会纳入后续完整渲染回归，不重复计数；默认信道、所有目标和产品构建仍在执行。
- 追加后的所有目标检查实际通过（139.938 秒），桌面真实失败信道两项通过（137.672 秒，包含既有缓存计划错误与本批八种拒绝），收据中的 Rust/Cargo 输入均不变。这两项是信道专项，不冒充完整桌面回归。
- 独立默认日志留样实际成功：两个真实设置拒绝和六个真实 AlphaImage 拒绝，经原生 host 转换、生产 respond 和 mpsc 完整回传。日志文件 Error 八条、stderr Error 八条、原始原因八条；每条都有 request 901–908、operation、owner、effect、frame、elapsed 和原始原因，stdout 隔离。恢复后输出与健康原样相同。原始工件在 `artifacts/pr-ci-followups/generator-error-default-output/`，测试二进制 SHA `7dc6d3f2315ce5ac5311eadd31ddf318344b2167fc3966690dd5dccc9e7fe06b`，源码和二进制在留样期间不变。手动 Flight 的范围限制仍适用。

完整渲染回归与 FFmpeg 产品构建在草稿发布时仍在运行。最终结果、远端检查、实际合并树及剩余边界须在该 PR 的完成记录逐项补充；本文前述阶段性数字不能代表这两个门禁已经成功，也不与其他源码批次累加。

本批没有面板代码、样式、资产、工程格式或 UI 行为变化。完成后仍须保留 #138/#135 开放：其他 CPU 文本边界、模型构造不变量、DecodeTask 退出和旧协议不属于本批已完成范围。

## 本地最终结果与远端基础设施跟进

追加 G6 后的四阶段已全部终态成功：工作区所有目标 FFmpeg/host diagnostics 检查 139.938 秒、桌面信道 2 项 137.672 秒、完整渲染 448 项 1369.922 秒，以及带 FFmpeg 的 Debug 产品构建 70.829 秒。合计 450 项通过、0 失败、1 项既有外部图片忽略；五项专项包含在 448 中，默认留样不另计。各阶段 Rust/Cargo 字节收据保持，完整结果为 `artifacts/pr-ci-followups/generator-alpha-regression-results.json`。这些是本地原基底的结果，不能冒充主线实际组合的远端门禁。

远端 run `39786337607` 的四项审计/桌面门禁成功，但 Rust/GPU 两次均在原 45 分钟预算取消。第一项任务 `119850447755` 的依赖安装共 32 分 42 秒，其中 108 MB 下载用 32 分 8 秒；第二项 `119878502401` 的依赖安装共 25 分 3 秒，其中 83.6 MB 下载用 24 分 37 秒（56.6 kB/s）。两次完整原日志、步骤时间、终态及 SHA 均保存，取消不计为测试通过，不能把没有已输出断言失败当作全套成功。

确认第二项终态后，仅在现有三个 Ubuntu 安装入口，将 runner 默认的 `http://azure.archive.ubuntu.com/ubuntu` 改为 Ubuntu 官方 `https://archive.ubuntu.com/ubuntu`。保留 deb822 源的 Signed-By、包列表、其他源、已有命令和原任务期限；没有产品重试、测试重试包装、断言变化或新增忽略。这是独立的 CI 下载源修复，不是生成器功能修复；官方源在新 runner 上的实际耗时与全套 CI 仍须新提交验证。参考 Ubuntu 官方 [归档签名说明](https://documentation.ubuntu.com/security/software-integrity/archive-verification/) 和 [软件包管理说明](https://ubuntu.com/server/docs/how-to/software/package-management/)。

本地应用辅助程序首次因候选 LF 与工作目录 CRLF 的严格字节保护断言退出，发生在写入之前；修正辅助程序以保留原换行，并验证移除三行后与原文件逐字节相同。原失败保留，生产代码与测试断言未因此改动。后续生成器非有限数值、出生编号和矢量原生错误在本人合批分支单独完成，尚未将其算入本 PR。

下载源修改后，现有工具测试实际运行 71 项，70 通过、1 项既有 Rust effect_tool 配置跳过，退出 0；工作流字节在运行期间不变。初次裸 Python 环境缺少 yaml 导致两个测试模块导入错误，原结果保留；加载已有 `.tools/build-validation-python` 的固定 PyYAML 6.0.3 后执行同一套测试，没有安装新依赖或改变测试断言。对应原始 stdout/stderr 与收据为 `generator-ci-source-tool-tests.*`。这只证明本地工作流与发布路由约束，不能证明托管 runner 下载已恢复。
