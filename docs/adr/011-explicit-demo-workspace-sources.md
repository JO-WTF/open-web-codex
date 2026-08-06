# ADR-011：显式 Demo Workspace 原始源与 Tutorial Blueprint 分离

状态：已接受（2026-08-03）

## 决定

普通 Enterprise Supervisor 只发现当前授权 Workspace 的真实 `.xlsx`、`.csv`、
`.json`。空 Workspace 是零数据源；缺少可信 `codex/sandbox-state-meta.sandboxCwd`
或发现失败必须显式终止，不得读取 Plugin 示例目录、历史 Profile Resource 或任何
备用目录。

当且仅当用户明确要求创建 Demo、样例或合成数据时，Root Supervisor 才把任务委派给
Data Agent。`supply_chain_demo` MCP 只从 Runtime metadata 取得目标根，在空 Workspace
中原子写入版本化、确定性的普通源文件；它不接受路径参数，不覆盖文件，也不创建
Dataset Release、确认参数或启动分析。生成后继续使用与真实数据相同的 discovery、
profile、mapping、确认、normalize、Release 和 Binding 链路，并贯穿
`synthetic_demo`、模板版本和 digest provenance。

Demo 源的事实粒度与通用合同一致：需求点引用城市；距离与时效按出发城市到目的城市
建模；报价按出发区域到目的城市建模。大规模模板可以生成 120,000 个需求点，但不会
把路线或运价复制到需求点粒度。

Tutorial Blueprint 是不同的所有者和生命周期：平台安装一个锁定的 Dataset Release、
Agent Release 与 Supervisor Release，只在用户主动从 Learn 安装后可用。Blueprint
资产不进入普通 Workspace 发现，Demo Tool 也不安装 Blueprint。

## 被否决的方案

- 在普通 Prompt、Skill 或数据 MCP 内保留示例目录并在发现失败时回退；这会把模型
  上下文中的样例误报为 Workspace 事实。
- 创建 Supervisor 时用一个布尔值静默注入示例数据；这会绕过 Runtime Skill/Tool
  边界，也无法保留清晰的用户授权和 provenance。
- 让 Demo Tool 自动发布 Release 或启动分析；这会跳过确认和平台持久化所有者。

## 后果与验证

Demo 写入 Tool 是显式、有副作用且需审批的独立 MCP Server。普通 Data Agent 只能看见
声明的 Workspace 读取 Tool 和该单一写入 Tool，其他 Agent 不得启用它。验证必须覆盖
空/非空 Workspace、缺少 metadata、幂等复用、部分文件、digest 漂移、符号链接、并发、
跨 Workspace 拒绝，以及 Tutorial 独立安装。真实浏览器、Runtime、重启和 Release/
Binding 旅程通过前，能力基线仍标记为未验证。
