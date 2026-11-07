# #83：直接 OCIO 图片纹理处理

## 交付与边界

上游 #644 已以八项必需检查全部成功的准确 head 合入，#646 已改为面向 main。
本批在 `feat/color-image-transform` 接续 #646，未修改其他工作树的 UI、深度、
跟踪或预渲染实现。根工作树及其他分支只在归属/依赖清单等公共文件存在交集；
本批不修改这些清单，也不评审他人 PR。

新增 `compile_image`、直接 fragment 纹理路径与 `ColorProcessorOutput`。
公共 GLSL/table 准备逻辑复用，原 compute ABI、数值表插值和 dispatch 长度
守卫保留。图片片元按二维坐标加载 Float32 数值，不经硬件 sRGB 传递、过滤、
整图中间 buffer 或逐帧 CPU 读回；alpha 策略来自明确的已验证处理器。

不可变表在 GPU 创建前通过已有预算准入；原 encoder/submit 协议保留租约，
未提交、放弃、实际提交完成的生命周期分别验证。重复 encode 不增加表或图片
资源。源和目标等大且不能别名，所属 renderer/encoder、目标声明与 alpha 均
在绘制前检查。数值/覆盖率/深度/法线拒绝进入颜色变换。

本批仅接受 Float32 输入和输出。Float16 溢出/量化策略、完整工程/资产空间
标记、宿主命令与默认日志、应用显示/输出接线仍需完成；没有把 Linear/sRGB
历史标签冒充完整 OCIO 空间，没有改变现有应用渲染结果或 PNG 基线，也不关闭
完整 #83。不声明 HDR 显示、Metal、移动端或一般用户配置已获资格。

## 证据

本地证据目录：`E:/Dev/aem/artifacts/issue-goal-current/color-image-transform`。
运行器冻结实际源码并记录前后 SHA-256、命令、head、后端与原始 stdout/stderr。
首次 rustfmt 在新测试模块文件尚未写入时拒绝解析模块；补齐实际测试后通过。
这属于准备顺序错误，不算产品失败或有效负对照。

- `first-effects`：三份配置、20 个真实 OCIO 程序均编译为 image WGSL，再经
  独立 parser/validator 检查；compute 与 image 表值一致，入口/资源语义明确。
- `first-vulkan`：3 项 GPU 测试通过。20 个组合各用 3×4 多行图片，先确认输入
  上载/读回与 Float32 原值和行序精确一致，再各复用 3 次。每次 240 像素与
  原冻结 OCIO CPU 参考比较，alpha 位模式精确保留。
- 沿用既有容差：绝对 0.0001 + 相对 0.0002，未修改参考值或断言。Vulkan
  首轮最大容差占用约 0.103。独立检查一维假设、透明度、各类非法目标、
  所属错误、预算字节数、重复编码无新准入和实际提交/放弃后的释放。
- `native-errors`：实际错误 WGSL 的 wgpu Validation cause 保留；外层不泄露
  测试源内容，表准入全部释放，失败后可正常重建。真实限额不足保留 typed
  budget refusal，恢复限额后可正常创建，两项通过。

## 错误处理审计

| 项目 | 类型 | 证据与限制 |
|---|---|---|
| CI-01 | 新增 parse/validation 返回 | 复用原 Naga/JSON 类型；新增最终 image WGSL parse source。真实原生配置正常编译；最终 writer/parse 内部构造失败尚未单独注入 |
| CI-02 | 新增 GPU 创建错误 | 默认 MOTION_LOG=info 下实际错误 shader、原始 wgpu source/downcast 与受限外层文字；无应用命令，未声称 file/stderr 信道已接通 |
| CI-03 | 预算功能边界 | 实际拒绝、typed resource_rejection、原字节数不变、恢复后成功；native 次要故障与首要预算原因均可保留，双重故障未独立注入 |
| CI-04 | 图像功能边界 | 数据/深度/法线、错误 alpha/精度/目标/所有权和别名在绘制前返回 Err；拒绝不改变目标图片 |
| CI-05 | 正常路径 | 复用、无表、放弃未提交和健康完成不是故障，不增加 Present 日志或每帧文件 IO |
| CI-06 | 待接业务信道 | 宿主准备命令、非 Present start/in-flight、超时/阻塞观察及完整工程空间仍待后续接线；没有新重启、重试或隐藏错误 |

## 最终冻结与负对照

- `effects`：136 项不同 Rust 测试通过，0 失败/忽略。
- `dx12`：图片、旧 LUT、SDR 输出、compute、原始读回共 24 项通过；直接图片
  20 个组合的最大容差占用约 0.170。另以 `dx12-errors` 验两项真实原生失败。
- `vulkan-regression`：旧 LUT/compute 6 项通过。与首轮 Vulkan 图片/错误验证
  合起来覆盖新旧路径。重复后端或恢复检查不重复计数，共 162 项不同 Rust 测试。
- `ffmpeg-check`：render/host/desktop 的 locked、all-targets、desktop/ffmpeg
  编译通过，既有警告保留；没有因为警告或观察超时重启构建。
- 三项负对照只修改一处生产语句，原测试、CPU 参考、容差不变，均退出 101
  并触发原断言失败：`wrong-image-row` 检出第二行像素错读；
  `missing-image-premultiply` 检出 alpha 边缘的颜色差异；
  `missing-table-lease` 检出表字节未保留在预算中。不是语法编译失败。
- 负对照后两个源码原件逐字节恢复：image 编译器 SHA-256
  `327b3146e60f58d77a50beb9c3a7e629fa9a03b4cd7782036d48949bf5db51bf`；
  render 绑定 SHA-256
  `8c566a9f693cf4a724948712861ae27b2ed20f84b3b149ccef39d12d7c134634`。
- 恢复后的 `recovered-effects` 7 项和 `recovered-native` 的 3 项 DX12 图片
  测试全部通过；交付验证器核对完整冻结源码及实际提交字节，
  再产生含成员 SHA-256 的本地验收包。原测试断言没有为通过验收而修改。

冻结哈希基于实际编译的工作副本，交付另存 Git 索引字节；二者仅允许 Git 的
CRLF→LF 标准化，验证器拒绝其他差异。
