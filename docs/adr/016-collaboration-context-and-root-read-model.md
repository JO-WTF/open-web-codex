# ADR-016：不可伪造 CollaborationContext 与 Root 只读协调

状态：已被 [ADR-018](018-built-in-network-copilot-runtime-closure.md) 替代（2026-08-08）；
下文仅保存历史决定。当前阶段直接复用 Runtime wait/mailbox/steer、child elicitation 和
终态，不建设 Assignment Grant、Root coordination MCP 或 Run Completion Controller。

## 背景

Root 目前主要通过子 Agent 自述获取进度，assignment 又主要依赖自然语言。把完整状态
塞进 Prompt 会增加上下文和不可信转述；让 Platform 调度 Agent 则会形成第二套 Runtime。

## 决定

1. Platform/Profile Host 根据授权 Run 生成不可由浏览器或模型构造的
   `CollaborationContext`，固定组织、Profile、Workspace、Task、Run、Supervisor、
   Installation、Work State、Dataset grants 和预算。
2. Supervisor 委派时由 `AssignmentCompiler` 产生目标、可读 components、能力交集、
   交付件、完成条件和摘要预算；自然语言不作为权限恢复来源。
3. Platform 提供 Task/Run scoped Root Coordination Tool，只读查询 execution、Approval、
   Work State blocker 和 Artifact deliverable 的权威投影。
4. Coordination Tool 不能 spawn、follow-up、interrupt、审批、修改 Work State 或宣称成功。
5. Codex Runtime 继续拥有 spawn/message/follow-up/wait/interrupt 和 Thread context。

## 否决方案

- Root 轮询子 Agent 的自然语言状态；
- Platform 新建 Agent Scheduler；
- 把数据库投影注入为第二份模型上下文；
- 根据 Agent 显示名决定权限。

## 后果与验证

需要 context builder、assignment compiler、只读 query service 和越权测试。刷新、重启、
乱序、子 Agent 输入和 execution 终态必须从同一投影恢复，且普通 Thread 不能访问受治理
任务的协调状态。
