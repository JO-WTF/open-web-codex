# ADR-009：Thread-first 数据接入与分阶段 Readiness

状态：已被 [ADR-018](018-built-in-network-copilot-runtime-closure.md) 替代（2026-08-08）；
下文仅保存历史决定，不得作为当前实现合同。Workspace 仍独立于 Thread/Run，但当前
数据路径是普通 Workspace 文件，不再使用 SourceAsset、Data Intake 或 Dataset binding。

## 背景

业务用户知道自己的目标（例如“进行印尼仓网规划”），但不知道平台内部
Dataset Release、文件角色或 manifest 格式。让用户在创建 Thread 前发布精确
Dataset 会把平台内部合同泄漏到产品界面；更严重的是，空的依赖列表可能被
readiness 当成“无需检查”，随后进入分析并在等待数据时重复运行。

## 决定

1. Thread 只复用已有且已授权的 Workspace。Thread/Run 不创建 Workspace、clone
   或 checkout；没有可用 Workspace 返回平台级 `workspace_unavailable`。
2. 原始上传是 Workspace-scoped `SourceAsset`，由不可变 revision 组成
   `WorkspaceDataDraft`。Draft 只表示上传批次和审计来源；同一 Workspace 的所有
   授权 Thread 都能发现 SourceAsset。它不是 Artifact，也不要求用户提交 Dataset ID、
   版本或内部 manifest。
3. Network Agent 根据用户目标发布带版本和内容 hash 的
   `data_requirement_profile.v1`，Data Agent 只读取有界 Workspace 画像并发布
   `source_profile.v1`、`mapping_proposal.v1`。平台持久化 `DataIntakeSession` 的
   evidence、缺口、用户确认、参数答案和输入 revision。模糊匹配永远只是候选；
   Profile、Mapping 和 Final Checklist 都必须整体确认。
4. 只有领域能力生成不可变 normalized Dataset Release，并建立带 contract、mapping
   和 parameter 快照的 `TaskDatasetBinding` 后，分析才可启动。原始 Draft/SourceAsset
   不得伪装成分析输入或 Artifact。
5. Readiness 分为 Thread、Input 和 Analysis 三层。Thread readiness 只检查平台、
   Profile/Runtime、Provider/model、执行定义和对话所需 capability；Input readiness
   持续表达数据接入状态；Analysis readiness 必须验证 immutable Release、binding
   及 fingerprint，且在 Task 级重新计算。
6. `active`、`ready`、`failed`、`cancelled` 是 Intake 生命周期；缺口和待确认项
   由 `DataIntakeInputRequest` 投影表达。一个 Intake Turn 有界结束；同一
   input/gap fingerprint 不得自动 spawn、wait 或 retry。
7. Supervisor Policy 只声明角色、Artifact 合同和动态委派原则，不声明固定工作流。
   当前正式 Network/Data/Visualization 合同分别为 4.0.0、4.0.0 和 2.0.0；旧
   Indonesia Tutorial 仍是独立样例，不是正常业务入口。
8. Workspace SourceAsset 的发现范围是当前授权 Workspace 全域。Task 只锁定实际被
   已确认 Mapping/Planning Dataset 采用的 Source snapshot；无关 Thread 的新文件不
   自动使当前分析指纹失效。

## 后果

- 用户可以在空 Workspace 中先获得 Thread，并在同一个 Thread 里上传文件、确认映射
  和回答业务参数；数据完成前分析保持 blocked。
- 平台保存更多可恢复状态，必须做 revision、幂等、并发拒绝、授权和 fingerprint
  校验。
- 领域 MCP/Capability Package 负责 Excel/JSON/CSV 语义画像和归一化；平台不在
  Browser 或启动脚本里复制业务 schema。
- 当前实现已提供 Workspace SourceAsset 原子写入、全域列表接口、结构化回答投影、
  有界 Excel/CSV/JSON MCP 画像、严格 `planning-dataset.v2`、通用 Network snapshot /
  optimization / report / map handoff 和对话内数据接入卡片；真实 Browser/Runtime
  恢复验证仍是后续 capability gate，未因此声明分析链路已可用。

## 验证

平台 Rust 编译、Browser typecheck 和状态/权限代码检查必须通过；发布前还必须用
真实 Runtime 与 capability package 验证空 Workspace → 上传 → 模糊映射 → 用户确认
→ 参数 → normalized Release → Analysis 的全旅程及重启恢复。
