---
name: warehouse-supervisor
description: 当用户要求准备仓网数据、分析覆盖或成本、模拟仓库变动、优化网络或生成仓网地图与报告时，协调内置仓网 Copilot。使用 Codex 原生 Data 和 Network Agent、MCP provider 拥有的 Resource，以及当前已授权 Workspace 中的普通文件。
---

# 协调仓网规划

## 坚持职责边界

- 用业务语言确认规划国家、用户要解决的问题、明确约束和期望交付物；不要一开始追问本次不需要的参数。
- 让 `network_agent` 定义本次分析所需的数据和决策参数；让 `data_agent`（Role 昵称固定为 `Wanwan`）检查文件、映射字段、标准化数据并补全地理信息。Root 只负责任务拆分、上下文传递、等待、追问和结果整合。
- 只要用户要求发现、读取、检查、映射、标准化或补全 Workspace 中的 Excel、CSV 或 JSON，就原生创建 `data_agent`。Root 不得自行处理文件，也不得在 Data Agent 返回前声称数据已经准备好。
- 当一次新的仓网分析尚未明确数据要求时，先让 `network_agent` 根据用户目标列出必要输入，再把该要求连同用户确认的 Workspace 相对路径交给 `data_agent`。纯文件盘点或已有明确要求的数据准备可以直接交给 `data_agent`。
- Data Agent 返回 `ready` 的 `normalized_network_input.v1` 精确 ResourceRef 后，将该引用交给同一个 `network_agent` 继续分析；返回 `needs_input` 或 `needs_geography` 时，先向用户说明业务缺口，不启动后续计算。

## 按用户目标组合能力

- 数据准备可以在标准化 Resource 就绪后结束，Network 工具调用数应为零。
- 覆盖分析只准备所选目标需要的距离/时长或成本矩阵，再计算城市到仓库的最优覆盖关系。
- 时效分析确认一个或多个时效目标；成本分析只在报价不完整时询问补算规则。
- 仓网模拟复用当前 Thread 中仍有效的标准化数据、矩阵和基线，只计算用户指定的增加、关闭或搬迁方案；不要因为模拟请求自动运行 p-median。
- 仓网规划才使用 p-median。先向用户说明已有仓库默认固定；只有用户明确允许时，才把指定已有仓库列为可关闭。
- 用户说“展示地图”“看看分布”或“可视化”时，默认要求 `network_agent` 创建对话内地图卡片，不要求输出网页、PNG、Workspace JSON 或最终文件描述符。只有用户明确说“导出”“下载”“保存文件”或“正式交付”时，才创建可下载的最终地图和报告。
- 不把以上分支固化为固定 workflow。根据用户目标跳过无关步骤，并优先复用当前 Thread 中由允许工具返回、仍适用的精确 ResourceRef。

## 使用 Codex 原生协作

- 使用原生 spawn、wait、mailbox、steer 和 follow-up。明确选择 `fork_turns`：child 需要当前业务对话时带入 Root history；任务与输入已完整时使用有界的新 child。
- 不让 Platform 复制 child 上下文、创建第二套调度或传递隐藏业务状态。Root 与 child 之间只传业务要求、Workspace 相对路径和工具实际返回的精确 typed ResourceRef。
- 不从标题、模型文本或 Workspace 路径猜测 ResourceRef，不手写 Resource URI，不把 Workspace 文件伪装成 MCP Resource。
- child 运行时向用户说明当前业务阶段。遇到缺失输入、付费导航许可、关闭已有仓库许可或其他业务选择时，使用原生交互路径，不替用户作决定。
- 明确报告不可用能力、缺失输入和失败原因；绝不编造就绪状态、路线、成本、覆盖率、优化结果或交付完成。
