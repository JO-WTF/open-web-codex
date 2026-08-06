# open-web-codex 产品与工程路线图

| 字段 | 内容 |
| --- | --- |
| 状态 | 当前接受的阶段顺序 |
| 更新时间 | 2026-07-30 |
| 时间表达 | 以能力门和结果为阶段，不承诺未经评估的日期 |
| 产品方向 | [产品愿景](product-vision.md) |
| 架构依据 | [企业多 Agent 平台架构](enterprise-agent-platform-architecture.md) |
| 当前能力 | [能力基线](capability-baseline.md) |
| 近期任务 | [开发计划](development-plan.md) |
| M2 详细实施 | [Enterprise Supervisor Copilot 短期实施计划](enterprise-supervisor-copilot-plan.md) |

路线图描述中期交付顺序，不保存逐文件任务和完成日志。实现切片可以在所有权和依赖
明确时并行，但阶段只有在真实边界上的退出证据成立后才能结束。当前采用功能优先：
M2 的功能切片可以与 Gate 0 剩余可信工作并行，但未通过相关 Gate 0 门禁时，不能把
功能演示声明为可信发布能力。

## 顺序总览

```text
Enterprise Supervisor Copilot 功能主线
  + Gate 0 可信门禁并行收口
  -> 受治理的多用户执行
  -> Capability-gated Studio
  -> 生产 GA
  -> 按真实需求决定是否建设 Task Knowledge Ledger
```

| 阶段 | 核心结果 | 当前位置 |
| --- | --- | --- |
| Gate 0 | 当前 Workbench 的数据库、Workspace 与 Runtime 证据可信 | 并行可信门禁 |
| M2 | 单 Profile 上完成可治理的 Supervisor + Domain Agents 闭环 | 受限 happy path 已验证；阶段退出门禁待补 |
| M3 | 多用户、多 Profile 与企业能力治理成立 | 后续 |
| M4 | 只开放经过合同和真实验证的 Studio 能力 | 后续 |
| M5 | 满足安全、运维、恢复和可用性门禁 | 后续 |
| 条件阶段 | Task Knowledge Ledger / Agent Decision OS | 未排期 |

## Gate 0：恢复当前平台证据

### 目标

让当前 Codex Web Workbench 的实现、迁移、文档和验证重新指向同一事实，为后续
多 Agent 工作提供可信底座。Gate 0 不再阻塞所有 M2 功能编码；与当前切片直接相关
的授权和语义问题必须随功能解决，其余验证债务保留为 M2 发布门禁。

### 重点结果

- 空白 PostgreSQL 数据库能够完成全部迁移、重启和 ignored integration tests；
- 独立 Workspace 身份、授权、Run 引用和显式生命周期在真实数据库中成立；
- 共享 Workspace 并发、跨用户拒绝、恢复和删除阻断拥有证据；
- Codex Thread `cwd` 只能位于授权 Workspace，真实 multi-`cwd` 行为得到验证；
- 当前单 Profile Runtime、Provider、MCP、Artifact 和浏览器纵向链路保持通过；
- 通过专用分支同步已观测到的官方 Codex 更新，重新验证全部 retained seams；
- Chat/Responses 转译不再用 Tool 是否存在推断普通文本 phase；
- 能力基线不再引用迁移改写前的验证结果。

### 退出条件

能力基线可以明确区分已验证能力与剩余缺口，开发计划中的数据库和 Workspace
门禁有可复现证据，且没有 Thread/Run 隐式拥有 checkout 的路径。退出条件未满足
时，可以演示受限单 Profile 功能，但不能宣称 M2 已可信完成。

## M2：Enterprise Supervisor Copilot

### 目标

在一个授权 Profile 和一个真实企业用例中，完成“根 Supervisor + 少量 Domain
Agents + 受限企业 Tool + 持久 Artifact”的最小协作闭环。

### 核心交付

| 能力 | 阶段结果 |
| --- | --- |
| 多 Agent Runtime | 真实 spawn、follow-up、wait、interrupt、完成后继续执行和父子 Thread 轨迹可观察、可恢复 |
| Supervisor | 版本化 Supervisor Policy 作为不可变快照绑定根 Thread |
| Domain Agent | 可评审 Agent Definition 映射到 exact Runtime Role，并由 Supervisor 按证据缺口选择 |
| 企业能力 | 只读数据、确定性网络计算与地图 MCP，使用明确的 Task/Profile 资源范围 |
| Artifact | 独立身份、Schema、provenance 和同 Task 跨子 Thread 授权读取 |
| 结果责任 | Supervisor 处理冲突、缺口和部分失败，最终报告引用关键 Artifact |

### 明确不包含

- 完整原生 Agent/Skill/Plugin/MCP 生命周期或面向所有人的无界 Studio；
- Runtime Role 级动态数据权限，除非不可伪造执行身份已经验证；
- 多组织生产部署；
- Task Knowledge Ledger 或长期 Blackboard。

### 退出条件

真实用例在刷新、失败和 Profile 重启后仍指向同一 Agent 轨迹、Policy 版本和
Artifact；关键结论可追溯，一个 Agent 失败时能够形成明确的部分结果或终态。新用户
还必须能从空 Workspace 只通过生产 `/web` 完成数据发布或示例安装、类型化 readiness、
受治理启动、审批、交付审阅和刷新恢复。缺少必需依赖时不得先创建 Thread，再让模型向
用户索要内部 ID、Runtime 标识或服务器路径。

当前实现是通用仓网动态案例：组织 Draft `Indonesia Network Planning Copilot`
（`enterprise-supervisor-copilot@5.0.0`）可从 Data `5.0.0`、Network `5.0.0` 和
Visualization `2.0.0` 三个有界 Role 中按证据缺口选择能力；当前没有仓库级
Supervisor Release。十五个 Artifact handoff 是条件性证据边而不是固定 Workflow。Workspace
全域 SourceAsset 发现、Excel/CSV/JSON 有界画像、模糊映射、严格
`planning-dataset.v2` 和通用优化/报告/地图工具已有本地聚焦验证；完整真实
Runtime/browser 回归、重启恢复和无重复 Analysis Turn 仍待通过，因此能力基线继续
声明 unavailable。生产入口、审批拒绝、Agent 部分失败、Artifact 完整生命周期和
一般故障恢复仍是 M2 退出门禁。

M2 的里程碑状态保留在 [开发计划](development-plan.md)，逐切片工作、真实案例和
活动可信风险由
[Enterprise Supervisor Copilot 短期实施计划](enterprise-supervisor-copilot-plan.md)
维护。

## M3：受治理的多用户执行

### 目标

把单 Profile 协作闭环扩展为多用户、多 Profile、可发布和可隔离的企业能力平台。

### 核心交付

- 按授权用户动态路由持久 Profile；
- HttpOnly Cookie、CSRF、Session 轮换、吊销和登录限速；
- 两用户并发的 Profile、Thread、Workspace、Event、Approval、Secret 和 Artifact
  隔离矩阵；
- rootless Runner、出网策略、配额和进程/文件系统强隔离；
- Agent Definition、Capability、发布、弃用、评价和授权策略；
- “企业已经发布”与“当前 Profile 的 Runtime Role 可发现”分别表达；
- 有界 Agent 候选查询，不向 Supervisor 暴露整张目录；
- Runtime 到企业 Tool/MCP 的不可伪造执行授权上下文；
- Artifact 跨 Run 复用、保留、替代和依赖失效；
- 显式 Push、保护分支策略和审计。

### 退出条件

两个用户和两个 Profile 在并发、重启、错误注入和猜测资源 ID 的条件下不能互相
读取或控制资源；Agent 发布、Runtime 可用性和执行授权可以分别审计和失败。

## M4：Capability-gated Studio

### 目标

把已经拥有稳定 Codex 合同和真实验证的管理能力逐项开放给用户，而不是先建设一个
看起来完整的 Studio。

### 候选能力

- MCP inventory、配置、OAuth 与安全 elicitation；
- Plugin 安装、升级、权限变化和卸载；
- Memory 健康、导出与重置；
- Agent Definition / Runtime Role 的发布状态和可用性视图；
- Skill 读取、验证、测试、发布和回滚；
- Artifact renderer、Schema、下载和授权管理。

### 退出条件

每个开放模块都有生成合同、Capability Manifest、授权策略、失败语义、fixtures 和
真实 Runtime smoke；不支持或未验证的操作在产品中明确禁用。

Studio 不成为绕过 Codex discovery、Profile、Runner 或平台授权的配置写入入口。

## M5：生产 GA

### 目标

让系统具备可持续运行、恢复、升级和审计的生产条件。

### 核心交付

- Artifact、日志、事件和审计的保留策略；
- PostgreSQL、Profile Home、Workspace 与 Artifact 的备份恢复和灾难演练；
- HTTPS、Secret Manager、密钥轮换与安全评审；
- 容量、可观测性、告警、资源限制和故障处置；
- rolling upgrade、Capability canary、兼容性判断和回滚；
- 完整 Diff/File/Logs/审查体验、可访问性和目标视口 E2E；
- 发布物 provenance、SBOM 和由非作者执行验证的运维手册。

### 退出条件

[产品设计](product-design.md) 中的 GA 验收、安全目标和可恢复性要求全部拥有
生产形态证据，而不是只在 Fake Runtime 或单进程开发环境中通过。

## 条件阶段：Task Knowledge Ledger 与 Agent Decision OS

这两项能力不随 M5 自动开始。

Task Knowledge Ledger 只有在生产数据反复出现以下问题时才进入立项：

- Artifact 无法有效表达细粒度事实、假设、冲突和替代关系；
- Supervisor 多次重复整理相同证据；
- 跨 Run 决策无法判断依据是否过期；
- 有界摘要不足以支撑长期复核。

Agent Decision OS 还要求身份、权限、恢复、评价、Artifact 和 Ledger 都已经稳定，
并且存在明确的业务责任人、重评触发条件和停止规则。它不意味着模型自动执行企业
决策，也不保存无限模型记忆。

## 路线图变更规则

阶段顺序只有在以下证据之一出现时调整：

- 前置能力不能消除后续阶段的关键风险；
- 真实用户数据证明另一项能力是纵向闭环的必要条件；
- Codex 官方能力变化显著降低或提高某阶段成本；
- 安全、合规或恢复门禁要求前置。

路线图变更需要同步复审产品愿景、产品设计、企业架构报告和开发计划。已经完成的
过程不留在本文件中；当前能力进入能力基线，长期决定进入 ADR。
