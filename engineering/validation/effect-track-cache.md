# 效果参数轨道缓存的借用查找（#134）

## 范围与协调

本批处理 #134 的参数缓存键分配与展开曲线的重复解析，基于 main `8a722097` 创建独立分支 `codex/effect-track-cache`。当前 main 已把历史审计中的两次字符串键构造合并成一次，但每次命中仍创建 String；实际提取的原实现以分配探针复现 24,000 次分配后再优化。

开始前检查完整开放 PR、相关分支、所有工作树对此文件的未提交改动。三个工作树的同文件差异分别是旧样式、效果菜单和目录缓存；没有覆盖轨道缓存。已有 presenter-source-reuse 工作留给原分支，未重复实施。#424 摄影机 PR 与本批独立。已核对 `PROVENANCE.md`、`ATTRIBUTION.md`、`NOTICE`，没有新增依赖或第三方资产。

## 实现与边界

缓存按 `(object, effect)` 分组，参数名通过内层 `HashMap<String, EffectParam>` 的借用 `&str` 查询；只有成功加载缺失参数才持有其字符串。参数行复用同一引用完成导航与采样，不再为缓存命中构造字符串或再次查询整条组合键。展开曲线使用已解析的 `curve` 轨道，关联 p0002 通道也走同一按需缓存。

保留既有 `(root, composition, revision)` 失效位置；改变帧不会清空作者轨道，每次绘制仍在当前图层局部时间采样。复用的是轨道定义，不是上次采样结果。缓存保存的实例与参数范围、清空和控件生命周期沿用原规则；二级表的实例开销未单独测峰值内存。

没有改变命令、历史、采样钳制、属性筛选、布局、样式 token、控件类型、点击区域或菜单路由。曲线对象依旧按既有规则选取当前键，采样和原始错误路径保留。其他键（曲线展开状态、枚举控件、表达式目标）、整个面板和完整帧仍有分配；不声称零分配 UI、FPS 提升或完成整个 #134。节点卡片、工作区布局、目录缓存等其他子项仍需独立核验。

## 验证

`desktop/app/tests/effect_track_cache.rs` 直接编译生产缓存模块，并在独立测试二进制内使用线程局部计数、底层仍为 System 的分配器；不改变应用分配器。200 个参数预热后连续查询 120 帧：

| 同一探针 | 缓存命中分配 | 加载/隔离检查 |
|---|---:|---|
| 提取后的原组合 String 键实现 | 24,000（零分配断言按预期失败） | 另 1 项通过 |
| 借用参数名的二级缓存 | 0 | 2 项通过 |

探针测的是生产缓存查询，不是整个面板帧；预热、参数名和夹具分配不计入命中窗口。另核对同名参数在不同对象/效果实例间隔离，失败加载保留原始错误且不缓存错误，成功恢复及 clear 后重载。失败基线与优化结果保存于 `.tools/effect-track-cache/baseline-allocations.log`、`optimized-allocations.log`。

新增真实控件回归在 `effect_controls/track_cache_tests.rs`：使用桌面 Engine 加载内置 brightness_contrast、curves 夹具，单独改变 root、composition、revision，实际 render 后逐值核对标量键、曲线键和通道均已更新；仅改变帧时通过数值行的真实读数检查 2 帧图层偏移与子帧插值。现有 CI 的 desktop `cargo test -p motion-desktop ...` 自动包含独立分配探针，不需要新增工作流步骤。

最终定向验证 **56 通过、0 失败、0 跳过**：效果控件 24（含新上下文/局部时间测试，以及真实曲线/颜色/导航失败的默认文件日志、Error stderr 与 stdout 隔离检查）、效果时间轴 8、属性导航 9、数值事务 2、已修改属性筛选 11、独立分配探针 2。生产编译检查通过，普通/测试构建分别只有 19/18 项既有桌面警告。来源检查覆盖 1786 tracked paths，样式消费者、格式与差异检查通过。没有把定向结果扩大为全桌面通过；完整远程 CI 仍待 PR 运行。

发布前接入新主线 #423（`d961d8db`）及已合并 #424（`1021adf0`）。独立集成工作树的组合树 `64f3338819334bf5ea2d641e7f1f6f2d83ed466c` 完成 **67 通过、0 失败、0 跳过**：上述 56 项再加摄影机 11 项，包含 64 张原生中英/DPI 截图和真实默认错误信道复验，分配探针仍为 0。实际 #424 squash 合并树与预演的 `1ef47f0cd2856a8d49838074ef0d5a3003cfc64a` 一致；本分支重放到该主线后也与已验证组合树逐值匹配。日志和图片在 `.worktrees/camera-cache-main-integration/.tools/integration/` 与其 `artifacts/`。来源检查覆盖 1805 tracked paths，样式和差异检查通过。之后仅补充本完成记录，没有再改产品代码。

## 错误与观测审计

| ID | 分类 | 处理与证据 |
|---|---|---|
| EC-CACHE-MISS | 正常分支 | 缺失是按需解析；成功后缓存，失败返回原 Err，未缓存错误、吞掉 panic、改变返回值或加入重试。 |
| EC-CACHE-DECODE | 既有观测 | 保留 effects.parameter、effects.curve_track、effects.curve_channel_parameter 的原始原因与 object/effect/param 上下文。曲线不再重复解析已由同一快照成功载入的参数；缺失必需曲线仍由原观察点报告。 |
| EC-CACHE-SAMPLE | 既有覆盖 | 当前帧采样仍在缓存外，effects.parameter_sample 与曲线通道的 sample/合法性观察点保留。没有新增健康帧日志或文件 IO。 |
| EC-CACHE-SCOPE | 既有规则 + 新验证 | 工程、合成、revision 任一变化清空，帧变化重新采样；错误和正常失效分开处理。实例/参数来自当前工程，保留范围不随帧数增长。 |
| EC-CACHE-WORKER | 协议边界 | 没有新线程、handler、in-flight 状态、超时、重启或 GPU 资源；原有 Engine 非 Present start/完成/错误信道不变。 |

## DESIGN §0.7 与 §12

| 基线行 | 状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：本批没有路径面板改动。 |
| path_controls 两列硬编码 | retained with reason：没有路径几何变化。 |
| path_controls 按钮/字段高度 | retained with reason：没有路径控件或命中区变化。 |
| path_controls 自绘数值底色 | retained with reason：没有路径字段变化。 |
| path_controls 布尔宽按钮 | retained with reason：没有路径输入变化。 |
| effect_controls 行高/步距 | retained with reason：当前为既有 effect.row.height/header.height 等 token，本批只改数据读取。 |
| effect_controls 布尔按钮 | retained with reason：当前已使用 Checkbox，本批保留。 |
| effect_controls 枚举按钮 | retained with reason：当前已使用 Dropdown，本批保留。 |
| effect_controls hover alpha | retained with reason：当前使用主题 Hovered 状态，本批没有绘制变化。 |
| effect_controls 缺少 depth | retained with reason：当前已有树缩进与效果展开，本批没有树结构变化，也不声称全项视觉整改完成。 |
| timeline 11 列坐标 | retained with reason：没有时间轴布局变化。 |
| timeline snap guide 颜色 | retained with reason：没有吸附绘制变化。 |

§12 相关界面行为由真实控件和既有导航/数值/曲线回归覆盖；本批没有新增视觉输出，不将本次数据缓存测试扩大为所有面板、DPI、工作区或物理键鼠验收。
