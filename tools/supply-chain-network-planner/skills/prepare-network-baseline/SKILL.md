---
name: prepare-network-baseline
description: 为当前问题准备最小的数据、矩阵和业务参数，并形成可审查的仓网基线。
---

# 准备仓网基线

本 Skill 由 Network Agent 使用，目标是只准备当前问题真正依赖的内容。

1. 调用 `define_network_requirements`，明确国家、分析类型、目标和必要参数。
2. 调用 `get_network_case_status`。数据未就绪时，让 Supervisor 把同一个 `case_id` 交给 Data Agent，不能要求 Data Agent 另建数据包。
3. 使用 `build-network-matrices` 准备路线矩阵。只有成本问题才准备成本矩阵。
4. 用户询问当前时效或成本时，优先使用当前覆盖；如果 Case 中没有当前覆盖，必须说明“未发现当前覆盖方案”，并计算 `optimized_existing_footprint`。
5. 调用 `evaluate_network_baseline`，明确 `min_time` 或 `min_cost`、时效目标和是否包含成本。

球面距离是规划估算，不是导航承诺。日级时效需要额外确认司机每日有效行驶小时数，教程建议值不能成为隐藏默认值。

交付时只返回 Case 的方案标签、需求量、未分配需求、时效覆盖率、成本摘要、假设和缺口，不返回覆盖明细行。
