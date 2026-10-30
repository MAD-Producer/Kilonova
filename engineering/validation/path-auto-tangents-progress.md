# #270 路径自动切线实施记录

状态：本地模型、编辑器、实际冻结输出及默认故障日志验收已完成。合并及关闭 issue 仍以当前提交的 CI 为准。

## 已实现

- `VectorPath` 显式保存 Manual/Automatic 模式。未指定约束时为手动模式，保留可选约束的语义；没有工程迁移器或逐版本兼容分支。现有路径构造点补齐显式默认模式，断言不变。
- 形状和蒙版通过同一个路径采样函数先采位置、再按自动约束生成手柄。存储的节点 ID、关键帧和手动手柄不改；切回手动模式恢复原始手柄。
- 扁平路径、分组几何和蒙版使用现有 Vector/Mask 命令协议编辑模式；原有事务负责锁定、原子失败和撤销。选点片段复制保留模式。
- 首顶点旋转不改变对应轨道或几何。形状/蒙版转换保留模式；经过非等比缩放或 skew 的自动路径明确拒绝有损转换，未采样烘焙动画来伪造无损结果。
- 自动切线接受既有采样坐标上限 131072，作者轨道上限仍由原模型验证。避免把作者数据边界误用于允许过冲的采样结果。算法健康路径无逐节点格式化字符串分配。

## 已执行验证

本地日志：`E:/Dev/aem/artifacts/path-auto-tangents/`。

| 命令/结果 | 证据 |
| --- | --- |
| `cargo test -p motion-model --tests --locked --offline -- --test-threads=1`：30 组，118 passed，0 failed，0 ignored | model-tests-final.log |
| core 的 path_tangent_mode / path_first_vertex / path_conversion / path_clipboard：4 组，18 passed，0 failed | core-path-tests-final.log |
| 桌面 FFmpeg 测试目标编译通过 | editor-compile-final-4.log；追加的故障验收测试编译见 error-proof-compile.log |
| 新增开放/闭合路径各 1002 个连续采样时刻及 2048 点成本观察：6 项目标测试通过（其中 5 项为上述模型回归） | sampling-stability.log |
| 桌面路径菜单、钢笔、选点及真实 worker/剪贴板回归：39 passed | editor-tests-final.log |
| 主窗/真实浮窗、中文/英文、形状/蒙版自动模式和菜单：1 passed | native-tests-final.log；native-final/ 下含 16 张切线相关 GPU 截图 |
| 真实 FFmpeg MP4 冻结导出、后续模式编辑、独立参考和手动负对照：1 passed | movie-tests-final.log；frozen-movie/ 下三个实际 MP4 |
| 实际关键帧过冲经 Seek 返回采样错误，默认文件/Error stderr/stdout 隔离及恢复通过 | sampling-error-proof.log；sampling-error-channel/ 下父/子 stdout、stderr、文件日志 |

新增数学对照覆盖直线三等分、非均匀点距的共线入/出手柄及弦长上界、闭合接缝旋转、反向路径、重复点、极值、最大节点数以及非法输入不部分写入。真实模型覆盖形状/蒙版同帧与子帧采样、手动模式不变、序列化、首顶点及原始轨道保持。真实 Engine 覆盖三个目标的模式编辑、保存重开、撤销重做、锁定/缺失目标、转换拒绝不变性。

## 错误处理及剩余验收

PA01 新边界：`Error::PathSampling` 保留路径 ID、帧与原始 source；测试实际构造非法采样并验证原因链。PA02 命令边界：复用 `PathOperation` 的 `set_tangents` 上下文，实际缺失/锁定目标错误保留 source，工程、修订和历史均不变。PA03 正常分支：重复点、空开放路径、恢复手动模式不作为故障。

PA04 复用的观测通路已验证：真实无效模式目标通过现有 `path_operation` worker，在默认级别将 `set_tangents`、对象和路径信息写入文件/Error stderr，不写 stdout；与既有无效首顶点/复制目标一起验证。另用合法作者坐标及合法时间曲线在第 5 帧产生超界采样，通过真实 `Seek` 返回 `path=3 frame=5` 和 `invalid geometry node=0`，文件日志与 Error stderr 都保留原因，stdout 隔离，随后第 0 帧恢复且工程/修订不变。这不是伪造 Err。非 Present 命令仍在处理前发布 Info start，既有计时与 in-flight 规则不变。没有新增模型内日志、worker、重试、超时或 Present 热路径日志。新菜单的取消/换文档为正常分支；既有钢笔/选点 FailureCounter 的阈值、抑制与恢复测试通过。

PA05 功能与协议边界：编辑投影使用真实自动采样轮廓，同时保留作者几何作为拖动/微移种子。自动模式不绘制、命中或拖动手动手柄；平滑/角点工具提示先恢复手动模式，这是正常限制，不记故障。原节点轨道在自动插点中不变，新锚点按已显示的曲线定位，删除后按邻接重算。手动插点原有保形断言全部保留。主/浮窗四种语言/缩放组合由现有 Shell 菜单、Action、worker 实际派发模式；取消/失焦/换文档/锁定仍使用原 guard。输入是生产控件的归一化输入，绑定真实 HWND/wgpu surface，不声称 OS 注入测试。冻结导出在实时工程已改为手动后仍与独立自动参考的首/末解码帧完全相同，且与手动负对照有可见像素差异。

PA06 首轮新增测试夹具错误：`Track::set_at` 对未动画轨道更新静态值，并不自动创建动画；新夹具已改为明确的关键帧。既有 MP4 解码 helper 固定核对六帧，新夹具原为四帧，已按 helper 契约改为六帧。保留 editor-tests.log / movie-tests.log 原始失败；没有放宽既有或新增断言，没有修改生产代码来迁就夹具。

## DESIGN 处置

新增模式沿用既有右键菜单行及 `context.*` 样式、尺寸 token；没有创建属性行、布尔按钮、枚举循环按钮、图标或缩小命中区。自动模式只屏蔽不可编辑的手动手柄。`verify_style_consumers` 与 `git diff --check` 通过。§12 已核对实际中英文菜单、主/浮窗、锁定/取消及保存/历史；未把编译当作交互验收。

§0.7 的以下每一项均 retained with reason：本提交不整改该属性绘制/时间轴布局，保留现有行为，不宣称该历史审计项已修好。表中旧行号只用于识别基线，不冒充当前行号。

| 基线项 | 处置与范围 |
| --- | --- |
| Path 六处行步进 | retained with reason：属性行绘制未改 |
| Path 两列分割 | retained with reason：数值列未改 |
| Path 混合按钮高度 | retained with reason：属性按钮未改 |
| Path 自绘 SUNKEN/ACCENT 背景 | retained with reason：数值字段背景未改 |
| Path 全宽布尔按钮 | retained with reason：现有启用控件未改 |
| Effect 行高/步进 | retained with reason：效果面板未改 |
| Effect 布尔文字按钮 | retained with reason：效果面板未改 |
| Effect 全宽枚举按钮 | retained with reason：效果面板未改 |
| Effect 手工 hover alpha | retained with reason：效果面板未改 |
| Effect 深度/缩进 | retained with reason：效果面板未改 |
| Timeline 11 列硬坐标 | retained with reason：按基线要求保持列位置 |
| Timeline 吸附线颜色 | retained with reason：按基线要求保持反馈颜色 |

无新增依赖或第三方资产。图像是本程序真实 GPU 捕获；Epic Overlay 的失效 Vulkan JSON 环境消息和 OpenH264 既有警告未隐藏，也未在本 issue 中修改系统或编码器配置。
