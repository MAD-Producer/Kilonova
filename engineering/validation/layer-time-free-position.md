# #257：自由拖动图层的位置寻址

接续 `9027bbaa`。开工核对开放 PR 和近期本地分支，跟踪与预渲染没有占用本轮范围。

## 功能

自由拖动按下时捕获连续空间键 ID，并经 Composition、主窗／浮窗、串行 worker 传给宿主。后续预览按原 ID 修改，Roving 重排时间后不按旧播放时间插入新键。宿主以原键值累积位移，三维投影使用该键的实际祖先变换，而不是重排后的播放头样本，避免位移漂移。

变速或倒放图层的非空间位置也使用精确 TrackAt，包含分离维度，不再按合成帧取整写入。旧的无映射离散轨道仍走原路径。目标删除、时间上下文变化、迟到松开保留正常取消；未改变视图来源／revision 的失效协议。

## 验证

最终同一二进制 53 个不同用例通过。分组结果与重复用例去重见[清单](layer-time-free-position-evidence/manifest.json)，原始输出、源码和失败记录见[证据包](layer-time-free-position-evidence/evidence.zip)。

新 worker 手势覆盖二维／三维、普通／正放／倒放、固定／Roving、提交／取消共 24 种组合，逐次验证三次预览、独立重建场景后的实际屏幕位移、四个原键 ID、完整工程的非目标状态、保存重开及一次撤销重做。另验证正放／倒放和分离维度的局部帧及精确值。

Windows 显式原生 GPU 验证中文主窗二维正放、英文浮窗三维倒放，共六张修改前／后／独立进程重开截图；原有三维轴手柄原生用例也重新通过。原生自由位移用例按产品既有 Alt 语义关闭吸附，保留严格位移断言。两项其他既有手工原生用例仍未运行，不宣称全平台或所有交互验收。

带 FFmpeg workspace all-targets 检查通过，随后仅调整新增测试的手势输入和比较时机；最终测试程序重新编译并运行。样式消费者、改动小模块格式和 diff 检查通过。本轮没有布局、颜色、尺寸、图标、命中区域、工程格式、依赖或外部素材改动。

## 观测与首轮记录

| 项目 | 分类、证据和限制 |
|---|---|
| FREE-UI | 扩展现有 transform.spatial_key_begin／continue 捕获与取消路径；观察器和生命周期累计计数不变，前阶段阈值证据保留。没有新增按帧日志。 |
| FREE-HOST | 新的缺失键错误保留 object、key 与原始原因，沿用串行 worker 的请求 ID、非 Present Info start 和 Error 信道。真实注入分别经过普通 drag 和 viewer_drag，验证完整工程／revision／history 不变、默认文件与 stderr 各两个 Error、stdout 隔离、健康恢复及取消回滚。 |
| FREE-NORMAL | 过期视图请求在解析键前正常取消，不报缺失键错误。目标消失和迟到松开不视作故障。 |
| FREE-BOUNDARY | worker 的阻塞在途状态、panic hook 和超时基础设施没有改动；本轮不把成功测试当作这些边界的新证明。 |

首次命令误写 Cargo 包名，未开始编译；正确包名为 motion-desktop。首次编译发现新增测试把有符号键帧与 u32 比较，修正夹具类型。随后一次构建会话句柄消失、系统也无对应编译进程且日志没有终态，不计通过，保留日志后重新构建至记录退出码 0。

错误信道新用例首次在活跃手势中比较完整 history，被自然增长的 gestureElapsedMs 触发。改为在开始手势前注入拒绝并严格比较原有全部字段，再开启手势验证恢复和回滚；没有删除比较字段、放宽断言或修改产品耗时记录。

原生自由拖动首次偏差来自现有整数吸附。新增夹具改用已有 Alt 关闭吸附；未扩大容差，未修改产品吸附、阈值、命中区域或原有轴手柄断言。原始日志、失败源码与二进制哈希均保留。

## CI 与剩余工作

基准提交 `9027bbaa` 的全部必需 CI 已通过，见清单链接。此前表达式预算失败未在本地复现，不能宣称修复了其根因；本轮没有改预算或表达式实现。新提交需要独立 CI 验证。

#257 保持打开：伴随锚点补偿的连续空间键、真实媒体预热／设备／高倍速音频及最终集成仍未全部验收。空间曲线复制粘贴、空间值方向拖动和 Roving 区间缓动保留既有边界。前阶段被外层中断的整组 graph_ranges 仍不计通过。

## DESIGN §0.7 与 §12


| 基线位置 | 状态与原因 |
|---|---|
| path_controls.rs 六处行步进 | retained with reason：未改路径步进。 |
| path_controls.rs 双列分割 | retained with reason：未改列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：未改按钮和命中范围。 |
| path_controls.rs 自绘数字背景 | retained with reason：未改数字绘制。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：未改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：未改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：未改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：未改效果树。 |
| timeline.rs 十一列坐标 | retained with reason：保留原列坐标。 |
| timeline.rs 吸附线颜色 | retained with reason：未改吸附绘制。 |

§5.2 没有新增属性行或缩小命中区域。§12 仅完成上述 Windows 两种窗口／语言用例；全平台、DPI、全部布局与面板视觉整改仍 pending verification。
