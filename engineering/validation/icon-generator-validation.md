# 图标生成期描边样式校验

关联 #138 第五项。原生成器检查 SVG 属性名，却直接把 `stroke-linecap` / `stroke-linejoin` 的值写入 `paths.json`；Rust 只接受三种端点、三种连接，其他值会在首次描边绘制时触发现有 panic。

在生成器中按现有 Rust 消费者支持范围校验，缺省值仍为 butt / miter。无可见 stroke 的图元也检查，避免输出不能被运行时解释的数据。错误包含源路径、属性名及值，路径最多 2048 字符、值最多 128 字符；不改变正常生成输出。仍使用工具已有的 ValueError / stderr 退出方式，不引入运行时日志或异常吞噬。

## 证据与边界

| ID | 分类 | 证据与限制 |
| --- | --- | --- |
| IG-01 | 功能修复 | 在全部图标编译完成、打开目标文件写入之前拒绝不支持的描边样式；生成与 `--check` 两种模式相同。 |
| IG-02 | 实际失败与恢复验证 | 修改前运行新增测试，3 项中 12 个子案例失败；记录 `artifacts/property-selection-context/icon-generator-before.log`。修改后 3 项通过，见 `icon-generator-after.log`。测试未修改既有断言。 |
| IG-03 | 错误输出与副作用 | 独立 Python 子进程执行实际 main，分别给 cap / join 非法值，覆盖生成与检查两种模式；非零退出、stderr 含属性/文件/原值、stdout 为空，原 `paths.json` 字节不变。不是模拟成功返回。 |
| IG-04 | 正常分支 | 九种受支持组合和缺省值保留几何/样式；现有 54 图标 `--check` 通过，没有修改 SVG、manifest 或 paths.json。 |
| IG-05 | CI 接入 | 新测试名 `test_ci_icon_generator.py` 由现有 `python -m unittest discover -s tools -p 'test_ci_*.py' -v` 自动发现；现有生成器 `--check` 继续阻止不合法源文件进入构建。没有另加 workflow。 |
| IG-06 | 协议/功能边界 | 只改变构建工具输入校验；没有新 worker、Present、业务锁或请求 ID。stdout 上保留原有正常验证结果行，失败没有结果行；不把构建命令的 stdout 结果当成应用日志。 |
| IG-07 | 待处理 | Rust 仍有处理静态嵌入数据的现有 panic，手工篡改 paths.json 并绕开检查不在本次运行时容错范围；#138 的错误类型、音频时钟与其他不变量仍未完成，本 PR 不关闭合集。 |

本机环境为 Windows，使用现有 `.tools/ui-pr-validation-env/Scripts/python.exe -X utf8`，无新增依赖。测试 SVG 是代码自行生成的线段，不是第三方素材。生成器外的图标绘制、token 和 panels 没有修改，无用户可见样式变化；不将此项声称为整个 DESIGN 验收。

开始前核对近期分支、worktree 与开放 PR：#348 在改 paint 菱形输入，#349 在改属性筛选，#351 在做配置隔离，另有表达式沙箱分支；已合并图标几何分支无未提交修改。本次避开这些文件，仅改生成器、新工具测试和本文。

```powershell
python -X utf8 -m unittest discover -s tools -p test_ci_icon_generator.py -v
python -X utf8 tools/generate_desktop_icons.py --check
python -X utf8 -m unittest discover -s tools -p 'test_ci_*.py' -v
```

工具全集首轮 `icon-generator-ci-tools.log` 发现新增工具没有登记 inventory，运行 54 项，1 error / 1 skip；补齐 `tools/validation_inventory.json` 的 CI 自动发现分类，没有修改检查器或既有断言。复跑 `icon-generator-ci-tools-final.log` 为 **53 通过、1 项既有可选检查跳过**（未设置 `MOTION_EFFECT_VALIDATOR`，没有执行 Rust 效果 schema/WGSL 加载），无失败。新增测试是 CI 自动发现项，不据本机成功推断其他平台结果。
