# #223 交付范围与关闭条件

本记录逐项对应 issue 的原始范围。功能完成、证据覆盖和合入状态分别认定；CI 未通过或 PR 未合入时，不关闭 issue。

| 原始要求 | 实现入口 | 直接证据 | 当前认定 |
| --- | --- | --- | --- |
| 分组列出素材与多合成，保留孤立素材 | `project_library` 模型、命令与项目 catalogue | 模型阶段 `project_library_is_shared_by_compositions_and_keeps_orphan_assets_after_save_reopen`；面板 `project_inventory_lists_unreferenced_items_and_all_compositions_without_decoding_animation_payloads`；原生目录矩阵 | 本机功能已验证 |
| 面板导入与系统拖入共享流程，注册素材可拖到时间轴 | `shell/project_import.rs`、主/浮窗 DroppedFile 路由、原生资产拖放桥接 | `project_import_buttons_capture_the_browsed_or_context_folder_in_both_languages_and_windows`、`project_drop_registers_in_captured_folder_and_explicit_placement_has_separate_history`；原生主→主、主→浮、浮→主拖放实际落点及单次发布断言 | 本机功能已验证；生产入口与控制输入已测，不宣称 OS 文件拖放注入 |
| 异步缩略、视频预览及目录/搜索范围持久化 | 有界 thumbnails、source Player、root 绑定 ViewPreferences | 合法上限 256 素材在阻塞解码时仍可绘制/搜索；真实图像/视频首帧、播放/暂停/定位像素断言；`project_view_preferences_are_bounded_and_saved_scope_never_applies_to_a_different_project` 与实际偏好保存失败信道测试 | 本机功能已验证；静音源预览沿用 128×128 位图预算 |
| 导入可见，明确选择目标及创建图层 | 捕获目录的 ProjectImport、ProjectAsset 注册/放置命令、菜单播放头放置、轨道拖放 | 真实图像/音频/视频先注册且图层为空；视频关联音频同目录；明确目标帧放置、独立撤销及保存重开 | 本机功能已验证 |
| 大量资产不阻塞，缺失/取消后可恢复 | 后台 image Pool、媒体任务、source Player、显式刷新 | 受控阻塞前 pending/start 发布、未返回任务继续占许可；缺失源实际 OS 错误；取消/失效不提交；修复源后显式刷新/重新导入恢复；默认文件/Error stderr 与 stdout 隔离 | 本机功能与失败路径已验证；不声称外部解码永不阻塞 |
| 主窗/浮窗输入与焦点一致，中英文语义相同 | owner 绑定项目弹窗、共用命令与媒体工作流 | 目录输入矩阵覆盖焦点、跨窗输入、IME、按键重复、过期来源及关闭帧；真实窗口双语言目录 12 场景、拖放 6 场景、预览 4 场景 | Windows 原生 GPU 与生产输入验证；其他系统由 CI 验证，未宣称原生人工实测 |
| 成功、取消、失效可复现；持久数据保存重开与撤销 | 原子文档命令及已有历史/存储路径 | 模型原子失败/保留 redo，真实注册与放置分离历史、原生目录和拖放保存重开、错误/取消/过期信道证据 | 本机已验证，完整模型/核心 528 passed、1 既有 ignored 对应源哈希未变 |

## 证据与边界

模型、浏览器、后端注册、交互、主分支整合和源播放六个阶段的结果及 zip 分别冻结，未改写旧阶段源身份或原始失败。详见 [project-library.md](project-library.md) 和各阶段 `*-results.json`。播放器阶段相关 67 个文件在最终本机验证期间保持一致；不把按过滤重复执行的测试相加。三项手动原生验收使用 `--ignored` 显式执行，不把普通过滤中的 ignored 算作通过。

自有底座 #530 已在五项必需 CI 通过后合入 main（`2af21061`）。#532 的旧远端提交 `e1e8716a` 也已通过五项必需 CI（run `40631788056`）；随后同步当前主线摄影机与渲染改动得到 `1698884a`，旧提交的绿色结果不替代新提交的 CI。没有跳过断言、关闭强制回溯或为产品添加重试/重启。

`1698884a` 重新执行模型/核心 531 passed、1 既有 ignored；桌面分组分别为目录命令 9、项目面板 16、播放器 3，显式原生 3 全部通过。分组有重叠，不相加；面板普通过滤中的 3 ignored 已在原生组显式执行。生成 116 张真实 GPU 截图并完成带 FFmpeg 的 Windows dev 构建。原始日志、截图及源码身份见 [最终整合结果](project-library-final-main-results.json) 与 [归档](data/project-library-final-main-evidence.zip)，未覆盖任何旧阶段结果。

DESIGN §0.7 十二项既有问题全部沿用原记录的保留原因；不把本次功能验收认定为已修复全局 UI。用户已明确顺序为先完成 #223，再专注 UI。完整工作区中项目顶部拥挤、时间轴表头密度及预览信息层级的问题保留在[UI 复查](project-library-ui-review.md)，后续统一收敛，不在此 issue 中继续扩大界面范围。

## 尚未达到的关闭条件

- 当前最终提交的跨平台必需 CI 全部通过。
- #532 合入 main，远端 head 与通过的检查匹配；底座 #530 已合入。
- 核对合入结果后关闭 #223；当前保持开放。不把其它未完成 issue 或后续 UI 收敛标记为完成。
