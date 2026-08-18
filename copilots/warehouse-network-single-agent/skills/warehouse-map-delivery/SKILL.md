---
name: warehouse-map-delivery
description: 将精确仓网结果转换为对话内地图、HTML 可视化或明确请求的最终报告；不重新计算业务结果。
metadata:
  short-description: 生成仓网地图与报告
---

# 仓网地图与交付

只从精确分析、比较或分配结果生成 distribution、coverage 或 comparison GeoJSON。将 Planner 返回的完整 `data_ref`（包括 profile）原样交给 `map_utils`；图层、颜色、图例和 hover 只使用 profile 已声明的字段和类型。地图 Tool 返回的 embed 指令必须原样作为独立段落返回。

只有用户明确要求最终报告时调用一次 `publish_network_planning_report`；只有明确要求导出时才调用地图文件渲染。中间 Resource、导航请求和地图数据不是 Artifact。不要为地图或报告重新求解路线、成本或方案。
