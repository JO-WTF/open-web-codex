---
name: warehouse-network-analysis
description: 仅供 network_agent 使用。评估当前仓网或单一仓库变动的时效、覆盖、成本与分配结果；不执行选址或交付渲染。
metadata:
  short-description: 评估仓网覆盖与变动
---

# 仓网分析

使用同一份精确 `prepared_network_input.v1` Workspace 路径、route matrix 与可选 cost matrix 引用。Tool 必须拒绝输入身份不同的矩阵或方案。只问当前仓网时按 `existing_only` 和用户目标评估，同时返回城市等权与需求量加权覆盖。`current_assignments` 缺失时只能称为 `optimized_existing_footprint`，不能称真实现状。

增加、关闭或搬迁单个仓库时调用 `assess_facility_change`；关闭已有仓必须先获得用户许可。比较只使用同一 structured result 返回的成本、时效、活动仓、受影响城市和重新分配城市。缺少输入或 Tool 终态失败时返回 typed 结果并停止。
