# #579：原生曲线夹具的平台编译修正

首轮 CI run `41306838947` 的 Linux job `124848861224` 在编译测试时失败。
原始 job 日志保存在本机 `artifacts/issue-goal-current/path-authoring/ci-linux-job.log`，
并收入 `layer-time-path-authoring-evidence/evidence.zip`。七项编译错误来自
`animation_controls/mapped_tests/native.rs` 及其 `native/clipboard.rs`：
Windows EventLoopBuilder 扩展和仅 Windows 存在的 native_test_support
被 Linux 测试构建无条件导入。

修正在模块声明处添加 `#[cfg(target_os = "windows")]`。它限定的是两个已有
Windows 原生验收夹具，不更改跨平台生产实现，也不屏蔽普通 mapped_tests
或同级 clipboard 测试。已有原生测试仍在 Windows 编译，手动运行要求不变。
不修改断言、错误处理、运行期限、重试或 CI 门禁。

带 FFmpeg 和宿主 diagnostics 的 Windows `cargo check --workspace --all-targets
--exclude motion-android --locked` 再次通过（19.04 秒）。该结果仅证明本机
平台编译；Linux／macOS 必须由推送后的 CI 再验证，不能提前声明通过。
此修正不改变产品 UI，无新依赖或素材；DESIGN §0.7 的十二项基线均沿用
前阶段逐项 retained with reason，仍未做全局视觉整改。
