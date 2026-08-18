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

`data_ref.profile` 是筛选、表达式、标签和 tooltip 的唯一模型可见真相。按 `discriminator_property` 与 `feature_types` 建立图层，只使用相应 feature type 中具有所需非空类型的字段。为仓库、需求城市和干线配置 hover，选择可读标题及业务标识、仓型、需求量、服务关系、运输时长、距离、成本和方案状态；缺失字段直接省略。

按 `profile.discriminator_property`、`feature_types` 和实际非空字段构造筛选、标签与 hover；不要从业务名称或历史地图猜字段。单一结果可使用 `assigned_warehouse_id`、`distance_km`、`duration_hours`、`unit_cost`；comparison 才可使用 `baseline_*` 与 `facility_*`。缺失字段直接省略，不制作通用替代字段。用户要求修订已有地图时，纯标题、图层、颜色、图例、hover 或视角只调用 `revise_map_card`；需要新覆盖线时先从精确分配结果生成新 GeoJSON。

只在用户明确请求最终报告时调用一次 `publish_network_planning_report`，明确请求导出时才调用地图文件渲染。中间 Resource、导航请求和地图数据不是 Artifact。不要为地图或报告重新求解路线、成本或方案。
