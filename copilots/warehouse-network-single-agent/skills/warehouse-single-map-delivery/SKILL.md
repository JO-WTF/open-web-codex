---
name: warehouse-single-map-delivery
description: 单 Agent 把已完成的仓网结果制作成地图或报告。
metadata:
  short-description: 单 Agent 地图与交付
---

# 单 Agent 地图与交付

地图只展示规划工具已经确认的结果，不重新计算路线、成本、分配或达标率。

- 根据用户目标选择仓网分布图、时效覆盖图或方案对比图。
- 时效覆盖图先用 `prepare_network_coverage_map` 按用户要求的服务时限准备，再用 `create_network_map_card` 交付；按规划结果中的“达标、未达标、未分配”分别展示需求城市和覆盖线路。
- 用户没有指定样式时，清楚区分中心仓、越库仓（XD）、候选仓、启用/关闭状态和时效结果。
- 使用规划工具交付的完整结果调用对应的仓网地图工具，不调用通用地图构建器，也不自行拼接 GeoJSON 或图层。
- `create_network_map_card` 成功后立即返回业务摘要和完整 embed 段落，不再调用 `publish_workspace_geojson`、另一张地图、报告或其他工具。只有用户明确要求报告时才生成报告。
- 修改已有地图只使用用户明确选中的 `map_spec_ref`；缺少或失效时请用户重新选择，不猜测旧地图。
