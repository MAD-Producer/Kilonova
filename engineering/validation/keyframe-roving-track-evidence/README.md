# Track 接入验收证据

范围及未完成项见[阶段说明](../keyframe-roving-track-progress.md)。所有二进制工程包均由
本仓库测试生成，不包含外部素材。`manifest.json` 区分最终验证、早期运行、失败和中止，
提供实际文件和源码的 SHA-256。重复运行的测试不累计计数。

`project-final/project.json` 是实际 `storage::save` 的完整工程；`spatial.msproj` 是实际导出
工程包，`reopened/project.json` 是导入结果。`scene-samples.json` 记录 101 个实际 Scene
世界矩阵、独立匀速位置对照及原整数时间轨道必须不同的负对照，不是 GPU 或 MP4。

错误信道证据使用真实桌面 worker，子进程移除 `MOTION_LOG` 以使用默认级别。检查
实际拒绝、成功恢复、工程／revision／历史不变、文件 Error、stderr Error、Info start
及 stdout 隔离；不以纯模型 Err 或 mock 替代日志验证。

可复现的模型／核心命令（Windows 需先准备 MSVC 环境；target 可复用本机目录）：

```powershell
cargo test -p motion-model --test spatial_track --test spatial_schedule --test spatial_hot_path --test spatial_timing --test automatic_bezier --test animation_sampling_errors --test path_tangent_sampling --test timeline_errors -p motion-core --test spatial_keyframes --test keyframe_clipboard --test merge_dimensions --test separate_dimensions --test layer_anchor_animation --test expressions --test animation_presets --test path_conversion --test path_clipboard --test project_format_contract --test camera_workflows --jobs 1 --locked --offline -- --nocapture --test-threads=1
```

`MOTION_SPATIAL_TRACK_ARTIFACTS` 可选且应指向一个新的输出目录；不设置时使用独立的
临时目录。已有导入目标按生产接口正常拒绝覆盖，不能为重复采集绕过该保护。

桌面测试程序由 `motion-desktop` 包、`motion-studio` bin、`--profile test` 及
`--features ffmpeg,gpu-tests` 构建；本机沿用固定 FFmpeg／libclang，日志保存实际命令
结果。测试程序不是供用户启动的桌面发行可执行文件，不宣称它就是带 UI 的运行版本。

在同一源码构建出的测试程序上依次运行以下过滤器，均加 `--nocapture --test-threads=1`：

- `engine::animation_action_tests::`：6 项；`MOTION_SPATIAL_KEYFRAME_ERROR_ARTIFACTS` 指向
  新目录可保留空间关键帧 worker 的实际 stdout／stderr／文件日志及状态。
- `panels::animation_menu::`：19 项通过，1 项原生截图测试保持 ignore。
- `panels::timeline_shortcuts_tests::`：55 项通过，1 项原生截图测试保持 ignore。
- `plugin_editor::tests::`：6 项通过。

本机运行前将固定 FFmpeg 的 `bin` 加入 PATH，并沿用构建的工作区 TEMP／TMP。首轮系统
Temp 的权限拒绝保存在 `desktop-worker.log`，成功运行保存在 `desktop-worker-final.log`。
