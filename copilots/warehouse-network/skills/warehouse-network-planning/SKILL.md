---
name: warehouse-network-planning
description: 仅供 network_agent 使用。准备路线与成本，并完成仓网基线、仓库变动和选址规划。
metadata:
  short-description: 路线成本与仓网规划
---

# 仓网规划

只处理 Supervisor 传入的业务输入和 Tool 返回的精确引用。一次请求先判断用户要的是基线、单仓变动，还是多仓选址；不要为同一目标重复计算相同矩阵。

路线与成本：

- 优先使用输入中已确认的 `provided` 路线事实。
- 只有用户确认绕路系数和平均速度后才用 haversine。
- 需要导航时先调用 `create_navigation_matrix_request`，展示调用量和费用风险；取得原生许可后执行并导入路线结果。
- 基线的路线与成本 `warehouse_scope` 必须是 `{kind:"existing_only"}`；单一设施变化使用 `{kind:"existing_plus_candidates",candidate_ids:[...]}`，候选 ID 必须覆盖 before 中仍启用候选与本次新增候选；全量 p-median 才使用 `{kind:"all_warehouses"}`。不同 scope 不复用同一矩阵。
- 成本优先使用报价。用户授权按完整报价均值外推时，调用 `plan_cost_matrix` 的 `cost_policy.kind=observed_quote_mean`；Planner 返回完整报价总数、分层数量、币种、公式、均值和证据。Network Agent 不从 preview 或脚本自行计算。
- 多 Agent 没有 shell/脚本能力；不要假装生成脚本。已有授权的 `warehouse_quote_mean_calculation.v1` 证据必须交给 Planner 校验，不能再改写成另一份 explicit 规则。
- 报价均值外推是敏感性方案；缺少仓租、建设、容量或吞吐成本时，不称为经济意义上的全局最优。

分析与规划：

- 当前网络用 `evaluate_network_baseline`；省略 `coverage_mode` 或使用 `auto`，由 Tool 判断真实当前分配还是 `optimized_existing_footprint`。没有 current assignments 时不得称为真实现状。
- 新增、关闭或搬迁一个仓库用 `assess_facility_change`。关闭已有仓必须先得到用户许可；比较只引用该 Tool 同时返回的成本、时效、活动仓、受影响城市和重分配城市。
- 用户给出新增仓数时，用 `opening_policy={kind:"exact",number_to_open:n}`；只要求达到时效/需求加权覆盖率而未给仓数时，用一次 `opening_policy={kind:"minimum_feasible",maximum_number_to_open:n}`，把约束放入 `service_constraints`。Planner 在一个总时间预算内从 0 开始搜索，返回每个仓数的可行性、首个可行仓数、coverage 和最优性；不要循环调用多个 p-median。
- 默认保留所有已有仓。只有用户明确许可并给出可关闭的已有仓 ID 时才允许 closure policy。

Tool 成功结果和精确 ResourceRef 是后续工作的唯一输入。`infeasible` 是 minimum-feasible 搜索中的业务结果；权限、身份不一致、取消、超时、外部失败、能力不可用或输入无效是当前请求的 typed 终态，应如实停止并报告。
