# 第三方归属说明 / Third-party attribution

本文件列出当前计划进入仓库或发布包的第三方材料。每个组件保留原始许可证和版权声明。根目录 [LICENSE](LICENSE) 只适用于按该许可证贡献的 Motion Studio 代码。

This file lists third-party material currently intended for the repository or a release. Each component keeps its original license and copyright notice. The root [LICENSE](LICENSE) applies only to Motion Studio code contributed under that license.

| 组件 / Component | 用途 / Use | 版权与来源 / Copyright and source | 许可证与声明 / License and notice |
| --- | --- | --- | --- |
| Motion Studio 源码 / Motion Studio source | Rust、UI、schema、测试和原创资产 / Rust, UI, schemas, tests, and original assets | Motion Studio contributors | MIT，见 [LICENSE](LICENSE) / MIT, see [LICENSE](LICENSE) |
| IBM Plex Mono Regular | 随应用分发的等宽字体 / Bundled monospace font | IBM Corp.; `google/fonts` revision `51303ca9e8ac9dcea7b12d307ba568fd0e6fcfca`, path `ofl/ibmplexmono/IBMPlexMono-Regular.ttf` | SIL Open Font License 1.1，见 [`crates/motion-media/fonts/OFL.txt`](crates/motion-media/fonts/OFL.txt) 和 [`crates/motion-media/fonts/SOURCE.json`](crates/motion-media/fonts/SOURCE.json) / SIL Open Font License 1.1, see the linked files |
| wgpu / Naga 30.0.1 | 官方 Cargo 依赖，用于 GPU 渲染、shader 转换与校验 / Official Cargo dependencies for GPU rendering and shader translation and validation | gfx-rs developers; [wgpu v30.0.1](https://github.com/gfx-rs/wgpu/tree/v30.0.1), exact package revisions and checksums in `Cargo.lock` | MIT OR Apache-2.0; upstream licenses retained in the published crates. 无本地源码补丁 / No local source modifications |
| Shader Effects 公式与参考 / Shader Effects formulas and references | 效果库使用的适配公式和独立 kernel / Adapted formulas and independent kernels used by the motion library | `shader-effects-inc/shaders`, revision `d49008cc2d6b547f216ca09bc13ae8f44572f040` | MIT；范围见 [`crates/motion-effects/motion-library/SOURCES.json`](crates/motion-effects/motion-library/SOURCES.json) 和 [`crates/motion-effects/motion-library/LICENSE.shaders.txt`](crates/motion-effects/motion-library/LICENSE.shaders.txt) / MIT; see the linked scope and notice files |
| FFmpeg desktop shared runtime | 视频导入与解码 / Video import and decoding | FFmpeg developers; Unix source `n8.1.3` (SHA-256 `09f990289327d3ebfedf4fc15cae3f884a65dab7a5aac7bbb5b01ff83900efd9`); Windows BtbN `autobuild-2026-09-30-13-08`, FFmpeg `n8.1.3-9-g29e619e767` (SHA-256 `3e47bda1607740550141e37c0e49d1e5182b34699f15adfd137ee266d346811a`); exact source/build links in `tools/prepare_ffmpeg.py` and bundled `FFmpeg-SOURCE.json` | Unix LGPL-2.1-or-later, Windows distribution LGPL-3.0-or-later. No source modifications; Unix external dependency autodetection/GPL/nonfree disabled. Shared libraries remain separately replaceable. Original license texts, configure arguments and per-file hashes accompany release archives. |
| libclang 18.1.1 (CI tool only) | Generate FFmpeg Rust bindings; not bundled in releases | LLVM contributors and Tao He; [libclang 18.1.1](https://pypi.org/project/libclang/18.1.1/), upstream LLVM 18.1.1; no modifications | Apache-2.0 WITH LLVM-exception; original notices in the installed wheel |
| zlib 1.3.2 (Unix FFmpeg) | 解码压缩头 MOV；静态链接进 Unix libav，不依赖构建机 libz / Compressed MOV headers, statically linked into Unix libav | Jean-loup Gailly and Mark Adler; [official 1.3.2 source](https://zlib.net/fossils/zlib-1.3.2.tar.gz), SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`; no source changes, static PIC build | Zlib license; original `LICENSE` accompanies archives as `licenses/ffmpeg/zlib-LICENSE.txt`; source, revision, checksum and linkage in `FFmpeg-SOURCE.json` |

## Rust 依赖 / Rust dependencies

workspace 和传递 Rust 依赖保留各自作者发布的许可证 metadata。发布自动化必须根据 `Cargo.lock` 和包 metadata 生成完整的依赖声明；本表不能替代该清单。

Workspace and transitive Rust dependencies retain the license metadata published by their respective authors. Release automation must generate a complete dependency notice from `Cargo.lock` and package metadata; this table is not a substitute for that inventory.

## 字体与系统资源 / Fonts and system resources

用户选择的系统字体不会由 Motion Studio 重新分发。工程包会保存用户导入时提供的字体字节和许可证文本。新增内置字体前，必须先添加来源记录和完整许可证。

System fonts selected by a user are not redistributed by Motion Studio. A project package stores the selected font bytes and the license text supplied at import time. A new bundled font must add a source record and full license before it is committed.

## Adobe 与其他商标 / Adobe and other trademarks

在需要标识来源或兼容性边界时，Adobe 产品名可以出现在内部参考记录中。Motion Studio 资产不包含 Adobe logo、产品图标、截图、二进制文件、插件、预设或字体。商标免责见 [NOTICE](NOTICE)。

Adobe product names may appear in internal reference notes when identifying a source or compatibility boundary. No Adobe logo, product icon, screenshot, binary, plug-in, preset, or font is included as a Motion Studio asset. See [NOTICE](NOTICE) for the trademark disclaimer.


## Noise mathematical reference

Ken Perlin, *Improving Noise*, SIGGRAPH 2002, [author-hosted paper](https://mrl.cs.nyu.edu/~perlin/paper445.pdf), is a mathematical reference for quintic interpolation and gradient noise. No paper text, reference source code, permutation tables or third-party assets are redistributed. The 2D lattice hash, eight-direction gradients, bounded octave contract, WGSL kernel, CPU oracle and editable example are independently authored Motion Studio code under this repository's MIT license. This entry records a mathematical reference rather than an adapted or bundled third-party implementation.

## Color blending mathematical reference

W3C, *Compositing and Blending Level 1*, Candidate Recommendation Draft dated
2024-03-21, [fixed revision](https://www.w3.org/TR/2024/CRD-compositing-1-20240321/),
sections 6 and 10, defines the source-over and separable transfer mathematics used
as a reference for the node Blend operation. The document is under the W3C
permissive document license; no document text, illustrations, sample assets or
upstream implementation is bundled. Motion Studio's WGSL kernel, typed node,
CPU oracle and example are independently authored under the repository MIT
license. Bounded Add, Subtract and Divide transfers are arithmetic extensions,
not claims of W3C blend-mode conformance. No layer compositing behavior is changed.
