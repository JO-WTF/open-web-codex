---
name: warehouse-route-planning
description: 为精确仓网输入准备路线与成本矩阵；处理 provided、haversine 和经许可的导航路线。
metadata:
  short-description: 准备路线与成本矩阵
---

# 路线与成本矩阵

只消费 `ready` `normalized_network_input.v1` 的精确 ResourceRef。当前仓网使用 `existing_only`；明确包含候选仓的模拟或规划使用 `all_warehouses`。

优先使用输入中已有的 `provided` 路线事实。只有在用户确认绕路系数和平均速度后才使用 `haversine`。用户选择导航时，先调用 `prepare_route_matrix(route_method="navigation")` 获得路线数量与计费风险；取得用户许可后，仅通过受授权的导航 Tool 和 `register_navigation_route_matrix` 登记完整、验证通过的导航矩阵。不得把地图 GeoJSON、模型文本或估算值伪装为导航矩阵。

成本优先使用用户路线报价；没有完整报价和用户确认的计算规则时，明确标记成本不可用。返回 Tool 产生的精确矩阵 ResourceRef，不从 Workspace 或 Resource 列表重建数据。
