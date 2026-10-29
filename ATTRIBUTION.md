# 第三方归属说明 / Third-party attribution

## 插件作者工作区：可选本机 WGSL 语言服务

- 作者：wgsl-analyzer contributors。
- 来源：[官方仓库](https://github.com/wgsl-analyzer/wgsl-analyzer)，验收固定版本
  `2026-09-30`，revision `659b96821595a83cbc06abf844461eb736caaab5`。
- Windows x86_64 官方发行资产 `wgsl-analyzer-x86_64-pc-windows-msvc.zip`，
  SHA-256 `f9b9c0621149fcf989907787daeab9212035a42af45d4511b6ec50834a7e6b05`。
- 许可：MIT OR Apache-2.0；使用未经修改的官方可执行文件，原始许可随本地验收目录保存。
- 范围：独立安装的可选子进程，通过公开 LSP 标准连接。上游源码、图标和二进制
  不写入本仓库，不嵌入效果包，也未加入正式发布流水线。客户端、虚拟文档映射、
  界面和故障夹具由本项目独立实现；没有复制 VS Code 扩展或其他编辑器界面资源。
- 标准来源：[Microsoft LSP 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)。

本文件列出当前计划进入仓库或发布包的第三方材料。每个组件保留原始许可证和版权声明。根目录 [LICENSE](LICENSE) 只适用于按该许可证贡献的 Motion Studio 代码。

This file lists third-party material currently intended for the repository or a release. Each component keeps its original license and copyright notice. The root [LICENSE](LICENSE) applies only to Motion Studio code contributed under that license.

| 组件 / Component | 用途 / Use | 版权与来源 / Copyright and source | 许可证与声明 / License and notice |
| --- | --- | --- | --- |
| Motion Studio 源码 / Motion Studio source | Rust、UI、schema、测试和原创资产 / Rust, UI, schemas, tests, and original assets | Motion Studio contributors | MIT，见 [LICENSE](LICENSE) / MIT, see [LICENSE](LICENSE) |
| arboard 3.7.0 | 系统文本剪贴板，用于跨属性复用缓动定义；关闭图像功能 / System text clipboard for portable easing definitions; image features disabled | 1Password and arboard contributors; [v3.7.0](https://github.com/1Password/arboard/tree/5f5c8637911ba219edee619105a19b4f74e66a9f), exact package checksum in `Cargo.lock`; no source modifications | MIT OR Apache-2.0; upstream `LICENSE-MIT.txt` and `LICENSE-APACHE.txt` retained in the published crate |
| IBM Plex Mono Regular | 随应用分发的等宽字体 / Bundled monospace font | IBM Corp.; `google/fonts` revision `51303ca9e8ac9dcea7b12d307ba568fd0e6fcfca`, path `ofl/ibmplexmono/IBMPlexMono-Regular.ttf` | SIL Open Font License 1.1，见 [`crates/motion-media/fonts/OFL.txt`](crates/motion-media/fonts/OFL.txt) 和 [`crates/motion-media/fonts/SOURCE.json`](crates/motion-media/fonts/SOURCE.json) / SIL Open Font License 1.1, see the linked files |
| wgpu / Naga 30.0.1 | 官方 Cargo 依赖，用于 GPU 渲染、shader 转换与校验 / Official Cargo dependencies for GPU rendering and shader translation and validation | gfx-rs developers; [wgpu v30.0.1](https://github.com/gfx-rs/wgpu/tree/v30.0.1), exact package revisions and checksums in `Cargo.lock` | MIT OR Apache-2.0; upstream licenses retained in the published crates. 无本地源码补丁 / No local source modifications |
| Shader Effects 公式与参考 / Shader Effects formulas and references | 效果库使用的适配公式和独立 kernel / Adapted formulas and independent kernels used by the motion library | `shader-effects-inc/shaders`, revision `d49008cc2d6b547f216ca09bc13ae8f44572f040` | MIT；范围见 [`crates/motion-effects/motion-library/SOURCES.json`](crates/motion-effects/motion-library/SOURCES.json) 和 [`crates/motion-effects/motion-library/LICENSE.shaders.txt`](crates/motion-effects/motion-library/LICENSE.shaders.txt) / MIT; see the linked scope and notice files |
| FFmpeg desktop shared runtime | 视频导入与解码 / Video import and decoding | FFmpeg developers; Unix source `n8.1.3` (SHA-256 `09f990289327d3ebfedf4fc15cae3f884a65dab7a5aac7bbb5b01ff83900efd9`); Windows BtbN `autobuild-2026-09-30-13-08`, FFmpeg `n8.1.3-9-g29e619e767` (SHA-256 `3e47bda1607740550141e37c0e49d1e5182b34699f15adfd137ee266d346811a`); exact source/build links in `tools/prepare_ffmpeg.py` and bundled `FFmpeg-SOURCE.json` | Unix LGPL-2.1-or-later, Windows distribution LGPL-3.0-or-later. No source modifications; Unix external dependency autodetection/GPL/nonfree disabled. Shared libraries remain separately replaceable. Original license texts, configure arguments and per-file hashes accompany release archives. |
| libclang 18.1.1 (CI tool only) | Generate FFmpeg Rust bindings; not bundled in releases | LLVM contributors and Tao He; [libclang 18.1.1](https://pypi.org/project/libclang/18.1.1/), upstream LLVM 18.1.1; no modifications | Apache-2.0 WITH LLVM-exception; original notices in the installed wheel |
| cargo-audit 0.22.2 (CI tool only) | Audit the full Cargo.lock against current RustSec advisories; not bundled in releases | RustSec contributors; [release source revision 281452c35cf0870969042374110f099a411bc185](https://github.com/rustsec/rustsec/tree/281452c35cf0870969042374110f099a411bc185/cargo-audit); no source or binary modifications. Official Linux x86_64 archive SHA-256 `ab28a1bdb54db4d5d8ad5981cf1f959410370b3d28250dbd35f6a44248620e39`, Windows x86_64 archive SHA-256 `0a7316540862c13d954f648917ceacca593747baed6eec180fafa590be2710ab` | MIT OR Apache-2.0; upstream LICENSE-MIT and LICENSE-APACHE accompany the source. RustSec advisory data remains in the CI workspace; reports record its exact Git revision. |
| actions/cache (CI tool only) | Cache pinned libclang wheels and verified FFmpeg build inputs/runtime; not bundled in releases | GitHub and actions/cache contributors; [revision 0057852bfaa89a56745cba8c7296529d2fc39830](https://github.com/actions/cache/tree/0057852bfaa89a56745cba8c7296529d2fc39830) (v4); no modifications | MIT; [upstream license](https://github.com/actions/cache/blob/0057852bfaa89a56745cba8c7296529d2fc39830/LICENSE) |
| zlib 1.3.2 (Unix FFmpeg) | 解码压缩头 MOV；静态链接进 Unix libav，不依赖构建机 libz / Compressed MOV headers, statically linked into Unix libav | Jean-loup Gailly and Mark Adler; [official 1.3.2 source](https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz), SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`; no source changes, static PIC build | Zlib license; original `LICENSE` accompanies archives as `licenses/ffmpeg/zlib-LICENSE.txt`; source, revision, checksum and linkage in `FFmpeg-SOURCE.json` |

## Optional desktop depth prototype

Depth Anything V2 Small is by Lihe Yang, Bingyi Kang, Zilong Huang, Zhen Zhao,
Xiaogang Xu, Jiashi Feng, Hengshuang Zhao and the upstream contributors. Reference:
[official source](https://github.com/DepthAnything/Depth-Anything-V2/tree/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf),
revision `a561b849ebae10a6f5ef49e26c83cbbcd36c71bf`;
[official Small weights](https://huggingface.co/depth-anything/Depth-Anything-V2-Small),
SHA-256 `715fade13be8f229f8a70cc02066f656f2423a59effd0579197bbf57860e1378`.
Only Small's Apache-2.0 weights are supported by the export tool; the
noncommercial Base/Large/Giant models are not used. No model weights, reference
source or upstream images are bundled. The separately installed reference is
unmodified; the owned export procedure produces a fixed-shape opset-17 ONNX
artifact and retains its original license and provenance next to that artifact.
The host's letterboxing, process protocol, scheduling and installation checks
are independently authored under this repository's MIT license.

The optional worker uses separately installed Python (Python Software Foundation,
PSF license), NumPy 2.4.6 (NumPy Developers, BSD-3-Clause,
[source](https://github.com/numpy/numpy)), opencv-python-headless 5.0.0.93
(OpenCV team and wheel contributors, Apache-2.0 and the installed wheel's
third-party notices, [source](https://github.com/opencv/opencv-python)), and ONNX
Runtime 1.31.0 (Microsoft and contributors, MIT,
[source](https://github.com/microsoft/onnxruntime)). These versions record the
validated environment, not bundled dependencies. Local model export additionally
uses unmodified PyTorch 2.14.1 (PyTorch contributors, BSD-3-Clause,
[source](https://github.com/pytorch/pytorch)) and ONNX 1.23.2 (ONNX contributors,
Apache-2.0, [source](https://github.com/onnx/onnx)). Their existing licenses and
notices remain in their external installations; no runtime binaries or packages
are redistributed by this change.

External depth validation also uses NASA's Eileen Collins portrait, distributed
as `skimage/data/astronaut.png` in scikit-image v0.25.2 (Git blob
`834cda0012478c5edc8d43bade96d315dedeaab4`). Author/credit: NASA; source:
[portrait data and public-domain record](https://scikit-image.org/docs/stable/api/skimage.data#skimage.data.astronaut).
The portrait is identified as public domain by the dataset and is used for
factual technical evaluation under NASA's
[media guidelines](https://www.nasa.gov/nasa-brand-center/images-and-media/),
with no endorsement or promotional use. Original downloads remain outside the
product tree; hashes, resizing and crop/translation modifications are recorded
in the external fixture manifest. No NASA image, logo or person is added to the
product UI, examples or release assets.

Natural-domain depth validation additionally uses NASA's Destiny laboratory
photograph `iss007e11800`, recorded 2003-08-03 by the Expedition 7 crew
(individual photographer not specified), and NASA Johnson Space Center's
`Step_Inside_the_Station-4K-Clean` tour, recorded by the ISS crew 2022-10-10
and published in the NASA library 2025-12-17. Sources: [photograph record](https://images.nasa.gov/details/iss007e11800)
and [tour record](https://images.nasa.gov/details/Step_Inside_the_Station-4K-Clean).
NASA is credited as the author/source. Usage follows the informational and
factual technical-evaluation permission in the [NASA media guidelines](https://www.nasa.gov/nasa-brand-center/images-and-media/),
with no endorsement or promotional use; separately identified third-party
content is not granted NASA's permission. Selected library metadata does not
identify a third-party copyright holder. The acquired source variants, hashes,
dates and exact extracted video timestamps are retained in the validation
manifest. Modifications are bounded frame extraction, RGB decoding and image
resizing for numerical/edge comparisons; no source audio is retained. Original
media remains external. Only credited evaluation frames and scalar results may
enter the technical evidence archive, never product UI, examples or releases.

## Rust 依赖 / Rust dependencies

The read-only adapter memory probe directly uses two unmodified binding crates
already present in `Cargo.lock`: `ash` 0.38.0+1.3.281 by Maik Klein, Benjamin
Saunders, Marijn Suijten and ash contributors, from
[ash-rs/ash](https://github.com/ash-rs/ash), MIT OR Apache-2.0, package SHA-256
`0bb44936d800fea8f016d7f2311c6a4f97aebd5dc86f09906139ec848cf3a46f`;
and `objc2-metal` 0.3.2 by the objc2 contributors, from
[madsmtm/objc2](https://github.com/madsmtm/objc2), Zlib OR Apache-2.0 OR MIT,
package SHA-256
`a0125f776a10d00af4152d74616409f0d4a2053a6f57fa5b7d6aa2854ac04794`.
Their original licenses remain in the published crates. No binding source,
external assets or documentation text is copied or modified. These target-specific
direct declarations expose the same native adapter that wgpu already owns.

`motion-ui` uses the unmodified `unicode-segmentation` 1.13.3 crate for extended
grapheme boundaries during editing. Authors: kwantam, Manish Goregaokar and
unicode-rs contributors. Source: [unicode-rs/unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation),
crates.io release 1.13.3, package SHA-256
`c6f5d3c3b1bf09027a88a6bc961fc00497d651009560b5463668dc81b0fa87a8`.
License: MIT OR Apache-2.0; retain the crate's COPYRIGHT, LICENSE-MIT and
LICENSE-APACHE notices. No upstream source, tables or assets are modified or
copied into the repository. The lockfile already includes this transitive package;
this change declares its direct use by the UI crate without adding another version.

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

## Scene ingestion dependencies and external references

The CPU-only `motion-scene` importer is independently authored from the public
[Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html),
document revision `8e798b02d254cea97659a333cfcb20875b62bdd4`. No specification text,
upstream loader implementation, screenshots or product assets are copied.

Three existing, locked registry components become direct dependencies of this
crate. Their unmodified implementations and notices remain in their packages;
the exact registry checksums are recorded in `Cargo.lock`.

| Component | Author and source revision | License | Modification scope |
| --- | --- | --- | --- |
| base64 | Marshall Pierce and contributors; [rust-base64](https://github.com/marshallpierce/rust-base64), 0.23.1 | MIT OR Apache-2.0 | Unmodified Cargo dependency for data URIs and portable node-image payloads; exact package checksum in Cargo.lock |
| percent-encoding | rust-url developers; [rust-url](https://github.com/servo/rust-url), 2.3.2 | MIT OR Apache-2.0 | Unmodified Cargo dependency for local URI decoding |
| sha2 | RustCrypto developers; [hashes](https://github.com/RustCrypto/hashes), 0.10.9 | MIT OR Apache-2.0 | Unmodified Cargo dependency for source/resource identity |

External validation uses unmodified `Avocado` (Microsoft, public 2017) and
`BoxVertexColors` (Marco Hutter, public 2023) from
[Khronos glTF Sample Assets](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/8f492390ddc7a2ae221a1369ef560589cf8c70fb),
revision `8f492390ddc7a2ae221a1369ef560589cf8c70fb`. Both model-specific `LICENSE.md`
and metadata identify CC0-1.0. Original glTF/GLB files, buffers, textures and
licenses stay in the external validation directory. Source URLs and individual
SHA-256 values are recorded in its manifest. Only local, owned frozen bundles
and numerical reports are generated; no model or texture enters the product,
node examples or release artwork. This records technical evaluation, not
endorsement by the authors or Khronos.

The independently authored OBJ/MTL importer references Library of Congress,
Digital Formats descriptions [fdd000507 (OBJ)](https://www.loc.gov/preservation/digital/formats/fdd/fdd000507.shtml),
last significant revision 2025-02-18, and
[fdd000508 (MTL)](https://www.loc.gov/preservation/digital/formats/fdd/fdd000508.shtml),
last significant revision 2019-10-04, accessed 2026-10-26. These are reference
links only: no source text, loader code, illustration or model is copied or
redistributed, and rights to the references remain with their authors. Wavefront
names identify the documented interchange format, not a product affiliation.
The OBJ/MTL/glTF comparison geometry and PNG pixels used by the added tests and
local probe are authored by Motion Studio contributors and encoded locally;
they are covered by this repository's MIT license. No extra dependency or
third-party model/texture is introduced for OBJ.
