# Copilot 核心概念

> **适合谁**：已经跑过首个示例，想知道每类文件为什么存在的开发者
> **预计时间**：10 分钟
> **前置条件**：了解仓库中的 `copilots/` 与 `tools/` 目录
> **完成结果**：能把新需求交给正确 owner，而不是把同一逻辑散落在多层

## 一句话模型

```text
用户目标
  → Root Agent（当前任务的负责者）
  → Skill（工作方法）
  → Role（权限与分工）
  → Tool（确定性执行）
  → MCP Resource / Workspace 文件（中间数据）
  → delivery（明确的最终交付）
```

`copilot.toml` 是这条链的装配清单。它声明一个 Root，列出包内 Skill/Role、引用本地或共享
Tool、定义原生测试，并精确标记哪些 Tool 结果是最终交付。

## 谁拥有哪个事实

| 对象 | 通俗解释 | 拥有 | 不拥有 |
| --- | --- | --- | --- |
| Agent | 当前真正执行和对话的人 | 理解目标、选择已授权能力、解释结果 | 算法真相、平台授权 |
| Skill | 工作方法 | 业务顺序、何时询问、交付质量 | Tool 实现、Runtime 启动、权限扩大 |
| Role | 权限分工 | Skill 开关、MCP server/tool policy、Agent 边界 | 业务算法、第二套 Agent 调度 |
| Tool | 确定性执行 | 输入校验、算法、原子写入、typed 成功/失败 | Task 身份、浏览器展示、隐藏兜底 |
| `copilot.toml` | 装配清单 | Root、组件引用、测试、delivery producer | 安装脚本、Secret、模型输出 |
| Platform | 通用工作台 | Workspace 授权、Task、Profile 进程、审批、最终交付投影 | 领域字段、Agent 推理、MCP Resource 内容 |
| Codex Runtime | 执行内核 | Thread/Turn、上下文、Agent 协作、Skill/Plugin/MCP 执行 | Web 会话、领域数据库、Workspace 授权 |

## 单 Agent 与多 Agent

单 Agent 包有一个 Root Agent，直接持有完成任务所需的 Role policy。多 Agent 包也只有一个
Root，但 Root 通过 Codex 原生协作把有边界的工作交给 child Role。

它们是两个独立 Copilot 包，不是一个包里的两个 mode。Task 创建时只选择一个包，生命周期中
不会按提示词自动切换。

## 本地 Tool 与共享 Tool

第一次练习可以把 Tool 放在 Copilot 内：

```toml
[[tools]]
id = "record_review_tools"
root = "tools/record-review-tools"
runtime = "tools/record-review-tools/runtime.toml"
```

两个或更多 Copilot 复用同一能力时，把 Tool 放到根级 `tools/<package>/`，提供严格 `tool.toml`，
再通过唯一注册表引用：

```toml
[[tools]]
id = "supply_chain"
package = "warehouse-network-planner"
```

共享只表示实现复用，不会建立 Copilot 间的消息、上下文或结果通道。

## 文件、Resource 与交付

- **Workspace 文件**：用户可见、可下载、跨工具显式交接的普通文件；必须使用相对路径和
  create-new 写入，不能覆盖原始输入。
- **MCP Resource**：由 provider 保存的精确 typed 中间快照；通过 `{server, uri, schema}` 引用，
  不是 Platform 数据库记录。
- **交付（delivery）**：明确给用户展示或保留的最终报告/地图；producer 必须在 manifest 中声明。

Resource 不会因为出现在模型文字里就变成交付，Artifact 也不用于 Agent 间传数据。

## 生命周期

开发阶段从源码开始：

```text
init → 修改 → validate → check → sync → Web 真实验收
```

`validate` 只检查静态装配；`check` 进一步准备 Tool、启动真实 app-server 做发现，并用本地确定性
Provider 运行原生正常链；`sync` 通过同一门后才重启本地服务。Web 真实任务仍是最终验收。

## 成功信号与失败跳转

成功信号：面对一个新规则，你能唯一回答“它属于 Skill、Role、Tool、manifest、Platform 还是
Runtime”。如果答案同时落在三层，先回到上面的 owner 表；仍不清楚时看
[按阶段排错](testing-and-troubleshooting.md#按阶段排错)，不要先写兼容或兜底。
