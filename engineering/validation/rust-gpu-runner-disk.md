# Rust/GPU runner 磁盘不足修正

#608 的提交 421bb2d32e4b5e0bbec3fc612baa96d355c6a6f7，运行 41954512361 的 Rust and GPU 任务 127007911253 在执行 Rust 测试时失败。检查注释的原始原因是 `System.IO.IOException: No space left on device`，runner 自身的 Worker 日志也无法写入。这不能据此判定某个产品断言失败。原始注释保存在 `rust-gpu-runner-disk-evidence.json`。

桌面 Linux 任务已有构建前磁盘清理，Rust/GPU 任务缺少这一阶段。现在沿用同样的固定目录清理，仅移除该任务不使用的托管 Android 与 .NET SDK，在安装依赖和恢复 Rust 缓存之前打印前后磁盘用量。保留 Rust 缓存、构建结果、测试产物和故障日志；不修改测试选择、断言、编译配置、超时或错误返回值。

本地使用仓库已固定的 PyYAML 依赖执行 `python -m unittest discover -s tools -p 'test_ci_*.py' -v`：111 项执行、110 项通过、一个既有跳过、零失败。首次未配置本地依赖目录时，两项测试模块因缺少 yaml 无法导入；补上已有依赖路径后原测试通过，未修改断言。差异空白检查通过。

这验证工作流结构与发布门禁，不能代替 Linux runner 上实际释放空间的结果。修正后的独立 CI 仍需验证；未重跑或伪称原失败任务通过，也未将其余运行中的任务算作成功。
