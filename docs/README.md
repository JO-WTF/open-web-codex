# open-web-codex 文档导航

这组文档共同回答五类不同问题：产品最终去哪里、目标架构为什么这样设计、当前
代码真实是什么、下一步按什么顺序建设，以及系统如何运行和验证。

它们不是同一份“大而全”文档的拆页。每篇文档只拥有一种主要时间视角和一种权威
责任，避免长期目标、当前事实与计划状态互相覆盖。

## 文档分层

| 层级 | 权威文档 | 回答的问题 | 不负责 |
| --- | --- | --- | --- |
| 产品北极星 | [产品愿景](product-vision.md) | 最终服务谁、解决什么问题、形成什么产品 | V1 功能清单、当前代码状态 |
| 产品要求 | [产品设计](product-design.md) | 当前 V1 的用户流程、功能、状态和验收标准 | 证明能力已经实现 |
| 目标架构推演 | [企业多 Agent 平台架构](enterprise-agent-platform-architecture.md) | 为什么需要这些能力，方案如何比较并收敛 | 充当当前代码说明或任务清单 |
| 多 Agent 专项演进 | [协同演进](multi-agent-collaboration-evolution.md) 与 [信息交换演进](agent-information-exchange-evolution.md) | 两条核心能力怎样分阶段演进、当前处于哪里、每阶段怎样验收 | 改写全项目路线图或提前声明条件能力 |
| Agent 能力分层 | [Supervisor、Agent、Skill 与 Tool](supervisor-agent-skill-tool-architecture.md) | Agent 能力栈与 Profile、Workspace、Thread、Agent Catalog 怎样连接 | 把治理目录、Runtime 实例和企业权限混为一体 |
| 当前架构 | [系统架构](architecture.md) | 当前系统组成、事实所有者和运行边界 | 保存长期设想和实现历史 |
| 安全边界 | [安全模型](security-model.md) | 资产、信任边界、授权链和不可削弱的安全属性 | 宣称某项门禁已经通过 |
| 能力证据 | [能力基线](capability-baseline.md) | 当前构建中什么已实现、验证到什么程度、还缺什么 | 描述产品愿景或未来任务 |
| 中期顺序 | [产品与工程路线图](roadmap.md) | 各阶段交付什么结果、进入和退出条件是什么 | 保存逐文件任务和完成历史 |
| 近期执行 | [开发计划](development-plan.md) | 当前与下一个里程碑的任务、阻塞和验收 | 重复全部长期路线图 |
| M2 专项执行 | [Enterprise Supervisor Copilot 短期实施计划](enterprise-supervisor-copilot-plan.md) | 单 Profile 多 Agent 功能闭环怎样分片实现、如何验收、哪些可信风险暂缓但必须登记 | 改写中期阶段顺序或宣称能力已经成立 |
| 运行操作 | [本地运行手册](mvp-runbook.md) | 如何启动、配置、验证和排错 | 定义产品或架构 |
| 开发约束 | [项目 Agent Guide](../AGENTS.md) | 修改代码时必须遵守的所有权、流程和验证规则 | 代替面向读者的产品文档 |

## 专项文档

### Codex 上游与本地修订

- [Codex 上游同步手册](codex-upstream-sync.md)：如何同步官方
  `openai/codex`。
- [本地 Codex Patch Map](custom-codex-patch-map.md)：必须保留的本地 seam、原因、
  影响文件、验证方式和退出条件。
- Git 历史记录每次具体修改；Patch Map 不承担提交日志职责。

### 协议与实现规格

- [Chat/Responses 转换规格](chat-responses-translation-spec.md)
- [Chat/Responses 转换计划](chat-responses-translation-plan.md)
- [app-server 事件适配](app-server-event-adaptation.md)

这类文档服务一个明确合同。合同成为当前实现后，应同步更新系统架构和能力基线；
完成过程不继续堆入规格正文。

### 扩展指南

- [领域 Agent 扩展架构](domain-agent-extension-architecture.md)
- [Skills、MCP 与自定义 UI 扩展](custom-skills-mcp-ui-guide.md)
- [零基础多 Agent 开发教程](multi-agent-development-tutorial.md)
  - [第 1 步：Hello Agent 第一次真实 Tool 调用](tutorials/hello-agent-quickstart.md)
  - [第 2 步：Hello Team 在 Web 发布 Supervisor](tutorials/hello-agent-team.md)
  - [第 3 步：运行并审阅企业仓网 Supervisor](tutorials/supply-chain-agent-tutorial.md)
  - [第 4 步：审批与故障恢复](tutorials/approvals-and-recovery.md)

扩展指南可以提供模式和示例，但不能重新定义 Profile、Workspace、Artifact、
Agent Definition、Runtime Role 等核心术语。

### 当前纵向切片

- [Enterprise Supervisor Copilot 短期实施计划](enterprise-supervisor-copilot-plan.md)：
  当前单 Profile 多 Agent Copilot 的详细工作包、真实企业案例、功能完成标准和活动
  可信风险。功能完成后，其事实进入能力基线；已经解决的过程不长期保留在计划中。
- [Agent、Skill 与 MCP 原生生命周期整改计划](agent-capability-lifecycle-plan.md)：
  从当前 V2-only 企业案例过渡到 Runtime 原生 Agent、Skill、MCP Studio 的跨里程碑
  整改顺序、所有权、合同和退出条件。

### 多 Agent 核心设计

- [多 Agent 协同：从单一执行到可治理协作](multi-agent-collaboration-evolution.md)：
  以目标责任、动态分工、异常恢复和最终综合为主线，给出从单 Agent 基础到条件性
  Agent Decision OS 的里程碑、细致目标和当前位置。
- [Agent 信息交换：从消息传递到可追溯的协作事实](agent-information-exchange-evolution.md)：
  区分 Runtime 消息、Thread Context、Artifact 和可选 Task Knowledge Ledger，说明
  为什么先采用 Artifact First，以及完整成果生命周期怎样演进。
- [Supervisor、Agent、Skill 与 Tool 的分层架构](supervisor-agent-skill-tool-architecture.md)：
  解释认知协调、专业角色、方法说明和确定性工具的分层，并把它们与 Profile、
  Workspace、Thread、Task/Run、Artifact 和 Agent Catalog 放在同一张架构图中。

这三篇文档细化企业架构报告中的两条核心能力和实现分层。它们可以记录当前阶段标记，
但“是否已经实现”的最终判断仍以能力基线和代码验证为准；全项目阶段顺序仍由路线图
负责。

### 架构决策

[ADR 索引](adr/README.md) 保存已经接受且会长期约束实现的决策。目标架构报告中的
方案比较不自动成为 ADR；当某项选择进入代码实施时，再创建简短 ADR，引用报告中的
论据并记录最终决定、后果与替代方案。

## 事实与冲突处理

不同问题使用不同权威来源：

1. “产品为什么存在、最终去哪里”以产品愿景为准。
2. “V1 要交付什么”以产品设计为准。
3. “当前代码由谁拥有、如何连接”以系统架构和代码为准。
4. “当前能力能否对外声称可用”以能力基线及其验证证据为准。
5. “接下来先做什么”以路线图和开发计划为准；当前 M2 的逐切片实现和风险登记以
   Enterprise Supervisor Copilot 短期实施计划为准。
6. “为什么接受某个长期技术决定”以 ADR 为准。

代码、生成合同和可复现测试是实现事实的最终证据。文档与代码冲突时，不能默默把
目标描述成现状：先确认事实，再更新对应权威文档并明确验证缺口。

## 更新规则

| 发生变化 | 必须复审 |
| --- | --- |
| 目标用户、产品边界或长期价值变化 | 产品愿景、产品设计、路线图 |
| 所有权、组件关系或持久化边界变化 | 系统架构、安全模型、相关 ADR |
| Runtime、Web 或平台能力新增或降级 | 能力基线、开发计划 |
| M2 工作切片、功能验收或可信风险变化 | Enterprise Supervisor Copilot 短期实施计划；形成证据后再更新能力基线 |
| 多 Agent 协同方式、Agent 责任或异常收敛规则变化 | 多 Agent 协同演进、能力基线、相关计划 |
| Agent 交换合同、Artifact 或 Ledger 边界变化 | Agent 信息交换演进、系统架构、能力基线 |
| Supervisor、Agent、Skill、MCP 或核心对象关系变化 | Agent 能力分层、系统架构、相关 ADR |
| 阶段顺序或进入条件变化 | 路线图、开发计划 |
| 启动参数、部署拓扑或故障处理变化 | 运行手册 |
| `codex/` 非生成差异变化 | Patch Map、相关测试、必要 ADR |
| 协议或浏览器 DTO 变化 | 规格、生成物、系统架构、能力基线 |

Canonical 文档只描述当前有效的目标、事实、顺序或操作。已经被替代的过程说明留在
Git 历史；只有仍然约束未来实现的决定进入 ADR。
