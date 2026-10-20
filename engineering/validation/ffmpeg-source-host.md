# Unix FFmpeg 构建依赖下载源修正

PR #357 的 macOS Intel job `118177265709`（run `39287166893`）在准备 FFmpeg 时，访问 `zlib.net/fossils/zlib-1.3.2.tar.gz` 出现 `socket.gaierror` / `URLError: nodename nor servname provided, or not known`，最终退出 1。该 job 中之前运行的测试通过；此次失败不是偏好设置代码编译错误。原始日志保存在本地 `artifacts/property-selection-context/preferences-macos-ci-failure.log`。

改用同一作者的官方 [zlib v1.3.2 GitHub 发行包](https://github.com/madler/zlib/releases/tag/v1.3.2)，原版本、原 tar.gz 字节及 SHA-256 均保持：`bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`。GitHub Releases API 该资产的 digest 与现有固定值一致；调用实际 `prepare_ffmpeg.download` 下载 1,502,830 字节并复核摘要一致。没有增加回退源、重试、关闭校验或吞掉异常；失败仍保留原异常链并退出非零。

ATTRIBUTION.md 同步来源 URL，作者、许可证、版本和修改范围保持。PROVENANCE.md、ATTRIBUTION.md、NOTICE 已核对，无新增依赖或产品资源。

验证：实际网络下载及摘要比对通过；`python -m unittest discover -s tools -p test_ci_ffmpeg.py` 3 项通过，保留缓存摘要校验、损坏输入重取及校验失败不发布的原断言；provenance guard、`git diff --check` 通过。未在 Windows 本机编译 macOS zlib/FFmpeg；真实 macOS 完整打包结果以本 PR CI 为准，不能用下载成功代替打包验收。
