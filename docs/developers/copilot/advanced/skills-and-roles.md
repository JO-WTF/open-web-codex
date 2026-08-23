# 进阶：Skills 与 Roles

> **适合谁**：最小单 Agent 已通过，准备拆分专业角色或收紧能力的开发者
> **预计时间**：15 分钟
> **前置条件**：理解 [Copilot 核心概念](../concepts.md)
> **完成结果**：每个 Role 只有必要 Skill/Tool，Root 与 child 职责不重叠

## Skill 写方法

Skill 应描述业务目标、需要的信息、可自行判断与必须询问的边界、失败终态、回答质量。不要写
Transport、安装路径、固定重试、缓存协议或第二套协作流程。Tool 已可用时直接使用；只有当前
确实不可用时才做原生 Tool Search。

## Role 写分工与权限

Role 的 `developer_instructions` 保持短而硬：职责、不做什么、失败如何停止。Skill 开关放
`[[skills.config]]`；MCP policy 放 `plugins.<tool>.mcp_servers.<server>`。Role 不写 server command，
也不能启用 manifest 未声明的 Skill/Tool。

## 拆成多 Agent 的判断

只有当工作可形成稳定专业边界、权限确实不同、child 有清楚终态时才拆。Root 负责用户目标、必要
选择和最终解释；child 只执行有界任务。Root/child 使用 Codex 原生 context、mailbox、wait 和
follow-up，不在 Platform 新建消息或结果 API。

仓网是当前参考：[4 Skills / 3 Agents 的精确分工](../warehouse-copilot.md#3-再建立独立的多-agent-包)。

## 成功信号与失败跳转

成功信号：`validate` 能证明所有引用一致，`test` 能从 canonical history 证明目标 Role 调用了其
被授权 Tool，Root 不越权。若引用失败，见
[Role 找不到 Skill、Tool 或 server](../testing-and-troubleshooting.md#role-找不到-skilltool-或-server)。
