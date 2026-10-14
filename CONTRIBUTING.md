# 贡献约定

## 分支命名

使用 `<type>/<short-kebab-topic>`，名称描述实际工作，不使用工具或代理名称作为前缀，也不默认附加日期。

| 前缀 | 用途 | 示例 |
| --- | --- | --- |
| `feat/` | 新能力 | `feat/render-dag` |
| `fix/` | 缺陷修复 | `fix/audio-playhead` |
| `perf/` | 性能改进 | `perf/texture-cache` |
| `refactor/` | 行为不变的重构 | `refactor/model-layer` |
| `test/` | 测试补充 | `test/media-registration` |
| `docs/` | 文档 | `docs/plugin-sdk` |
| `ci/` | 持续集成 | `ci/android-build` |
| `chore/` | 仓库维护 | `chore/branch-conventions` |

需要区分相关任务时可加 issue 编号，例如 `fix/141-asset-registration`。从最新 `main` 创建独立分支，通过 PR 审阅和 CI 后 squash 合并。

## 分支清理

合并后删除工作分支。清理已有分支时，先确认没有开放 PR 引用，分支头仍对应已合并的 PR，或全部提交已被 `main` 包含。Squash 合并不能只凭 `git branch --merged` 判断。

保留未合并提交、未提交改动及正在使用的工作树。清理分支引用不删除工作目录或构建产物；不会通过 reset、stash 或强制切换丢弃他人的改动。

开放 PR 的远程头分支保留到合并后再删除，避免改名导致 PR 关闭。本地可先改名并继续显式推送到原远程分支。

## 禁止事项（硬性）

本项目为 clean-room 实现。禁止：

1. 反编译、反汇编、提取 Adobe After Effects、Photoshop、Premiere 等 Adobe 软件的任何二进制内容（含字符串表、资源段）。
2. 复制 Adobe 的图标、指针、字体文件、音频、预设（`.ffx`）、模板、截图或文本文档。
3. 在产品名、Logo、图标、UI、包标识或发行素材中使用 Adobe 商标（After Effects、Adobe 等）。
4. 引入没有作者、来源、版本、许可和改动范围记录的第三方资产。

允许的参考来源：

- Adobe 公开用户指南（`helpx.adobe.com`）。
- Adobe 公开的脚本 API、快捷键和格式说明。
- 行业公开资料、标准、论文和社区讨论。

参考资料只用于理解公开行为，不得复制 Adobe 资源或暗示 Adobe 认可 Motion Studio。新资产必须先更新 `PROVENANCE.md` 和 `ATTRIBUTION.md`；CI 会拦截已知的二进制、字体和产品文案风险。
