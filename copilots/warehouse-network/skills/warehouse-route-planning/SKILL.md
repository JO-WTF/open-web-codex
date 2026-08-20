---
name: warehouse-route-planning
description: 仅供 network_agent 使用。为精确仓网输入准备 provided、haversine 或经许可的导航路线与成本矩阵。
metadata:
  short-description: 准备路线与成本矩阵
---

# 路线与成本矩阵

只消费 Supervisor 传入的 `ready` `prepared_network_input.v1` 精确 Workspace 相对路径与 `input_identity`；不得发现、读取 raw 数据或重建它。每次 Network Tool 都传该路径，由 Tool 自己校验身份。当前仓网使用 `existing_only`，明确包含候选仓的模拟或规划使用 `all_warehouses`。同一任务先算基线、再评估一个或多个候选仓时，第一次就用 `all_warehouses` 生成唯一一份路线矩阵，基线与后续候选场景都复用该精确引用；不得先为基线生成 `existing_only`，再为候选仓重复生成第二份矩阵。

优先使用 `provided` 路线事实。只有用户确认绕路系数和平均速度后才使用 `haversine`。导航时先调用 `create_navigation_matrix_request`，将 create-new 请求写入 `outputs/warehouse-network/requests/*.json`，并传可用的 exact prior navigation matrix；不得写入 Workspace 根目录或源数据目录。若返回 `ready`，复用 prior matrix；若返回 `execution_required`，向用户展示调用量并取得原生费用确认后，调用 `map_utils.execute_navigation_matrix` 自动执行该 request，再用 `import_navigation_matrix` 与 prior matrix 校验合并。不可达 lane、Provider 错误和取消必须原样报告；不得将 GeoJSON、模型文本或估算值冒充导航矩阵，也不要求用户手工导入导航 JSON。

成本优先使用用户报价。用户要求用现有报价均值外推缺失 lane 时，使用 `plan_cost_matrix` 的 `cost_policy.kind=observed_quote_mean`，由 Planner 对完整 prepared input 中当前所需层的全部标准化报价计算 `arithmetic_mean(price_per_vehicle / vehicle_capacity)` 并返回分层报价数、总报价数、币种、公式、prepared identity 和均值证据；Network Agent 不从 preview、Workspace 文件或脚本自行计算。当前多 Agent package 的 Root、Data、Network 都没有 shell；用户明确要求“必须由 Agent 写并运行脚本”时，如实返回该能力只在单 Agent package 可用，不得假装生成脚本。若用户提供已授权 Workspace 中现成的 `warehouse_quote_mean_calculation.v1` 文件，可传 `quote_mean_evidence_relative_path`，由 Planner 重新读取完整报价并逐字段校验证据后绑定到 Cost Matrix；不能把脚本 JSON 与 Tool 数值当成两份事实。只有用户既未提供显式规则、也未授权从报价派生且报价不完整时才明确成本不可用。仅返回 Tool 产生的精确 ResourceRef。

成本结果必须标明这是“现有报价均值外推”的敏感性方案；缺少仓租、建设、容量或吞吐成本时，不得称为经济意义上的全局最优。
