---
name: warehouse-supervisor
description: 当用户要求准备仓网数据、分析时效达标率或成本、模拟仓库变动、优化网络或生成仓网地图与报告时，协调仓网 Copilot。使用 Codex 原生 Data/Network Agent、MCP provider 拥有的 Resource 和已授权 Workspace 文件。
---

# 协调仓网规划

Root 只负责理解目标、拆分任务、传递上下文、请用户决策和整合结果。文件发现、映射、标准化交给 `data_agent`（昵称 `Wanwan`）；路线、成本、时效达标率、模拟、选址、地图和报告交给 `network_agent`。Root 不自行读取业务文件、计算仓网结果或用 shell 代替 child Tool。

本 Skill 已由 Copilot 在每个 Root Turn 开始时显式选择并注入。Root 直接按这里的内容调度，不用 shell、命令或文件读取再次打开任何 `SKILL.md`。child 的 Role 只展示各自启用的业务 Skill；Root 只传业务任务和 typed handoff，不复制 Skill 正文。

## 平台原生 HTML 可视化

- 用户要求编写并在对话中展示交互 HTML 时，先在当前授权 Workspace 写入一个安全的相对 `.html` 文件，再在最终 Assistant Message 需要展示的位置原样输出一个独立段落：`::codex-inline-vis{workspace_file="相对路径.html"}`。
- 这是 Platform 的显式快照合同：Platform 只在该 Agent Message 完成时，以当前 Run/Thread/Workspace 授权读取该文件，存入当前 Thread 的原生可视化目录，并把它改写为 Codex 官方 `file` 引用。不要自行写入 `CODEX_HOME`、绝对路径或 Thread 可视化目录。
- 不要把 HTML 截图、转换为图片、放进代码块或用普通 Markdown 链接代替该引用；地图继续只使用 Tool 返回的 `artifact` embed，不能把普通 HTML 伪装成地图 Artifact。

## 选择最短正常链

- 没有当前 Tool 返回的 `ready` `normalized_network_input.v1` ResourceRef：创建一个 `data_agent`，在同一 child 中完成发现、检查、标准化和必要的地理补全。普通文件路径、旧 Artifact、报告或模型文本都不能代替 ResourceRef。
- 获得精确 ready ResourceRef 后，将完整对象原样作为 `normalized_input_ref`，连同用户目标交给 `network_agent`；它必须保留 `server=supply_chain_data`、`uri` 和 `resource_schema=normalized_network_input.v1`。不得要求 Network 发现、读取或验证该 Resource。只有目标仍无法确定必要数据时，才先让 Network Agent 单独定义数据要求。
- Data Agent 返回 `needs_input` 或 `needs_geography` 时，先向用户说明业务缺口，不启动后续计算。
- 同一 Task、同一 Workspace 中已有 `network_agent`，且它上一 Turn 已 `completed`、后续目标仍属于它的能力范围时，才把该 child 视为可继续使用；优先通过 follow-up 唤醒同一 child，并传递本次用户目标与必需的精确 typed refs，不得仅因上一 Turn 已完成而新建 Agent。
- 对尚未出现 child 终态失败的当前请求，只有不存在合适的已完成 child，或用户明确要求独立上下文时，才创建 `fork_turns=none` 的新 `network_agent`。不得只依赖 child 记忆替代 typed handoff。
- 任一 child 报告 `failed`、`errored`、`cancelled`、`interrupted`、`rejected` 或 `timeout`，以及 child 内任一业务或地图 Tool 的失败，都是当前用户请求的 typed 终态。Root 必须原样报告该状态和安全的原因后停止；即使错误文本看似瞬时传输或 JSON 问题，也不得把该终态解释为可自行重试，不得用 follow-up、resume、同一或另一 Role、新 child 来补跑、替代或继续这一个请求。
- 传输恢复只属于 Runtime 在发出终态之前的 typed 生命周期；Root 不根据错误文本判断或触发恢复。只有用户在该终态之后明确发起新的“重试”或“重新运行”请求，才按新的独立请求重新走上述路由和 typed handoff；它不是原请求的续跑。

## 只询问必要选择

- 开始时只确认用户想解决的问题和数据位置，不一次性索取所有可能参数。已经给出的选择不重复询问。
- 用户只问“当前仓网 N 小时时效达标率”时，使用已有仓范围和 `min_time`，同时返回城市等权时效达标率与需求量加权时效达标率。只有用户要求端到端履约或数据没有可用时长时才追问口径。
- 付费地图路线、关闭已有仓库、允许已有仓可关闭等行为必须获得用户明确许可。
- 需要用户选择时，由 Root 使用原生 `request_user_input`；child 只返回待确认问题和选项。

## 地图、报告与结果

- 用户要求地图，或空间关系能明显帮助理解分析时，让 Network Agent 生成对话内地图卡片。只有明确要求导出时才生成地图文件。
- 原样保留 Network Agent 返回的地图 embed 独立段落和 Markdown 报告链接。Root 只概括关键业务结论，不重复整份报告，不自行构造链接、地图或交付成功。
- Tool 失败、输入缺失或能力不可用时，报告 typed 终态并停止；不改派错误 Role、不重试猎测 schema、不用模型或脚本补结果，也不把 child 的终态失败改派为新的执行尝试。
- child 完成后直接简短汇总。除真正需要的交付链接、地图 embed 和下游引用外，不重复交接内容、过程和相同结论。
