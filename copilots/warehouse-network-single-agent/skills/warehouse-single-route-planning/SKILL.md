---
name: warehouse-single-route-planning
description: 为精确仓网输入准备路线与成本矩阵；处理 provided、haversine 和经许可的导航路线。
metadata:
  short-description: 准备路线与成本矩阵
---

# 路线与成本矩阵

只消费 `ready` `prepared_network_input.v1` 的精确 Workspace 相对路径与 `input_identity`。每次 Network Tool 都传该路径，由 Tool 自己校验身份；不得扫描或重建 raw 数据。当前仓网使用 `existing_only`；明确包含候选仓的模拟或规划使用 `all_warehouses`。

优先使用输入中已有的 `provided` 路线事实。只有在用户确认绕路系数和平均速度后才使用 `haversine`。用户选择导航时，先调用 `create_navigation_matrix_request`，并传可用的 exact prior navigation matrix；它只生成缺失 lane 与计费估算。返回 `ready` 时复用 prior matrix；返回 `execution_required` 时取得用户的原生费用确认，调用 `map_utils.execute_navigation_matrix` 自动执行 request，再用 `import_navigation_matrix` 与 prior matrix 校验合并。不可达 lane、Provider 错误和取消必须原样报告；不得把地图 GeoJSON、模型文本或估算值伪装为导航矩阵，也不要求用户手工导入导航 JSON。

成本优先使用用户路线报价；没有完整报价和用户确认的计算规则时，明确标记成本不可用。返回 Tool 产生的精确矩阵 ResourceRef，不从 Workspace 或 Resource 列表重建数据。
