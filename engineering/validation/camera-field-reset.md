# 摄影机字段重置、关键帧与边界反馈（#275）

## 范围与来源

分支 `codex/camera-field-reset` 基于 main `8a722097`。开始前核对开放 PR、issue 时间线、摄影机相关远程分支和工作树；#422/#423 处理渲染错误，不覆盖这些摄影机控件。未修改其他工作树。已检查 `PROVENANCE.md`、`ATTRIBUTION.md`、`NOTICE`，没有新依赖、第三方资产、兼容名称或产品素材。

## 行为与功能边界

- 在已有数值字段上右键，选择“重置当前值 / Reset current value”。恢复值来自当前合成尺寸的 `Camera::new`，只写所选分量在当前帧的值。标量、耦合向量、分离轴分别复用 SetScalar/SetVector/SetComponent；保留其他分量、关键帧、插值和其他轨道，单步撤销。不提供删除整条动画的伪装重置，也不重置胶片宽度或摄影机模式。
- 图层摄影机保留既有属性行导航。旧版主摄影机此前构造空关键帧行；现在读取真实耦合/分离轨道，并用共享导航栏发出已有增删关键帧与 Seek 命令。派生焦距/像素缩放仍共用 FOV，不制造新动画通道；未实现的景深仍由既有镜头界面明确禁用。
- 数值写入、秒表与重置要求可编辑整帧。子帧仍可浏览与关键帧跳转，悬停说明整帧限制；允许的整帧误差沿用 key_navigation::editable_frame，数值与派生镜头命令使用同样的就近整数约定。切换工程、合成、摄影机、模式或时间，锁定、只读、失焦、Escape，以及离开摄影机属性界面均取消正在进行的编辑。打开重置菜单先撤销已有拖动，菜单确认用之后的新快照建立独立历史步骤。
- 显示与输入沿用模型范围：FOV 10–120°，Elevation −89–89°（避开 ±90° 极点），Radius 1–10,000,000px，Roll/Azimuth ±1,000,000°，Position/Target ±10,000,000px。非法、无穷或非数文本保持编辑并显示本地化范围提示；拖动在边界收敛。缺失、非数或越界采样显示“不可用”，不再伪造零值或随向量写入缺失分量。

数值错误提示与重置是本次功能变化；浮窗 Escape 完整释放字段、主窗/浮窗转发读数是交互修复；浮窗底部使用已有状态栏显示读数是反馈变化。没有增加属性行、缩小点击区域、引入背景常驻的数值框或修改主题 token。右键菜单复用 Dropdown，展开时抑制悬停读数，防止遮挡操作。

## 可复现验证

定向命令（Windows portable MSVC 环境，离线锁定依赖）：

```powershell
cargo test -p motion-desktop --bin motion-studio --config 'profile.dev.package.motion-desktop.debug=0' --config 'profile.dev.package.motion-desktop.codegen-units=256' --locked --offline -j1 panels::camera_controls -- --test-threads=1
```

本机日志保存于 `.tools/camera-fields/`。`MOTION_CAMERA_FIELDS_NATIVE_ARTIFACTS` 与 `MOTION_CAMERA_FIELDS_FAILURE_ARTIFACTS` 可指定截图和默认日志检查的输出目录；未指定时使用测试临时目录。

- `reset_is_one_undoable_value_edit_preserving_animation_and_other_channels_after_reopen`：真实桌面 Engine，图层/旧版主摄影机、两种模式、全部七种基础属性、耦合/分离轴；确认其他轨道、其他分量与原始两端关键帧逐值保留，新增当前键，一次撤销/重做、保存后从存储读取、关闭引擎重开。
- `reset_popup_cancels_on_escape_focus_loss_context_lock_and_subframe_without_editing`：Escape、失焦、工程/合成切换、锁定、只读、子帧；另验证拖动中打开菜单先回滚，再单步重置，以及整帧容差不写入前一帧。
- `elevation_rejects_poles_and_non_finite_text_and_clamps_drag_with_localized_feedback`：实际文本输入 90、−90、NaN、1e999 均不写入，中英文错误读数、拖动收敛到 89°、Escape 回滚。
- `legacy_camera_rails_toggle_and_seek_real_coupled_and_separated_keys`：主摄影机真实增删当前键、上一键/下一键跳转、原有键保留及撤销。
- `missing_sample_streak_reports_once_at_sixty_and_recovers_at_default_log_level`：真实 UI 缺失采样路径，跨播放帧连续 120 次、健康恢复、再次 60 次；默认 Info 文件及 Error stderr 各两条，首条 Debug 被抑制，stdout 无日志。
- Windows 原生验证：创建真实 HWND 与原生浮窗渲染目标，通过 `chrome` / `floating_chrome` 输入与焦点路由编辑；中英文 × 主窗/浮窗 × 100/125/150/200% DPI，共 64 张字段、菜单、重置和非法极点截图。英文键盘确认，中文鼠标确认，均验证一次撤销、取消及相同命令语义。主窗逻辑尺寸 500×620，窄浮窗 360×620，截图报告与实际 PNG 尺寸相符。不是 OS 物理键鼠驱动的验收，也不声称覆盖所有显示器或所有工作区。

初验截图发现悬停读数覆盖重置菜单，现已抑制。真实浮窗路由发现未提交文本按 Escape 后残留字段状态，现已完整取消。初始往返夹具在切换模式后误以为轨道仍为两键，并生成过摄影机与目标点重合的端点；模式转换本就会烘焙轨道，现改为转换后明确创建不重合的两键夹具，保留原始端点、其他通道及撤销断言。失败日志保留，不计入通过结果。

最终代码的定向回归共 **54 通过、0 失败、0 跳过**：摄影机 11（107.66 秒，包含上述新用例与既有镜头回归）、numeric 2、keyboard 12、panels 基础 13、properties 7、key_navigation 9。原生 64 张 PNG 全部生成，目视复核中英文菜单、非法值和主窗/窄浮窗反馈，覆盖高 DPI。`cargo check -p motion-desktop --tests --locked --offline` 已通过；测试编译仅 18 项既有桌面警告。最终来源检查覆盖 1786 tracked paths，样式消费者与差异检查通过。没有把这些定向结果扩大成全桌面测试通过；PR 的完整远程 CI 仍待完成。

## 观测与错误边界审计

| ID | 分类 | 证据与剩余边界 |
|---|---|---|
| CAM-FIELD-SAMPLE | 新观测 | 一个控件实例仅保留一条 root/composition/camera 身份的连续失败计数，最大 61（61 为已报告标记）；不保留 payload 或动态观察键。首次 Debug，第 60 次 Error，随后抑制；健康渲染或摄影机身份切换重置，播放帧变化不重置。记录 object、property、字段、帧、静态原因及范围；日志文件/stderr/stdout、阈值与恢复由真实缺失采样验证。60 次不代表一秒。 |
| CAM-FIELD-COMMAND | 既有覆盖 | 沿用真实 Engine 编辑/批量编辑、in-flight、Info start、Error 文件和 stderr、历史及持久化边界；没有新线程、阻塞操作、重试或重启，也未给 Present 添加日志。 |
| CAM-FIELD-CANCEL | 正常分支 | 取消、失效目标、观察模式、锁定、子帧和菜单关闭不当作故障；已开始的数值事务按 History(4) 回滚，未开始的文本只释放焦点。 |
| CAM-FIELD-DIMENSIONS | 既有观测 + UI 边界 | 默认值无法取得合成尺寸时禁用重置并显示原因；既有 lens.snapshot 已观测同一必需尺寸的缺失/非法，未重复增加错误风暴。 |
| CAM-FIELD-DERIVED | 功能边界 | 派生镜头不新增轨道，不把景深等未支持能力伪装成命令；重置为当前合成的原始默认值，不是保持当前视点的模式转换。 |

## DESIGN §0.7 基线处置与 §12

| 基线行 | 状态与原因 |
|---|---|
| path_controls 六种步距 | retained with reason：本批没有路径布局变化。 |
| path_controls 两列硬编码 | retained with reason：未改路径几何。 |
| path_controls 按钮/字段高度 | retained with reason：未改路径动作或命中区。 |
| path_controls 自绘数值底色 | retained with reason：本批复用 numeric，不改路径字段。 |
| path_controls 布尔宽按钮 | retained with reason：未改路径控件。 |
| effect_controls 行高/步距 | retained with reason：未改效果列表。 |
| effect_controls 布尔按钮 | retained with reason：未改效果布尔输入。 |
| effect_controls 枚举按钮 | retained with reason：未改效果枚举；新菜单使用共享 Dropdown。 |
| effect_controls hover alpha | retained with reason：未改效果绘制，没有新增 hover 色。 |
| effect_controls 缺少 depth | retained with reason：未改效果树结构。 |
| timeline 11 列坐标 | retained with reason：保持既有时间轴布局与点击区域。 |
| timeline snap guide 颜色 | retained with reason：未改吸附提示绘制。 |

§12 中相关的单属性单行、透明静止数值、既有列宽/行高、中英主窗/浮窗菜单、数字输入、取消/失效、历史和保存由上述真实输入与原生截图验证。其他面板的历史基线、全工作区尺寸组合、物理键鼠、非 Windows 原生窗口与整个 DESIGN 的整改不属于本次验收；远程 CI 结果单独记录。
