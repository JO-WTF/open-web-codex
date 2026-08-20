---
name: warehouse-single-route-planning
description: 为精确仓网输入准备路线与成本矩阵；处理 provided、haversine 和经许可的导航路线。
metadata:
  short-description: 准备路线与成本矩阵
---

# 路线与成本矩阵

只消费 `ready` `prepared_network_input.v1` 的精确 Workspace 相对路径与 `input_identity`。每次 Network Tool 都传该路径，由 Tool 自己校验身份；不得扫描或重建 raw 数据。当前仓网使用 `existing_only`；明确包含候选仓的模拟或规划使用 `all_warehouses`。同一任务先算基线、再评估一个或多个候选仓时，第一次就用 `all_warehouses` 生成唯一一份路线矩阵，基线与后续候选场景都复用该精确引用；不得先为基线生成 `existing_only`，再为候选仓重复生成第二份矩阵。

优先使用输入中已有的 `provided` 路线事实。只有在用户确认绕路系数和平均速度后才使用 `haversine`。用户选择导航时，先调用 `create_navigation_matrix_request`，将 create-new 请求写入 `outputs/warehouse-network/requests/*.json`，并传可用的 exact prior navigation matrix；不得写入 Workspace 根目录或源数据目录。返回 `ready` 时复用 prior matrix；返回 `execution_required` 时取得用户的原生费用确认，调用 `map_utils.execute_navigation_matrix` 自动执行 request，再用 `import_navigation_matrix` 与 prior matrix 校验合并。不可达 lane、Provider 错误和取消必须原样报告；不得把地图 GeoJSON、模型文本或估算值伪装为导航矩阵，也不要求用户手工导入导航 JSON。

成本优先使用用户路线报价。用户要求用现有报价均值外推缺失 lane 时，不要求用户手填均值，也不从 preview 计算：`plan_cost_matrix` 使用 `cost_policy.kind=observed_quote_mean`，由 Planner 对完整 prepared input 中当前所需层的全部标准化报价计算 `arithmetic_mean(price_per_vehicle / vehicle_capacity)`，并返回分层报价数、总报价数、币种、公式、prepared identity 和均值证据。用户明确要求脚本计算时，用 shell/Python 在 `outputs/warehouse-network/calculations/` create-new 一个 `.py` 脚本，再用 shell 执行；脚本只读取精确 `prepared_input_relative_path`，把符合 `warehouse_quote_mean_calculation.v1` 的输入 identity、完整报价总数、分层报价数、币种、公式和均值写入同目录 create-new `.json`，不得把完整报价行输出到终端或模型上下文。随后以 `cost_policy={kind:"observed_quote_mean"}` 和 `quote_mean_evidence_relative_path` 调用 Planner；Planner 会从完整 prepared input 重算并逐字段校验证据，再把同一证据绑定到 Cost Matrix。不要将均值改写成另一份 explicit 规则。只有用户既未提供显式规则、也未授权从报价派生且报价不完整时，才标记成本不可用。返回 Tool 产生的精确矩阵 ResourceRef，不从 Resource 列表、模型文本或预览样本重建数据。

脚本 evidence 的 JSON 顶层字段必须精确为：`schema_version="warehouse_quote_mean_calculation.v1"`、`prepared_input_relative_path`、`input_identity={schema_version:"prepared_network_input.v1",content_sha256}`、`total_quote_count`、`method="observed_quote_mean"`、`tool_version="observed-quote-mean.v1"`、`formula="arithmetic_mean(price_per_vehicle / vehicle_capacity)"`、`considered_quote_count`、`ignored_quote_count`、`rules`。`rules` 每层恰好一项，字段为 `layer`、`currency`、`quote_count`、`mean_cost_per_demand_unit`；不得增加别名或额外字段。
