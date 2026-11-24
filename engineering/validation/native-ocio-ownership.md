# 原生 OCIO 程序的独占移交

## 所有权边界

#83 的准备结果需要能由准备线程移交给消费线程。原生程序现在实现 `Send`，
仍不实现 `Sync` 或 `Clone`，不公开原生指针。移交的是完整所有者，不是把地址
转成整数跨线程传递。Session 的线程检查、GPU 对象归属和注册表策略没有改变。

安全依据是当前桥接的具体对象，而不是对所有 OCIO 对象作并发安全承诺：

- `tools/ocio-native/bridge.cpp` 的 Program 拥有 CPU processor、GPU shader
  description 和目录字符串。description 是着色器与查表数据，不是 GPU
  device、context、纹理或窗口。准备完成前不发布句柄。
- Rust 在准备时复制程序文本、查表和目录，外部不能修改原生对象。这里只允许
  所有者移动，不允许其他线程同时持有该程序的共享引用。
- Program 保留 `Arc<Api>`，保证最终调用 C++ release 时库仍在。Drop 调用一次
  release；OCIO CPU processor 与 description 的析构释放其拥有的数据。
- 固定 OCIO 2.6.0 提交 `10a43ec1df03d072ca9a9e2fb3f26e454b0900d8`
  的 [CPUProcessor 实现](https://github.com/AcademySoftwareFoundation/OpenColorIO/blob/10a43ec1df03d072ca9a9e2fb3f26e454b0900d8/src/OpenColorIO/CPUProcessor.cpp)
  中，当前 applyRGB 路径使用局部像素和处理器拥有的 CPU 运算；
  [GpuShader 实现](https://github.com/AcademySoftwareFoundation/OpenColorIO/blob/10a43ec1df03d072ca9a9e2fb3f26e454b0900d8/src/OpenColorIO/GpuShader.cpp)
  将描述及查表保存在拥有的字符串和向量中。桥接已经拒绝动态 uniform，
  不暴露更新接口。此次没有改变该固定源码或 C ABI。

上述实现审查支持当前独占移交约束；测试补充资格证据，不代替所有权论证。
进程全局运行时按原策略保留库，测试中释放调用者的 runtime 句柄不代表实际
卸载动态库。不得据此声称任意 OCIO 对象可以并发使用。

## 实际验证

证据目录：`artifacts/issue-goal-current/color-native-ownership/`。
Windows x86_64，固定已核验原生前缀，实际 Vulkan GPU。

- 20 个实际原生程序在创建线程、消费线程、返回线程上累计执行四轮原有
  240 个 CPU 参考样本，共 960 次比较；原始参考及容差不变。
  完整 source JSON 在移交后精确相同。消费线程逐程序触发 NaN 输入拒绝，
  保留原 typed cause，随后正确样本仍可执行；最终在第三个线程释放全部程序。
- 编译失败 doctest 证明程序仍不能满足 `Sync`。测试的预期 E0277 是该负对照
  的成功条件，不能当作产品编译失败或删除此约束。
- 实际准备的完整工程从准备线程移交至消费线程。删除配置文件后，在消费线程
  新建 GPU renderer，混合原生转换后的实心图层和半透明矢量；两个实际像素
  与原 CPU 参考合成公式比较。显示端点也实际执行，产品绘制不增加回读计数。
  再移交回来采样，陈旧颜色声明必须拒绝并保留上一帧；完整工程 JSON 不变，
  最后将完整准备结果移至第三线程释放。
- 原生完整集成八项、宿主完整集成二十项通过，后者仍包含受控阻塞、原 panic
  展开、失败不发布与恢复检查。新测试通过已有原生矩阵的 `native_color` 与
  `authored::` 过滤器执行，无需添加另一条长工作流或取消当前运行。

子进程测试看门狗仅用于报告不返回的原生/GPU 测试，不是产品超时、重试或
worker 重启策略。当前跨平台 CI 结果必须与新提交对应，本机通过不代替远端
资格，也没有证明此前原生访问冲突的根因或修复。

## 错误处理审计

| ID | 类别 | 证据与限制 |
| --- | --- | --- |
| NCO-01 | 功能边界 | 仅开放完整原生所有者的独占移动；没有 Sync、Clone、裸指针接口、环境覆盖或并发调用 |
| NCO-02 | 既有错误 | NaN 输入仍返回原 Admission，后续正确调用验证对象有效；Drop、原生异常边界与主要错误因果链不变 |
| NCO-03 | 既有观察 | 准备继续通过宿主 Flight 发布 Info start 与在途状态；本批没有新增准备、Present 日志、计数器或文件访问 |
| NCO-04 | 新资格 | 真实原生移交/返回/异线程释放，实际 GPU 合成与完整工程不变；静态 !Sync 负对照 |
| NCO-05 | 正常分支 | 移交、释放和正确采样不记故障，不自动重试或重启，不吞掉 panic |
| NCO-06 | 待接入 | 普通 managed Session 安装及其帧错误信道尚未完成；低层结果移交测试不能冒充完整应用验收 |

本批无 UI、依赖、第三方资产、固定库版本或原数值参考改动。
#83 仍开放；完成该 issue 后按用户要求停止，不开始其他 issue。
