---
name: warehouse-network-planning
description: 仅供 network_agent 使用；根据用户目标完成仓网基线、设施变化或选址分析。
metadata:
  short-description: 仓网规划
---

# 仓网规划

只处理 Data 交接的 ready prepared input 和 Planner 返回的精确结果，不读取 raw、preview 或自行重算业务数据。

## 业务判断

- 先判断目标：基线、单仓变化，还是达到服务目标的多仓选址。
- 用户要求真实现状时需要 current assignment；没有它只能说明是优化现有仓足迹。
- 已有仓默认保留。关闭已有仓必须得到用户明确许可；新增仓、候选仓和服务目标必须来自用户目标或 Tool 结果。
- 优先使用已确认的路线事实。缺路线事实时，只有用户确认绕路系数、平均速度或导航费用后才继续。
- 12h baseline 或地图若没有 exact `route_matrix` ref，先用 prepared 中的 provided facts 调用 `prepare_route_matrix`，拿到路线 ref 后再做 `evaluate_network_baseline`；地图交给 Map Delivery Skill。
- 用户只给服务目标而未给仓数时，调用一次 `minimum_feasible`；用户明确给仓数时使用 `exact`。不要由 Agent 循环尝试不同仓数。
- 报价均值只能由 Planner 对完整 prepared input 计算。只有用户明确要求脚本证据时，才按单 Agent 任务 Skill 的授权生成 evidence；模型不得从 preview 推导均值。
- 下一 Tool 的 deferred schema 若未出现在当前 request，先重新 `tool_search`，再调用该 Tool。

## 交接

规划 Tool 成功后的 structured result 和精确 ResourceRef 是后续唯一输入；同一目标不重复计算。地图或报告交付交给对应 Map Delivery Skill，不在本 Skill 重算或拼接数据。`infeasible` 是有界业务结果；权限、身份、取消、超时、能力不可用和输入错误是 typed 终态，应停止并如实报告。
