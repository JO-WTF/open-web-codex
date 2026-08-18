# ADR-021：以 MCP Server 为边界的 Platform Tool 渐进披露

状态：Accepted（2026-08-18）；局部替代 [ADR-018](018-built-in-network-copilot-runtime-closure.md) 中 Role 按 Tool 名称限制可见面的部分。

关联：[ADR-018](018-built-in-network-copilot-runtime-closure.md)、[ADR-019](019-task-selected-copilot-packages-and-shared-tools.md)。当前实现与验证状态仍以[系统架构](../architecture.md)和[能力基线](../capability-baseline.md)为准。

## 背景

Role 的 `enabled_tools` 名称白名单会随着一个已授权 Platform MCP server 新增 Tool 而过期，也让 Copilot 在没有改动 Role 的情况下无法使用同一 server 的新能力。把这些 schema 全部放入首轮上下文又会扩大上下文，并绕过 Runtime 已有的 ToolSearch 生命周期。

## 决定

1. Copilot 仍只能使用其 Task 选中的包、prepared Capability Root 和 Role 明确启用的 MCP server；不扫描或加载无关 Copilot、Workspace 或 Profile Tool。
2. 已启用 MCP server 内的全部已声明 Tool 对该 Role 开放，不再使用 `enabled_tools` 名称白名单。Capability Root 与 server scope 仍是强制权限边界。
3. Role 对每个已启用 server 设置 `omit_tools_from = ["direct"]`。Runtime 负责将其注册为 deferred Tool，`tool_search` 只搜索已授权 metadata，并只把命中 Tool 的 schema 加入下一轮上下文。
4. Server 默认审批与逐 Tool approval policy 不变；发现不等于预批准，也不改变 Workspace、Secret、Artifact 或 Resource 授权。
5. Provider 是否支持 ToolSearch 继续由 Runtime typed capability 决定。Platform、Browser 和 Skill 不模拟搜索、不能在不支持时以完整 direct schema 作为替代。

## 后果与验证

新增同一已授权 server 的 Tool 无需改动每个 Copilot Role 才能被发现；新增 server 或 Capability Root 仍需要显式 package/Role 合同和 prepared transport。验证覆盖所有 checked-in Role 不含 `enabled_tools`、每个启用 server 省略 `direct` surface、逐 Tool approval 保持，及 Runtime 的 deferred ToolSearch 回归。
