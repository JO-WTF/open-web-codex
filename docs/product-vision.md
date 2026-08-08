# open-web-codex 产品愿景

## 一句话愿景

> 让团队通过标准浏览器安全地使用 Codex，并在不重建 Agent Runtime 的前提下，
> 逐步把个人执行能力演进为可治理、可恢复、可追溯的企业多 Agent 协作平台。

## 我们要解决的问题

Codex 已经能够理解目标、维护 Thread 上下文、使用工具和 Skills，并组织子 Agent
执行。但当这些能力从个人终端走向团队和企业时，还会出现另一组问题：

- 用户如何从浏览器稳定访问同一份持久 Runtime 身份；
- 多个用户、Profile、Workspace、Secret 和企业数据如何隔离；
- 一次执行如何排队、取消、审批、恢复和审计；
- 代码、报告、数据集和可视化成果如何在任务结束后继续被授权访问；
- 专业 Agent 如何被评审、发布和选择，而不把角色名称当成可用能力；
- 多个 Agent 得到冲突结论时，谁负责补充调查、取舍并形成最终答案；
- Codex 官方能力持续变化时，平台如何复用上游而不是形成第二套 Runtime。

这些问题不能只靠更长的 Prompt 解决，也不应该被塞进 Codex 内部。产品的核心价值
正是把 Runtime 的认知执行能力与企业需要的身份、授权、控制和治理连接起来。

## 目标用户

第一阶段面向需要在团队环境中使用 Codex 的可信研发团队：

- Developer 使用 Agent 完成编码、分析和交付；
- Algorithm Engineer 把业务算法封装为 Tool，并组合 Skill、Domain Agent 和 Supervisor；
- Reviewer 审查执行过程、审批和变更；
- Project Admin 管理项目、成员、策略和 Workspace；
- Platform Admin 维护 Profile、Runner、版本与容量，但不默认读取业务内容。

随着企业多 Agent 能力成熟，平台进一步服务需要多个专业角色共同完成复杂分析和
决策准备的团队，例如供应链规划、数据分析、财务测算、风险评估和架构治理。

目标不是让模型替代业务责任人，而是让专业能力围绕同一目标协作，保留依据、假设、
过程和最终责任。

## 产品承诺

### 浏览器是完整入口

核心工作流只依赖标准浏览器和服务端平台。用户不需要安装桌面程序、本地 daemon
或浏览器扩展，也不会直接连接 Codex app-server。

### Codex 继续拥有认知执行

Thread、Turn、模型可见上下文、compaction、memory、Agent、Tool、Skill、Plugin
和 MCP 由 Codex Runtime 解释和执行。平台不建设兼容 Runtime，也不保存第二套模型
上下文。

### 企业平台拥有确定性控制

用户、组织、Profile、Workspace、Task、Run、授权、审批、Secret、Artifact、
审计和恢复由平台负责。模型可以参与规划和判断，但不能决定自己的身份、权限或
审计结果。

### 成果能够脱离执行过程长期存在

Artifact 具有独立身份、授权和保留周期。创建它的 Run、Thread、Turn 和 Item
用于证明来源，而不决定成果还能否被读取。

### 官方 Codex 可以持续同步

产品定制集中在少量、明确、可测试的 seam。能够通过平台合同、Skill、Plugin 或
MCP 完成的能力，不扩散到 Codex 高变化模块中。

## 演进路径

产品按问题成熟度演进，而不是按 Agent 数量演进。

### 第一阶段：Codex Web Workbench

先建立可信的浏览器纵向闭环：

- 持久 Profile 与授权 Workspace；
- Task/Run、审批、事件、恢复和审计；
- Provider、模型、Git 与 Runtime 能力的安全桥接；
- 浏览器刷新、服务重启和执行失败后的可解释状态。

这一阶段证明平台可以复用 Codex，而不改变 Codex 的 Thread 和执行语义。

### 第二阶段：单 Profile Copilot 创作平台

在一个用户和一个持久 Profile 上交付平台化的多 Agent 创作与运行闭环：

- 算法工程师通过 SDK 创建规范化 Tool，并通过受控发布和安装让 Runtime 发现；
- 用户在 Web 编写中文 Skill，定义方法、输入、Tool、失败处理和交付件；
- 用户创建 Domain Agent，组合 Skills、Tools、数据权限和交付合同；
- 用户创建 Supervisor，选择精确 Agent Releases，定义总体责任和动态协作原则；
- 平台自动处理 Draft revision、Release version、hash、依赖锁、安装和 readiness；
- 根 Thread 使用平台提供的只读协调能力观察 Agent、用户输入、共享工作状态和交付件；
- 印尼仓网和第二个非供应链案例共同证明扩展能力不是领域硬编码。

这一阶段建设的是有界、可验证的 Copilot Studio，不是任意代码托管平台，也不是第二套
Agent Runtime。Runtime 仍拥有 Thread、上下文、spawn、wait、Skills、MCP 和 Tool 执行；
平台拥有 Catalog、授权、安装事务、工作状态元数据、执行投影和 Artifact。

### 第三阶段：受治理的多用户 Agent 平台

当单 Profile 创作和运行闭环稳定后，再开放：

- 多 Profile 路由和跨用户隔离；
- 组织级 Catalog 可见性、发布、弃用、评价和授权；
- 有界候选查询与 Runtime 可用性验证；
- 企业 Tool 的不可伪造执行授权上下文；
- Artifact 的跨 Run 复用、保留和依赖关系。

### 第四阶段：持续决策系统

只有当生产数据反复证明 Artifact 与有界摘要不足以表达长期事实、假设、冲突和
决策时，才引入 Task Knowledge Ledger。只有身份、权限、恢复、评价和知识边界都
稳定后，才考虑 Agent Decision OS、事件触发重评和长期情景管理。

这不是默认承诺，更不是用无限 Blackboard 保存模型思维过程。

## 长期不变的边界

| 事实 | 长期所有者 |
| --- | --- |
| 用户、组织、授权、审批和审计 | Platform |
| Profile 身份和进程生命周期 | Platform + Profile Host |
| Workspace 授权与托管 Git 生命周期 | Platform / Workspace / Git |
| Task、Run、租约和恢复 | Platform |
| Thread、Turn、Context 和 Agent 执行 | Codex Runtime |
| Skills、Plugins、MCP 和 Tool 执行语义 | Codex Runtime |
| Tool、Skill、Agent、Supervisor、Copilot 的 Catalog 与发布治理 | Platform |
| Runtime Role 的发现和应用 | Codex Runtime |
| Profile Installation 与 Runtime readiness 投影 | Platform + Profile Host |
| 通用 Work State 元数据 | Platform |
| 领域 Work State payload 和业务算法 | Domain Tool package |
| Artifact 身份、授权和保留 | Platform Artifact Store |
| 企业数据访问决定 | 企业 Tool/MCP 边界 |

这些所有权不会因为产品阶段、页面设计或临时实现便利而改变。

## 成功是什么

产品成功不能只用“运行了多少个 Agent”衡量。长期应关注：

- 用户能否通过浏览器完成真实工作并形成可交付结果；
- 页面刷新、网络中断和服务重启后，执行能否恢复到同一事实；
- 关键结论能否追溯到数据、Artifact、策略版本和人工决定；
- 不同用户、Profile、Workspace、Secret 和企业数据是否保持隔离；
- Supervisor 是否能够发现证据缺口、处理冲突并及时停止；
- Codex 上游升级是否能够以有限、可解释的本地差异持续完成；
- 新能力是否通过真实合同和端到端验证，而不是由 UI 或文档提前宣称。

## 明确不做什么

- 不建设第二套 Codex、Thread Store、Memory Engine 或 Agent Scheduler；
- 不让浏览器直接访问 Runtime 协议、服务器路径或凭据；
- 不把 Workspace 绑定为某个 Thread 或 Run 独占的 checkout；
- 不把 Prompt 当作权限系统；
- 不把企业 Agent 目录记录当作正在运行的 Agent；
- 不默认建设无界 Blackboard、自由 swarm 或自动执行企业决策；
- 不用 Web 专属实现破坏 Codex TUI 和官方上游同步能力。

## 与其他文档的关系

本文只定义稳定的产品方向。当前 V1 的具体要求以
[产品设计](product-design.md) 为准；长期多 Agent 架构的详细推演见
[企业多 Agent 平台架构](enterprise-agent-platform-architecture.md)；当前代码事实
见 [系统架构](architecture.md) 和 [能力基线](capability-baseline.md)；当前接受的
阶段顺序见 [路线图](roadmap.md)，近期任务见 [开发计划](development-plan.md)。
