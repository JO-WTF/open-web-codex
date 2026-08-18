---
name: warehouse-map-delivery
description: 仅供 network_agent 使用。将精确仓网结果转换为对话内地图或明确请求的最终报告；不重新计算业务结果。
metadata:
  short-description: 生成仓网地图与报告
---

# 仓网地图与交付

只从精确分析、比较或分配结果生成 distribution、coverage 或 comparison GeoJSON。展示已有基线时携带精确 `baseline_ref`；已有前后方案比较时只使用 `compare_network_scenarios` 或 `assess_facility_change` 返回的单一 `plan_comparison_ref`，不分别拼装多个引用，也不为地图重算业务结果。

将 Planner 返回的完整 `data_ref`（包括 profile）原样交给 `map_utils`；图层、颜色、图例和 hover 只使用 profile 已声明的字段和类型。地图 Tool 返回的 embed 指令必须原样作为独立段落返回。

用户未指定样式时，覆盖线位于点图层之下，点图层从下到上依次为需求城市、XD 前置仓、中心仓：中心仓深蓝 `#1D4ED8`、半径 `12`；XD 橙色 `#F97316`、半径 `9`；候选仓紫色 `#7C3AED`；新增启用仓洋红 `#C026D3`；时效达标城市绿色 `#16A34A`，未达标红色 `#DC2626`。有干线 feature 时，Last mile 与干线必须使用独立图层和图例。`warehouse_type` 存在时，中心仓与 XD 必须是独立图层；只有 Tool 明确声明图像资源才可使用 `icon-image`。

默认完整样式为：中心仓白色描边 `2.5`；XD 白色描边 `2`；候选仓按仓型用半径 `12` 或 `9`、白色描边 `2`、不透明度 `0.9`；新增启用仓白色描边 `2.5`；没有时效结果的需求城市蓝色 `#2563EB`、半径 `4`、白色描边 `1.25`；时效达标城市半径 `5`、白色描边 `1.5`；未达标城市半径 `6`、白色描边 `1.75`；Last mile 覆盖线蓝色 `#2563EB`、宽度 `1.5`、不透明度 `0.5`；干线覆盖线深蓝 `#1E3A8A`、宽度 `2.5`、不透明度 `0.65`。图例仅列出实际展示对象，并分别保留中心仓、XD、候选仓、新增启用仓、时效达标、时效未达标、Last mile 和干线名称。

仓库点必须以 `kind="warehouse"`、`warehouse_type`、`is_existing` 三个正交字段建立图层，不能只用一个 `kind="warehouse"` 图层：现有中心仓筛选 `is_existing=true AND warehouse_type=center`，现有 XD 筛选 `is_existing=true AND warehouse_type=cross_docking`；候选中心仓/XD 分别筛选 `is_existing=false` 与各自仓型；`opened_candidate=true` 和 `closed_existing=true` 再以独立、高优先级状态层覆盖。只有 profile 中存在这些字段且候选 feature 已实际发布时才创建对应候选层和图例；字段或 feature 缺失时如实省略，不猜测。

生成仓网点、线、面地图时优先调用 `create_network_map_card`，传入 Network Tool 返回的精确 GeoJSON `data_ref`。用户要求行政区边界时，先将 Data 已确认的 Workspace GeoJSON 通过 `publish_workspace_geojson(require_polygon=true)` 发布成精确 `boundary_data_ref`，再传入地图 Tool。该 Tool 按 profile 自动生成仓库、需求、末端覆盖、干线、候选与设施状态图层；不要自行拼 `sources`、`layers` 或猜字段。只有超出该固定领域表达的明确用户样式需求，才按 `data_ref.profile` 直接调用低层 `create_map_card`。

低层样式调整时才按 `profile.discriminator_property`、`feature_types` 和实际非空字段构造筛选、标签与 hover；不要从业务名称或历史地图猜字段。单一结果可使用 `assigned_warehouse_id`、`distance_km`、`duration_hours`、`unit_cost`；comparison 才可使用 `baseline_*` 与 `facility_*`。缺失字段直接省略，不制作通用替代字段。

用户要求修订已有地图时，纯标题、图层、颜色、图例、hover 或视角只调用 `revise_map_card`，且只接受当前 Turn 由 Platform 注入的精确 `map_spec_ref`：它必须是 `server="map_utils"`、`resource_schema="map_card_spec.v1"`、`uri` 以 `maps-data://map-card-spec/` 开头的 ResourceRef。artifact ID、GeoJSON ref、地图标题、模型文本或“上一张地图”不是 spec；缺少精确选择时返回 `needs_context`，请用户在卡片上选择「基于此图修改」。`map_card_spec_ref_invalid` 或 `map_card_spec_unavailable` 是当前请求的 typed 终态，不构造变体、不重试。需要新覆盖线时先从精确分配结果生成新 GeoJSON。

只在用户明确请求最终报告时调用一次 `publish_network_planning_report`，明确请求导出时才调用地图文件渲染。中间 Resource、导航请求和地图数据不是 Artifact。不要为地图或报告重新求解路线、成本或方案。
