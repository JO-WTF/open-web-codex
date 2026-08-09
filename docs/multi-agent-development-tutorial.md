# 从 Web 单 Agent 到动态多 Agent Supervisor

状态：冻结迁移输入（2026-08-08）。

本组教程记录旧仓网原型的页面与合同，不是当前可执行的新手入口。阶段一完成后必须按
“普通 Workspace 文件 -> 内置能力原生 discovery -> Task”的正式链路整体重写；在此之前
不得依据教程继续增加 Dataset、Artifact 数据交接、capability template、Workspace 安装、
路径扫描或手工版本/hash。当前能力以 [能力基线](capability-baseline.md) 为准，目标合同以
[ADR-018](adr/018-built-in-network-copilot-runtime-closure.md) 为准。

教程质量标准、对象分工和完整学习路径见：

[Web 新手教程标准与学习路径](tutorials/README.md)

## 主路径

| 顺序 | 教程 | 完成后的真实结果 |
| --- | --- | --- |
| 1 | [单 Agent 配送承诺审计](tutorials/web-single-agent-delivery-audit.md) | 一个 Web 发布的 Dataset、Python MCP/Skill、Agent Release 和真实 Tool 调用 |
| 2 | [印尼仓网 1：数据](tutorials/indonesia-network-01-data.md) | 一个经过全量校验的 240,000 客户 Dataset Resource |
| 3 | [印尼仓网 2：时效](tutorials/indonesia-network-02-service-baseline.md) | 两个 Agent 通过 inspection Artifact 完成单层时效分析 |
| 4 | [印尼仓网 3：成本](tutorials/indonesia-network-03-two-level-cost.md) | 当前两级成本与一个指定候选的可对账比较 |
| 5 | [印尼仓网 4：优化与地图](tutorials/indonesia-network-04-optimization-map.md) | 三 Agent 动态 Supervisor、完整有限候选优化和可恢复地图 Artifact |
| 6 | [审批与故障恢复](tutorials/approvals-and-recovery.md) | 能区分等待、审批、失败、恢复与错误兜底 |

印尼案例的统一背景、来源和计算口径见
[案例总览](tutorials/supply-chain-agent-tutorial.md)。

## 主路径坚持的边界

- Dataset、capability package、Agent 和 Supervisor 都使用不可变 Release；
- Agent 能力来自 reviewed capability template，不来自 Prompt；
- Dataset 权限来自精确 Release 绑定，不来自文件路径；
- 确定性 Tool 负责校验和计算，模型不重算 240,000 行数据；
- Agent 之间传递有类型、有身份的 Artifact 引用，不复制原始数据；
- Supervisor 根据证据缺口动态选 Agent，不把一次轨迹固化成 Workflow；
- 页面恢复、失败状态和审批与最终答案一样需要验证；
- 教程不泄露 Secret、本地路径、Resource URI 或 Runtime 内部 ID。

## 开发者补充练习

[Hello Agent：第一次真实 Tool 调用](tutorials/hello-agent-quickstart.md) 使用仓库内
`tools/hello-agent` Plugin 和终端 smoke，适合需要学习本地 Plugin 结构和 MCP stdio
验证的开发者。它不是 Web 新手主路径的前置条件。

开发新的领域能力前，再阅读：

- [领域 Agent 扩展架构](domain-agent-extension-architecture.md)
- [Supervisor、Agent、Skill 与 Tool](supervisor-agent-skill-tool-architecture.md)
- [Skills、MCP 与自定义 UI 扩展](custom-skills-mcp-ui-guide.md)
- [系统架构](architecture.md)
- [安全模型](security-model.md)
- [能力基线](capability-baseline.md)
