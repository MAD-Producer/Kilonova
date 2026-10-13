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
