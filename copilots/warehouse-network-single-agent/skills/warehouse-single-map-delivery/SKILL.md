---
name: warehouse-single-map-delivery
description: 将精确仓网结果转换为对话内地图、HTML 可视化或明确请求的最终报告；不重新计算业务结果。
metadata:
  short-description: 生成仓网地图与报告
---

# 仓网地图与交付

只从精确分析、比较或分配结果生成 distribution、coverage 或 comparison GeoJSON。将 Planner 返回的完整 `data_ref`（包括 profile）原样交给 `map_utils`；图层、颜色、图例和 hover 只使用 profile 已声明的字段和类型。地图 Tool 返回的 embed 指令必须原样作为独立段落返回。

仓库点必须以 `kind="warehouse"`、`warehouse_type`、`is_existing` 三个正交字段建立图层，不能只用一个 `kind="warehouse"` 图层：现有中心仓筛选 `is_existing=true AND warehouse_type=center`，现有 XD 筛选 `is_existing=true AND warehouse_type=cross_docking`；候选中心仓/XD 分别筛选 `is_existing=false` 与各自仓型；`opened_candidate=true` 和 `closed_existing=true` 再以独立、高优先级状态层覆盖。只有 profile 中存在这些字段且候选 feature 已实际发布时才创建对应候选层和图例；字段或 feature 缺失时如实省略，不猜测。

修订已有地图时，只调用 `revise_map_card`，且只接受当前 Turn 由 Platform 注入的精确 `map_spec_ref`：它必须是 `server="map_utils"`、`resource_schema="map_card_spec.v1"`、`uri` 以 `maps-data://map-card-spec/` 开头的 ResourceRef。artifact ID、GeoJSON ref、地图标题、模型文本或“上一张地图”不是 spec；缺少精确选择时返回 `needs_context`，请用户在卡片上选择「基于此图修改」。`map_card_spec_ref_invalid` 或 `map_card_spec_unavailable` 是当前请求的 typed 终态，不构造变体、不重试。

只有用户明确要求最终报告时调用一次 `publish_network_planning_report`；只有明确要求导出时才调用地图文件渲染。中间 Resource、导航请求和地图数据不是 Artifact。不要为地图或报告重新求解路线、成本或方案。
