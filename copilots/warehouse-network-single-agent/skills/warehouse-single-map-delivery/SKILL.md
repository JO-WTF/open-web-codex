---
name: warehouse-single-map-delivery
description: 将精确仓网结果转换为对话内地图、HTML 可视化或明确请求的最终报告；不重新计算业务结果。
metadata:
  short-description: 生成仓网地图与报告
---

# 仓网地图与交付

只从精确分析、比较或分配结果生成 distribution、coverage 或 comparison GeoJSON。将 Planner 返回的完整 `data_ref`（包括 profile）原样交给 `map_utils`；图层、颜色、图例和 hover 只使用 profile 已声明的字段和类型。地图 Tool 返回的 embed 指令必须原样作为独立段落返回。

仓网点、线、面地图优先调用 `create_network_map_card`，传入 Network Tool 返回的精确 GeoJSON `data_ref`。用户要求行政区边界时，先将 Data 已确认的 Workspace GeoJSON 通过 `publish_workspace_geojson(require_polygon=true)` 发布成精确 `boundary_data_ref`，再传入地图 Tool。该 Tool 使用 `kind="warehouse"`、`warehouse_type`、`is_existing` 三个正交字段生成现有中心仓、现有 XD、候选中心仓/XD、启用候选仓和关闭现有仓图层；不要自行拼 `sources`、`layers` 或猜字段。只有明确需要超出固定领域表达的样式时，才按 profile 直接调用低层 `create_map_card`。

一次地图交付中，`create_network_map_card` 成功后必须立即引用该次返回的 `structuredContent.embed.code`，并停止地图构建；不得在同一交付中再调用 `create_map_card` 或用另一张卡片替换其 Artifact 引用。超出领域表达的样式要求应作为新的、明确的地图样式请求处理；只有在该请求开始时才调用一次低层 `create_map_card`，并且只引用该调用成功返回的 embed 指令。

修订已有地图时，只调用 `revise_map_card`，且只接受当前 Turn 由 Platform 注入的精确 `map_spec_ref`：它必须是 `server="map_utils"`、`resource_schema="map_card_spec.v1"`、`uri` 以 `maps-data://map-card-spec/` 开头的 ResourceRef。artifact ID、GeoJSON ref、地图标题、模型文本或“上一张地图”不是 spec；缺少精确选择时返回 `needs_context`，请用户在卡片上选择「基于此图修改」。`map_card_spec_ref_invalid` 或 `map_card_spec_unavailable` 是当前请求的 typed 终态，不构造变体、不重试。

只有用户明确要求最终报告时调用一次 `publish_network_planning_report`；只有明确要求导出时才调用地图文件渲染。中间 Resource、导航请求和地图数据不是 Artifact。不要为地图或报告重新求解路线、成本或方案。
