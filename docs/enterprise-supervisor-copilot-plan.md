# Enterprise Supervisor Copilot 冻结迁移输入

## 文档定位

本文已于 2026-08-08 冻结，只保存仓网 6.0 原型的角色、算法和迁移输入，不再是当前
实现合同，不得从本文继续添加 Prompt、aliases、Case、continuation 或平台领域分支。
当前实现事实与最新 7/7 最小真实 E2E 以 `docs/capability-baseline.md` 为准；接受边界以
`docs/adr/018-built-in-network-copilot-runtime-closure.md` 为准；执行顺序只以
`docs/development-plan.md` 为准。

本文后续只允许修正会误导迁移的安全/算法事实。阶段一仓网迁移完成后删除，由
能力基线和教程描述新主链。

以下内容描述冻结时的原型，不代表目标 Platform owner 或可继续兼容的 wire contract。

| 项目 | 当前值 |
| --- | --- |
| Supervisor | `enterprise-supervisor-copilot@6.0.0` |
| Data Agent | `enterprise-data-agent@6.0.0` |
| Network Agent | `enterprise-network-planning-agent@6.0.0` |
| MCP | `supply_chain_network`；Network 需要时使用 Maps MCP 的批量 `distance_matrix` |
| Root 调度 Owner | Codex Runtime |
| 数据、审批和浏览器投影 Owner | Platform Server |
| 原始 Thread/Turn/Item Owner | Codex Runtime |
| Codex 改动 | 仓网协作不修改 Runtime；仅在现有 Chat transport 保留补丁内修复第三方 Provider usage 解析 |

## 角色边界

### Data Agent

Data Agent 负责判断数据能不能用，以及如何转换：

- 读取同一个 Case 中由 Network Agent 写入的数据需求；
- 只检查授权 Workspace 中的 CSV、JSON、XLSX；
- 检查来源并提交显式字段映射；
- 处理行政区名称、坐标、边界和字段单位；
- 按问题需要标准化需求城市、已有仓、候选仓、current coverage 和报价；
- 把标准化输入和质量状态提交到 Case；
- 缺数据、歧义和业务参数通过官方 Runtime 输入请求交给用户。

它不计算覆盖、成本、时效或选址，不读取主机路径，不扫描目录来猜 Agent/Skill/MCP，不在空 Workspace 或工具失败时加载 Mock。

### Network Agent

Network Agent 负责定义问题和确定性分析：

- 从用户问题确定国家、分析范围、必要字段、可选字段和输出；
- 根据用户选择构建球面或批量导航路线矩阵；
- 构建报价或显式成本规则矩阵；
- 区分 `actual_current` 与 `optimized_existing_footprint`；
- 计算时效覆盖率、运输成本、增删搬迁场景和设施选址；
- 把分析结果提交到 Case，并按用户需要发布最终报告 Artifact；
- 不扫描 Workspace、不猜字段、不自建 MCP、不复制原始数据。

### Root Supervisor

Root Supervisor 只负责：

1. 识别国家和用户要回答的问题；
2. 让 Network Agent 定义本次最小数据需求；
3. 只有存在文件处理依赖时才启动 Data Agent；
4. 根据 Case readiness 和输入请求动态等待或继续；
5. 避免重复 spawn 相同任务；
6. 汇总业务结果、假设、缺口、方案标签和下一步。

Supervisor 不规定“先 Data 再 Network”的固定业务流程。Data 和 Network 的依赖由当前 Case 状态和问题决定；无依赖工作可以并行，依赖未满足时不得分析。

## 当前 Case 合同

```text
requirements -> sources -> mapping -> normalized_input
             -> route_matrix / cost_matrix
             -> baseline / scenario / facility_location
```

Root 创建 Case 后，所有 assignment 只传同一个 `case_id`。完整业务数据留在 Profile 范围的 Case SQLite 中，工具只返回有限状态和指标。内部组件 ID、hash、路径和明细不进入消息。只有最终 `network_planning_report.v1` 通过 ResourceLink 发布为平台 Artifact。

## 印尼教程数据

`tools/supply-chain-network-planner/examples/indonesia-network/base/` 是明确标记的教程 fixture：

- 50 个需求城市，需求量 `ceil(population / 1000)`；
- 5 个中心仓：Jakarta、Palembang、Medan、Surabaya、Makassar；
- 6 个 cross-docking 仓；
- 550 条末端报价和 30 条干线报价；
- 城市点位经过 geoBoundaries ADM2 边界检查；
- 报价和距离的 Spearman 相关系数不低于 0.85；
- `current-coverage-extension` 和 `candidate-extension` 单独提供，可按教程逐篇加入。

Fixture 不得自动回退。只有用户明确说 mock/demo/tutorial，Demo Server 才能复制它；真实用户文件优先走 Data Server。

## 用户输入合同

- 一次请求 1–3 个问题；每题最多 3 个预设选项，另可选自定义回答；
- 选项和自定义输入互斥；secret 使用 password 控件，不保存回答正文；
- child Agent 输入请求通过 root Run 的 pending input 队列显示；
- 回答使用 Approval UUID + version，过期版本返回 409 并要求重新读取；
- Runtime 投递未知时显示 `delivery_unknown`，不伪造成功。

## 执行可审查合同

每个 Agent execution 在 assignment/spawn 时产生稳定标题，标题只来自业务任务首句。前端主时间线显示一张 execution 卡片：状态、最新安全进度、等待轮数、结果摘要和终态；连续 wait 不生成重复消息。原始 Run events 仍完整持久化，可从审计接口读取。`completed`、`failed`、`interrupted` 一旦成立，乱序非终态事件不能覆盖它。

## Supervisor Draft 合同

Draft 只保存 `revision`、`content_sha256`、`updated_at` 和当前内容；用户不输入语义版本。保存需要 `expected_revision`，服务器递增 revision。发布时事务锁定同一 policy，第一次为 `1.0.0`，后续自动递增 patch。Release 不可变，Run 固定 Draft revision/hash 或 Release version/hash。

## 验收顺序

1. Python fixture、模型、地理、矩阵、成本和求解器单元测试通过。
2. 单一 MCP stdio inventory 与一次真实 Tool 调用通过；不能通过 shell `source` 调用。
3. Rust 迁移、Approval、输入队列、execution projection 和 Draft 并发测试通过。
4. Web typecheck、lint、Vitest、build 通过；输入队列、execution 卡片和 wait 聚合可恢复。
5. 真实 `/web` E2E 覆盖：显式教程数据、参数输入、无 current coverage、current extension、成本、场景、p-median、地图和刷新。
6. 最后核对 `codex/codex-rs` 差异只有 Patch Map 已登记的第三方 Chat usage 解析修复。

## 不在本次范围

- 不实现第二个 Agent 调度器；
- 不增加历史 `planning-dataset.v2` 双读或回退；
- 不通过名称启发式决定 Agent 权限；
- 不把原始客户表或完整矩阵加载到模型上下文；
- 不为每个客户逐条调用导航接口；
- 不让 Mock、旧求解器、零成本或静默默认值掩盖输入缺口。
