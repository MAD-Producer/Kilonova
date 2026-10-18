# 多摄影机图层与旧摄影机可见性修复

## 问题与实现

- CAM-01：`create_camera` 曾以 `Project.camera.created` 拒绝重复创建；旧格式默认 `created=true`，而序列化省略 true，前端把缺失当 false。结果可以出现摄影机不可见但后端报 already exists。现在始终显式序列化该布尔值，保留缺字段旧文件的读取语义。
- CAM-02：新摄影机以格式 19 的 `Content::Camera` 保存为独立图层，返回真实非零 object ID；ID 使用检查加法，受既有 128 图层上限约束。旧 object=0 摄影机、动画和父子关系保留，绝不覆盖。
- CAM-03：按存储栈逆序选择最上方、可见且处于半开时间范围内的摄影机。图层锁定不禁用摄影机。没有活动摄影机图层时使用旧摄影机（存在时）或隐式默认视图；观察视图保持独立。
- CAM-04：属性/关键帧、局部时间、拆分、裁剪、复制、删除、重排、父子矩阵、表达式和粒子出生时的摄影机祖先均识别独立摄影机。摄影机没有像素，拒绝图像效果/遮罩/混合模式、普通源锚点与普通图层缩放旋转。
- CAM-05：`camera_mode`、`pan`、`dolly` 支持 object；缺省 0 兼容旧脚本。新调用者使用 `create_camera` 返回的 object。模式转换按整数帧烘焙，保留隐藏关键帧，超过预算或时间溢出明确拒绝且不发布修改。
- CAM-06：新建入口始终可用，新建完成选择真实摄影机；属性和时间轴使用各自摄影机的轨道和采样值。摄影机属性一行一个属性，向量 inline，模式为显式下拉菜单。既有数值命中高度 26 保留，以 dark.json 的 camera.field.height / camera.row.height 定义；列坐标复用共享 inspector，时间轴保持原行高。

## 错误处理边界

- OBS-01（既有覆盖）：编辑继续经 engine edit 的串行信道返回原始错误，并由默认日志设施写文件与 Error stderr；未引入私有状态旁路、重试、重启或错误清除。桌面实际非法 object 回归确认错误返回后工程不变；既有默认级别文件/stderr/stdout 隔离测试随桌面全套运行。
- OBS-02（正常分支）：没有活动摄影机、隐式默认摄影机、观察视图、静态/分离但未动画的轨道均不是错误，不添加逐帧日志。
- OBS-03（功能边界）：新格式 19 拒绝无效摄影机、效果/源引用和超预算；预合成仍遵守既有仅平面图层限制，不放宽该协议。

## 验证

- 多摄影机专项：9 项通过；覆盖连续创建、旧格式显式 presence、保存重开、撤销重做、复制、边界帧、禁用、重排、移除、局部时间、模式转换、拆分、父子关系、表达式、锁定、错误原子性、隐藏键与资源上限。额外验证合法但极端的图层偏移：模式转换超出 i32 局部时间时返回错误且工程不变，pan / dolly 复用检查过的局部时间转换。旧摄影机工作流 2 项通过。
- 桌面全套：373 通过，1 项既有手动性能测试忽略；新 shell 回归验证连续新建与选择、非法对象返回错误且保留工程。
- 首轮核心全套发现 3 处把 19 当作未来版本的旧断言，以及旧 singleton 创建假设；相应迁移测试改为拒绝 20，旧 object=0 专项使用明确旧格式 fixture，新图层行为由新增测试独立验证。修正后受影响目标均复测，不删除原断言的拒绝/原子性含义。
- 共享 Host 2 项、粒子渲染 11 项通过；最终桌面摄影机交互复测 4 项通过。日志位于 `artifacts/multiple-camera-layers/` 的 `core-final-boundaries.log`、`host-tests.log`、`particle-tests.log`、`desktop-camera-tests.log`。
- 首次优化构建 `550d442` 的原生验收通过：中英文窗口、样式覆盖、浮动面板、节点预览和共享 MCP 均通过。人工查看 `native/smoke-zh.camera-properties.png` 与 `native/smoke-zh.timeline-cameras.png`，确认两台独立摄影机、所选摄影机参数和独立关键帧，行内向量、秒表与属性列对齐。窄列中的长数值、图层名称仍按既有省略规则显示，不声称整个 DESIGN §12 完成；D01–D12 处置见下表。
- 最终边界修复 `369e0f9` 的带 FFmpeg 优化构建通过（6 分 49 秒，22 项既有警告）；正式打包并独立解压，7 个运行库逐项 SHA-256 核对通过。Windows 包 `artifacts/multiple-camera-layers/package/MotionStudio-validation-windows-x86_64.zip`：83,503,185 字节，SHA-256 `aca689faddf2c1e8dd17e96234bac33522d82175eaa95350ac5cf7642ada3cee`。解压程序再次通过完整原生验收，日志 `native-final-acceptance.log`，截图和报告在 `native-final/`，使用独立工程与诊断目录。

## DESIGN §0.7 基线逐项处置

合并主线（含 #325）的集成复核：保留时间轴行内表达式的属性/表达式偏移规划，同时继续禁止摄影机的源锚点、几何与效果行；截图验收同时保留表达式与多摄影机场景。新增回归验证摄影机目标点表达式展开后，后续倾斜/视场角行保持正确位置。集成后桌面全套 410 通过 / 1 项既有测试忽略，新增行布局专项 1 项通过；多摄影机 9 项、旧摄影机 2 项、Host 2 项及粒子渲染 11 项再次通过，日志 `merged-*.log`。面板样式消费者和来源校验通过。此次合并未增加观测设施、重试或恢复机制，OBS-01–03 的信道、正常分支及功能边界保持不变。

| ID | 基线 | 本次处置 |
|---|---|---|
| D01 | path_controls 六处纵向步进 | retained：本次只屏蔽摄影机图像操作，保留既有布局；未声称专项视觉验收 |
| D02 | path_controls 双列宽 | retained：不改既有路径列宽 |
| D03 | path_controls 混合按钮高度 | retained：不改既有按钮命中 |
| D04 | path_controls 自绘字段 | retained：不改路径字段实现 |
| D05 | path_controls 布尔按钮 | retained：不改路径布尔控件 |
| D06 | effect_controls 行高/步进 | retained：未修改效果面板 |
| D07 | effect_controls fx 布尔 | retained：未修改效果面板 |
| D08 | effect_controls 枚举按钮 | retained：未修改效果面板；摄影机模式使用原生 Dropdown |
| D09 | effect_controls hover alpha | retained：未修改效果面板 |
| D10 | effect_controls 层级 | retained：未修改效果面板 |
| D11 | timeline 11 列坐标 | retained：按兼容基线保留；只加入摄影机类型和属性 |
| D12 | timeline snap guide 色值 | retained：保留既有拖拽反馈，不更换颜色 |

## 来源与限制

没有新增第三方依赖或资产；已核对 PROVENANCE.md、ATTRIBUTION.md、NOTICE。实现独立编写，未读取任何 Adobe 二进制或复制资源。

旧摄影机保留为 object=0 的兼容行，新建摄影机使用正常图层 ID；本次不自动迁移/删除旧动画，也不提供旧摄影机转换为普通图层的批处理。整数烘焙模式转换保持既有数值策略，不声称任意分数帧曲线完全相同。用户原工程只读检查，没有改写。实际用户工程的点击与显卡环境仍需要运行新包确认。
