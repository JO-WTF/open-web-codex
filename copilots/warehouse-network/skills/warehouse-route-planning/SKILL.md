---
name: warehouse-route-planning
description: 仅供 network_agent 使用。为精确仓网输入准备 provided、haversine 或经许可的导航路线与成本矩阵。
metadata:
  short-description: 准备路线与成本矩阵
---

# 路线与成本矩阵

只消费 Supervisor 传入的 `ready` `prepared_network_input.v1` 精确 Workspace 相对路径与 `input_identity`；不得发现、读取 raw 数据或重建它。每次 Network Tool 都传该路径，由 Tool 自己校验身份。当前仓网使用 `existing_only`，明确包含候选仓的模拟或规划使用 `all_warehouses`。

优先使用 `provided` 路线事实。只有用户确认绕路系数和平均速度后才使用 `haversine`。导航时先调用 `create_navigation_matrix_request`，并传可用的 exact prior navigation matrix；它只把缺失 lane 和计费估算写入 Workspace。若返回 `ready`，复用 prior matrix；若返回 `execution_required`，向用户展示调用量并取得原生费用确认后，调用 `map_utils.execute_navigation_matrix` 自动执行该 request，再用 `import_navigation_matrix` 与 prior matrix 校验合并。不可达 lane、Provider 错误和取消必须原样报告；不得将 GeoJSON、模型文本或估算值冒充导航矩阵，也不要求用户手工导入导航 JSON。

成本优先使用用户报价。没有完整报价和用户确认的计算规则时明确成本不可用，不虚构币种或费率。仅返回 Tool 产生的精确 ResourceRef。
