# #257：摄影机与镜头参数的精确时间编辑

接续 `85cf1660`，交付至草稿 PR #579。开工核对开放 PR 与最新分支：#575 负责
原生预渲染，#580 负责外部跟踪／校准相机导入，没有发现摄影机参数输入的重复工作。

## 功能变更

摄影机位置、目标、标量、秒表及重置入口复用 TrackAt，将合成时刻解析为图层
属性时间。映射图层不再要求合成播放头是整数；未映射摄影机保留原有整帧门禁和
近整数约定。整数键仍要求整数局部身份，不把正常不可用能力当作日志故障。
静态参数和关闭动画时的精确采样不需要创建整数键。

`camera_lens.frame` 从 u32 改为 f64，公开 schema 同步为 number；它表示合成时间，
胶片宽度和 FOV 继续在一个候选事务中提交。复用既有属性时钟、范围与锁定校验，
不另建动画通道、不改变渲染器的光学公式。现有 Rust 测试的整数 helper 显式转为
f64，一处测试命令字面量改为 `0.`；既有断言未修改。

数字与镜头手势持有文档代次、播放头和属性时钟身份。上下文改变时取消整个预览，
不允许旧手势在新的时间映射上继续写入。正常失焦／取消沿用单次撤销事务。

观察摄影机工具只改变宿主视图，不修改 authored 相机，既有代码已使用 f64 播放头；
本阶段不重写它，也不将它当作摄影机文档编辑的替代实现。

## 验证

最终 33 个不同测试通过：新增桌面映射 5、既有摄影机参数 11、宿主观察摄影机
7、既有核心镜头 7、新增核心时间映射 2，以及显式原生验收 1。本阶段选定测试
没有尚未运行的 ignored 项；不表示整个仓库的人工验收已经完成。

8 组主窗／浮窗、中英文及正放／倒放原生场景保存 40 张截图，覆盖提交位置、
镜头、右键重置及取消。8 份工程在独立进程重开后完整状态一致。普通参数、焦距／
胶片／缩放、重置、静态子帧编辑、精确关闭动画、整数插入禁用、整次撤销与失败
批量回滚均通过完整工程比较。既有近整数门禁、观察工具和四档 DPI 原生用例也
回归通过；新截图仅覆盖实际选定布局，不能外推为全部 DPI 或平台合规。

所有桌面组使用同一可执行文件，逐组核验运行前后 SHA-256；核心两组分别核验
对应二进制哈希。12 个改动 Rust 文件与最终编译源码哈希一致。带 FFmpeg 的
workspace all-targets check 通过（37.15 秒），样式消费者、新增模块 rustfmt
和 git diff --check 通过。详见
[清单](layer-time-camera-authoring-evidence/manifest.json) 及
[证据包](layer-time-camera-authoring-evidence/evidence.zip)。

新增工程夹具最初将 Camera 直接放入 Content::Camera，
而当前模型要求 Box；修正两个新夹具的构造，保留初次编译失败。首次运行还证明
把 Solid 的变换继承给 Camera 会违反已有模型不变量；两个新夹具改用正式
`Layer::camera` 构造器，保留原始失败输出，没有删除模型门禁或修改既有断言。
镜头新增期望值最初跳过了协议已有的 f32 输入转换，与实际 FOV 相差一个 ULP；
完整差异确认只有该值，期望构造改为先执行协议的 f32 转换。仍使用完整工程精确
相等断言，没有改为容差比较，也没有修改生产光学公式。

## 可观测性审计

| ID | 分类 | 处置与证据 |
|---|---|---|
| CAMERA-T-01 | 功能修正 | 精确时间寻址、镜头协议与原子提交、手势身份守卫分别验证；不是仅添加日志。 |
| CAMERA-T-02 | 新观察 | 固定 `camera.numeric_time` 站点包含对象、合成帧、prepare 阶段和原始原因；失败时禁用本次编辑并取消已有预览。 |
| CAMERA-T-03 | 观察范围 | 沿用面板进程累计饱和计数，首次 Warn、第 60 次 Error、后续抑制；健康调用不复位，新进程重置。固定键与 4096 字符上下文上限不变，健康路径无文件 IO。 |
| CAMERA-T-04 | 既有覆盖 | worker 的非 Present Info start、在途状态、原子候选校验、错误文件／stderr 和原 panic hook 不改。 |
| CAMERA-T-05 | 正常分支 | 非整数局部插入、只读／锁定、正常取消和空历史不是故障。无重试、重启、清错或吞 panic。 |
| CAMERA-T-06 | 实际异常验证 | 默认日志等级下，120 轮无效文档代次穿插健康绘制，文件只保留首次 Warn 和第 60 次 Error，stderr 仅对应 Error，stdout 隔离。健康调用未复位进程累计。另向实际 worker 提交先成功后失败的镜头批次，验证 Info start、原始 integer_key 原因、文件／stderr 错误及完整工程回滚；原有镜头／采样故障信道也回归通过。 |
| CAMERA-T-07 | 剩余范围 | 连续空间旧入口和浮动键补偿、真实媒体预热／设备／高倍速采样及全分支最终验收仍需推进，#257 保持打开。 |

## DESIGN §0.7 逐项处置

本阶段没有新布局、颜色、尺寸、图标、第三方依赖或素材。下列基线均 retained，
各项独立视觉合规仍待验证；retained 不表示已修复。

| 基线 | 本阶段处置 |
|---|---|
| path_controls.rs 六处字面行步进 | retained with reason：不改路径行步进。 |
| path_controls.rs 双列分割 | retained with reason：不改路径列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：不改命中尺寸。 |
| path_controls.rs 自绘数字背景 | retained with reason：不改数字绘制。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：不改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：不改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：不改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：不改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：不改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：不改效果树。 |
| timeline.rs 十一列坐标 | retained with reason：不改既有坐标。 |
| timeline.rs 吸附线颜色 | retained with reason：不改反馈和主题。 |

§5.2 没有增加属性行或缩小命中区域。§12 不能由本阶段参数交互证明全局布局、
全部平台和 DPI 合规；原有摄影机行控件的视觉问题不算作本次修复。实际浮窗截图
仍可见数字底部裁切及模式文字省略，没有通过本阶段顺带声称这些视觉问题已修复。
