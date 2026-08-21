---
name: warehouse-single-map-delivery
description: 单 Agent 将精确仓网结果转换为地图或用户明确要求的最终报告。
metadata:
  short-description: 单 Agent 地图与交付
---

# 单 Agent 地图与交付

根据用户目标选择 distribution、coverage 或 comparison 展示。只使用 Planner 已返回的精确 result/ref 和 Map Tool 声明的字段，不重算路线、成本、分配或覆盖。

- comparison 使用同一 prepared identity 的合法 before/after comparison；单结果覆盖图使用对应 assignment result。
- 将 Planner 返回的完整 `data_ref` 原样交给 `map_utils`；样式只引用 profile 声明的字段和类型。用户未指定样式时，优先区分中心仓、XD、候选仓、启用/关闭状态和时效结果。
- coverage 先用同一精确 result/ref 调用 `prepare_network_coverage_map`，再把返回的完整 `data_ref` 交给高层 `create_network_map_card`；不调用低层 generic builder，不自行拼 sources/layers/GeoJSON。
- 地图 Tool 成功返回 embed 后立即交付并停止；不要继续调用其他地图、Resource 或报告 Tool。只有用户明确要求报告时才调用报告 Tool。
- 修改已有地图只接受当前 Turn 注入的精确 `map_spec_ref`；缺少或失效时返回 typed `needs_context`，不猜旧卡片。
- 最终文件交付由 Tool 负责；不自行拼 GeoJSON、HTML 或 Artifact。
