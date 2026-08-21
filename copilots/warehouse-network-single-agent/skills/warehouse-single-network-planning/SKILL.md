---
name: warehouse-single-network-planning
description: 为单 Agent 准备路线与成本，并完成仓网基线、仓库变动和选址规划。
metadata:
  short-description: 单 Agent 路线成本与规划
---

# 单 Agent 仓网规划

按用户目标选择基线、单仓变动或多仓选址；复用同一份 Tool 结果，不重复计算同一矩阵。数据准备遵循 `discover_workspace_sources → inspect_workspace_sources`（inline profile + inspection identity）→ `prepare_network_input`；Data preparation Skill 交接的完整 prepared input 是本 Skill 的唯一业务输入，不接收 source profile Resource。

路线与成本：

- 优先使用 `provided` 路线事实。只有用户确认绕路系数和平均速度后才用 haversine。
- 导航先生成 request，展示调用量和费用风险，取得原生许可后执行并导入。
- 基线的路线与成本 `warehouse_scope` 必须是 `{kind:"existing_only"}`；单一设施变化使用 `{kind:"existing_plus_candidates",candidate_ids:[...]}`，候选 ID 必须覆盖 before 中仍启用候选与本次新增候选；全量 p-median 才使用 `{kind:"all_warehouses"}`。不同 scope 不复用同一矩阵。
- 成本优先使用报价。按完整报价均值外推时调用 `plan_cost_matrix` 的 `observed_quote_mean`；Planner 负责完整数据聚合和证据校验，不能从 preview 推算，也不能把均值改写成另一份 explicit 事实。
- 只有用户明确要求“用脚本计算”时，才可对 exact prepared input 执行有界 Python/shell 统计；脚本不是标准化数据 owner，也不能替代路线或求解 Tool。此时必须在 `outputs/warehouse-network/calculations/` 下 create-new 一个 `.py` 和一个 `.json`，JSON 与脚本结果必须共同满足下面的唯一 evidence 合同；不要把合同复制到 Root Role 或其他 Skill。

脚本 evidence 合同（`warehouse_quote_mean_calculation.v1`）：

- JSON 顶层只允许以下 exact 字段，不得 alias、缺字段或增加字段：`schema_version`、`prepared_input_relative_path`、`input_identity`、`total_quote_count`、`method`、`tool_version`、`formula`、`considered_quote_count`、`ignored_quote_count`、`rules`。
- 常量必须逐字为：`schema_version = "warehouse_quote_mean_calculation.v1"`、`method = "observed_quote_mean"`、`tool_version = "observed-quote-mean.v1"`、`formula = "arithmetic_mean(price_per_vehicle / vehicle_capacity)"`。
- `prepared_input_relative_path` 必须是本次 exact prepared input 的 Workspace 相对路径；`input_identity` 顶层只允许 `schema_version` 与 `content_sha256`，其值必须分别为 `"prepared_network_input.v2"` 与 64 位小写 SHA-256。不得使用 preview 身份或脚本自身身份。
- `total_quote_count` 是完整报价总数；`considered_quote_count` 与 `ignored_quote_count` 必须是完整输入上的整数计数并彼此一致，不能用 preview 行数代替。
- `rules` 中每个所需报价层必须且只能有一项；每项只允许 `layer`、`currency`、`quote_count`、`mean_cost_per_demand_unit` 四个字段。`quote_count` 为整数，`currency` 为实际币种，均值是该层 `price_per_vehicle / vehicle_capacity` 的算术均值。禁止 `fixed_cost`、`cost_per_km` 或其他别名/额外成本字段。
- 脚本执行后，先读取并检查 JSON，再把它的 Workspace 相对路径原样作为 `quote_mean_evidence_relative_path`，与 `cost_policy.kind = observed_quote_mean` 一起传给 `plan_cost_matrix`。Planner 必须从完整 prepared input 重算、逐字段绑定该 evidence，并返回绑定后的成本矩阵；Planner 重算值是唯一数值真相，脚本 JSON 不是第二份成本事实。
- 报价均值外推是敏感性方案；缺少仓租、建设、容量或吞吐成本时，不称为经济意义上的全局最优。

分析与规划：

- 当前网络用 `evaluate_network_baseline` 的 `auto` 模式；没有 current assignments 时只能称为 `optimized_existing_footprint`。
- 新增、关闭或搬迁一个仓库用 `assess_facility_change`；关闭已有仓必须先得到用户许可。
- 给出新增仓数时用 `opening_policy.kind=exact`；只要求达到时效/需求加权覆盖率时，用一次 `opening_policy.kind=minimum_feasible` 和 `service_constraints`，让 Planner 在总时间预算内先最小化新增仓数、再固定仓数最小化成本，不循环调用多个 p-median。
- 默认保留所有已有仓；只有用户明确许可并给出可关闭的已有仓 ID 时才允许 closure policy。

Tool 返回的 structured result 和精确 ResourceRef 是后续工作的唯一输入。`infeasible` 只表示搜索中的业务结果；权限、身份不一致、取消、超时、外部失败、能力不可用或输入无效是 typed 终态，应停止并如实报告。
