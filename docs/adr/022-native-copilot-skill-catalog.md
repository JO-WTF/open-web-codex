# ADR-022：Copilot 使用原生 Skill Catalog 与按需正文读取

状态：Accepted（2026-08-18）

关联：[ADR-019](019-task-selected-copilot-packages-and-shared-tools.md)、[ADR-021](021-deferred-platform-tool-discovery.md)。当前实现与验证状态仍以[系统架构](../architecture.md)和[能力基线](../capability-baseline.md)为准。

## 背景

Copilot Adapter 曾关闭 Runtime Skill catalog，并以服务器本地 `SKILL.md` 路径把 Root 完整正文作为每轮输入注入。于是任务 Skill 的 description 没有进入模型可见目录，正文也无法遵循原生的“先发现、再按需读”语义；SDK 的简化 frontmatter 解析还拒绝了原生支持的 `metadata.short-description`。

## 决定

1. 每个 Copilot 包保留恰好一个小型常驻 Root Skill，只包含身份、边界、协同和通用安全规则。Adapter 每个 Root Turn 只显式选择这一项，保证其正文持续可用。
2. 数据准备、路线、分析、选址、地图和交付等工作流拆为独立任务 Skill。`[root].task_skills` 是 package 显式合同：协调型多 Agent Root 取 `none`，只保留常驻 Root；直接执行的单 Agent Root 取 `all`，在原生 catalog 发现任务 Skill。child Role 的 Skill scope 仍由其原生 Role config 决定。Runtime 向模型提供 name、description、可选 short-description 和 locator；Host 文件型任务 Skill 仅在命中后按其精确 locator 打开正文及 references。
3. SDK frontmatter 校验接受与 Runtime 相同的 `metadata.short-description`，但不复制或再编码 Skill 元数据到 Platform DTO、数据库或 Browser。
4. Root/Role 的 Capability Root、MCP server、Tool search、审批、Workspace 和 Resource 边界不因 Skill 可发现性而扩大。Browser 仍只选择 package ID；它不能提交 Skill 路径或选择结果。

## 后果与验证

新增任务 Skill 会自动进入声明其 scope 的 Runtime catalog，不需要新增 Adapter 路径注入。SDK 验证覆盖 `short-description` frontmatter 与 Root task-Skill scope；Adapter 覆盖唯一常驻 Root、Root scope 与 Role scope；真实 Runtime 门验证 Root 正文常驻、任务摘要只在允许的 Root/Role catalog 出现且未选中任务正文不进入请求。
