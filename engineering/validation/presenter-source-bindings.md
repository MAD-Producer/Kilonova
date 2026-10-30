# 呈现源绑定复用验收

对应 #126 第七项中的 Presenter 绑定重建；不关闭包含矢量、LRU、图片顺序和首次编译等待等工作的合集。基线 `2ecfc4e8e6ac4cbe9d62c7d3acb60f2e99aca93f`。

`Presenter::set_source` 原先每次创建普通呈现、平移缩放两套 bind group，即使传入相同视图。本次保存唯一的当前 `wgpu::TextureView` 句柄，按 wgpu 对象身份比较；命中时复用两套绑定，换视图时一起重建。缓存只有一个条目，与原本 bind group 已持有的同一视图关联，不保存帧像素、不修改纹理预算、渲染表达式或项目格式。

`set_source` 即使命中，也仍清除 admission lease；`set_admitted_source` 随后恢复调用者的 lease，保持原先 raw/admitted API 边界。缓存视图字段在 lease 之前析构。既有命令和上传保留机制不变。

## 实际验证

新增测试使用真实 GPU、对象句柄与 readback，不使用模拟成功返回：

- 同一视图及其 clone 连续设置 120 次，两套 bind group 身份均保持不变，逻辑 admitted bytes 不增加。
- 同一纹理从红色更新为绿色后，缓存继续命中，普通呈现与平移缩放呈现都读回绿色，证明没有把旧像素缓存为新帧。
- 切换蓝色源时两套绑定同时更新；切回原源后重新得到绿色。
- 同一纹理新建另一个 TextureView 仍触发重绑，避免把不同视图语义混为纹理身份。
- raw 命中依旧 relinquish lease，admitted 再次设置依旧恢复 lease。
- 原有三项 presenter 验收继续覆盖第二个 uniform 的真实预算拒绝、已编码／未提交写入与 presenter 丢弃后的实际资源回收和导航像素。

复现：`cargo test -p motion-render --lib --locked --offline -- --test-threads=1`；默认 `gpu-tests` 开启，不能使用 CPU-only 忽略代替 GPU 验收。

## 边界与观测

- 新观察：无新增失败分支、等待、线程或日志策略。本次仅减少相同输入的绑定创建，既有 wgpu 验证和资源错误继续走原通道；缓存命中为正常分支，不增加逐帧 IO 或警告。
- 既有覆盖：实际执行预算拒绝及资源回收测试，不用“测试通过”替代对新故障信道的声称；本次没有新增故障信道。
- 功能边界：仍只缓存当前视图，A→B→A 会重建；不宣称解决全部渲染分配或达到特定 FPS 提升。
- 协作边界：开工检查模型库、后台预渲染、查看器 worktree。查看器分支也修改 presenter，但本次避开其通道／快照 API 和 uniform 数据；合并冲突探测结果另记录，不把无冲突等同于组合运行验收。
- UI：未修改面板、图标、样式或用户交互，DESIGN §0.7 的视觉整改项均保留在原范围；像素 readback 检查实际输出，不冒充全 UI／全 DPI 视觉验收。

## 最终执行记录

- 完整 `motion-render --lib`：**280 passed / 0 failed / 0 ignored**，95.50s，默认 GPU 功能开启。
- 临时将身份判断改成无条件重建，实际重新编译运行新增用例：在第一轮相同源设置时 bind group 身份断言失败，**0 passed / 1 failed**。随后按字节恢复源码，SHA-256 与备份一致；原先断言没有修改。
- 恢复后 presenter 四项实际 GPU 验收再次 **4 passed / 0 failed / 0 ignored**，7.33s；该数字是完整 280 项的子集，不重复相加。
- FFmpeg 桌面 build 和新程序 `--help` 成功；产品构建保留既有 28 条 warning，没有声称清理它们。
- 与 `feat/264-viewer-inspection` 的 `git merge-tree --write-tree` 返回成功，组合树 `69f9243b793f4379d4f495ecd34cd920fb12d8ea`。这只证明无文本冲突，尚未声称该未合并分支的组合运行通过。
- provenance 检查和 `git diff --check` 通过，无新增依赖／第三方资产。源代码恢复、产品二进制和证据包哈希见[结果 JSON](presenter-source-bindings-results.json)。

[原始日志](data/presenter-source-bindings.zip)：7 个文件，14,869 bytes，SHA-256 `93f84c80f565f03f121b880604d19c4af154c09f55861fe6adf7d3a5777b1b73`。包含完整通过日志、负面对照原始失败、恢复后通过、桌面构建和启动，以及查看器分支冲突探测。
