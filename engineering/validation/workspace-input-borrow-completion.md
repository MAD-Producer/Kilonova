# 主窗面板只读输入借用验收

Refs #129、#158、#134。基线 `af1e7f9f07dceb03d8f078889505adf1db78542f`，代码 `a17139eaff9a7b91730762a96848558445155fac`。最终主分支组合须按 PR 当前提交核验。

## 两行改动

chrome 在分发面板前将 body_input.clone() 存入 project_input；该变量其实用于所有普通面板分支，并非仅项目面板。原输入及局部 neutral 都活到 chrome 结束，之后没有修改它们；render_panel 本身仍建立局部输入并按焦点和区域过滤。现在直接传 body_input，删除这份多余的整帧复制。未改窗口输入归属、模态分支、跨帧快照、按键过滤、控件、样式、尺寸、命中区或协议。

## 实际前后对照

在完整 chrome 路径执行 128 组场景：中英文、900×600/1600×1000、主窗/浮窗、弹层开/关、正常/组合输入/IME consumed/失焦，以及空载荷或 81920 字节文本压力载荷。每组先热身 3 帧，再测 10 帧，共 1280 个测量帧。128 份完整绘制、动作、控制器与工程快照逐字节相等；每帧输入和工程不变，未发编辑动作。

分配请求减少 800 次、申请字节减少 13110400；所有场景未增加。64 组浮窗作为未改路径的对照，分配量和快照均未变化。数字包含压力文本，是被测线程的分配请求及申请字节，不是释放/存活内存，不推断典型帧、RSS、CPU 时间、GPU 时间或 FPS。每个面板内部仍有自己的输入快照，本项不宣称整条输入链路零分配或所有 #129/#158/#134 事项完成。

前后均核对实际桌面编译路径、探针/源码冻结；临时测试模块与分配器已经移除，原 tests.rs 按字节恢复。没有新增永久测试或修改已有断言。最终产品源码另行实际编译后，563 项面板通过、1 项上游手动拖动捕获保持忽略；164 项节点、3 项 F9/真实写入恢复协议通过。原有手动曲线 GPU 用例另行显式执行，1 项通过，13 张完整中英文、默认/最小窗口、值/速度、弹层及浮窗截图与既有基线逐字节一致。各阶段程序哈希及源码均一致。

## 观察、故障与限制

- WB-01 新观察：无。正常输入、空输入和模态过滤不新增日志、文件 I/O 或错误路径。
- WB-02 既有覆盖：日志/错误/取消信道未改；现有故障回归保留在完整面板、节点和协议运行中。没有把正常绘制或绿色计数当作所有观察站点的独立故障证明。
- WB-COMP-01 工具链故障：初始 after 编译的 rustc_driver 在测试开始前以 0xc0000005 退出；Windows 事件和原始日志保留，该次没有 runtime 结果。核对源码未变后做了一次同源码、同编译参数的诊断构建，仅增加 RUST_BACKTRACE 诊断环境，随后实际编译及测量通过。根因仍未确定，不称为工具链或产品修复，未改产品重试、超时或断言。
- WB-03 环境与待验：真实 GPU 日志保留本机可选 Vulkan overlay 注册文件的既有 Error，未宣称环境无 Error。生产 Input 注入不等同于系统物理输入或完整 IME 验收。未执行原 40/90 秒完整桌面脚本，不宣称整个 DESIGN §12 完成。
- WB-04 来源：实施前核对 443 个引用、53 个文件版本、131 个工作树，未发现相同的借用路由。无新增依赖或第三方资产；其他有意保留输入所有权的位置不改。三个集合 issue 保持开放。

## DESIGN §0.7 逐项处置

| 基线条目 | 状态 | 原因 |
| --- | --- | --- |
| path_controls 六处行高 | retained with reason | 未改行节奏 |
| path_controls 双列宽度 | retained with reason | 未改列宽 |
| path_controls 按钮高度 | retained with reason | 未改尺寸或命中区 |
| path_controls 数值背景 | retained with reason | 未改绘制 |
| path_controls 布尔控件 | retained with reason | 未改控件或事件语义 |
| effect_controls 行高与步进 | retained with reason | 未改效果行 |
| effect_controls 布尔按钮 | retained with reason | 未改开关 |
| effect_controls 枚举按钮 | retained with reason | 未改下拉 |
| effect_controls 悬浮 alpha | retained with reason | 未改颜色或状态 |
| effect_controls 缩进层级 | retained with reason | 未改属性树 |
| timeline 标签列坐标 | retained with reason | 保留兼容基线 |
| timeline 吸附引导颜色 | retained with reason | 未改颜色或 token |

## 证据

[data/workspace-input-borrow-af1e7f9f.zip](data/workspace-input-borrow-af1e7f9f.zip)：3105219 字节、78 项，SHA-256 `b402436d0c5d8fe160319da36ec99faad7c148998f88deace205309c42a7e949`。快照仅按内容哈希去重，不截断或重写原始字节；snapshot-manifest.json 为两阶段 256 个场景记录分别保存 SHA、字节数和对象路径，全部可还原并逐字节比较。相同输出共用原始对象，共 28 个对象；实际测量文件在本地保留。包中同时保存原始编译/测试命令、成功/失败日志、Windows 事件、最终源码和 13 张 GPU 截图。结构化结果见 [workspace-input-borrow-results.json](workspace-input-borrow-results.json)。
