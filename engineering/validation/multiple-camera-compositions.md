# 多摄影机的嵌套合成回归

关联 #249；基于 main `92a2327`，本次仅增加测试，不重复实现已合并的多摄影机功能，不修改 UI 或运行时错误处理。

## 分支核对与剩余范围

开始前检查本地 worktree、近期分支提交、开放 PR 的文件清单。#349 正在处理 U/UU 属性筛选，#344 正在处理采样/曝光及其 CI，#345/#347/#348 分别涉及文字边界、借用输入和菱形边界；node-gestures 有未提交的路由改动。已有 `fix/multiple-camera-layers` 的实现已在主线，当前没有其他开放 PR 处理本次新增的 `multiple_camera_compositions.rs`。

既有证据见 [多摄影机图层](multiple-camera-layers.md)：模型 ID、单相机兼容、创建/复制/重排/删除、活动区间、锁定、父子关系、表达式、模式切换、保存及撤销，以及原生双语界面。#249 还要求明确嵌套语义，因此新增不依赖属性面板的专项。

## 新增断言

- 父合成 30 fps、子合成 60 fps，引用 source_start=12、offset=3，独立计算子时间 `12 + (parent - 3) * 2`。两个作用域故意重复使用相机 ID 1/2。
- 子相机活动区间 [18.5,42.5)，动画局部 offset=5；覆盖边界前后、反向随机寻帧、锁定相机仍活动，父相机的 [8,20) 独立切换。比较实际 Scene 相机矩阵、子时间与不产生像素的相机层。
- 保存重开后，用独立显式选定的旧式单相机作为父/子参考，不调用 active_camera_layer 选择参考视图。真实 GPU 图像逐字节比较；版本化 composition export plan 同样逐字节比较。主动换错父相机、子相机均须改变真实像素，防止无效夹具带来假通过。
- 子相机全部禁用时，分别验证既有旧相机及隐式默认相机，父视图不受影响；往返切换不污染场景。

这里的 export plan 对照不是 MP4 编码、异步冻结或实际窗口输入验收。没有给 #249 添加自动关闭指令；其关闭仍需结合原有证据和实际冻结输出覆盖审阅。

## 中间失败记录

1. 首先将 GPU 测试放在 motion-core 集成目录，编译发现该 crate 没有 pollster；移动到已具备真实 GPU 测试依赖的 motion-render，不新增依赖。
2. 原始夹具直接添加相机却保留 Project::new 的基础版本，被格式校验拒绝；显式使用既有格式 19。随后 precise range 与整数 timeline 的 floor/ceil 不一致也被正常拒绝；修正为与公开命令相同的覆盖边界。原断言没有更改。
3. 第一版参考删除了相机层，真实像素断言通过，但 export plan 因对象顺序标识变化而不同。改为保留对象并禁用相机以走旧式视图，保留 plan 的逐字节断言；不能将两个身份不同的文档当成相同协议字节。

日志保存在 `artifacts/property-selection-context/multiple-camera-compositions*.log`。最终 `multiple-camera-compositions-oracle-final.log` 为新增 **3 通过、0 失败**（Windows Vulkan，12.39 秒）；`multiple-camera-compositions-core.log` 为既有 camera_workflows **2 通过**、multiple_cameras **9 通过**，没有修改它们的断言。编译与测试只使用既有依赖。

## 错误处理与来源

| 项目 | 处置 |
| --- | --- |
| MCC-EXIST-01 | 既有模型校验实际拒绝不合法的测试格式与区间；没有清错、放宽规则或更改返回值 |
| MCC-NORMAL-01 | 没有活动相机、旧/隐式视图、正常随机寻帧属于正常分支；没有新增逐帧日志 |
| MCC-BOUND-01 | 本次只增加同步测试，没有新 worker、IO 协议、恢复机制或 UI 反馈；不能声称新增错误信道覆盖 |
| MCC-PENDING-01 | 不把单机 Vulkan 的像素/导出计划对照等同于 Linux/Metal、实际异步冻结 MP4 或物理窗口交互 |

无新依赖、第三方资产或兼容名称；没有改面板或设计 token。既有观察和默认日志策略不变。
