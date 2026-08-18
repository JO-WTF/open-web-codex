---
name: warehouse-route-planning
description: 仅供 network_agent 使用。为精确仓网输入准备 provided、haversine 或经许可的导航路线与成本矩阵。
metadata:
  short-description: 准备路线与成本矩阵
---

# 路线与成本矩阵

只消费 Supervisor 传入的 `ready` `normalized_network_input.v1` 精确 ResourceRef；不得发现、读取或重建它。当前仓网使用 `existing_only`，明确包含候选仓的模拟或规划使用 `all_warehouses`。

优先使用 `provided` 路线事实。只有用户确认绕路系数和平均速度后才使用 `haversine`。导航先通过 `prepare_route_matrix(route_method="navigation")` 获得路线数量和计费风险，获得许可后只用受授权的导航 Tool 及 `register_navigation_route_matrix` 登记完整验证通过的导航矩阵；不得将 GeoJSON、模型文本或估算值冒充导航矩阵。

成本优先使用用户报价。没有完整报价和用户确认的计算规则时明确成本不可用，不虚构币种或费率。仅返回 Tool 产生的精确 ResourceRef。
