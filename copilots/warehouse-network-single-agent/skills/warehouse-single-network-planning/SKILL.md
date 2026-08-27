---
name: warehouse-single-network-planning
description: 单 Agent 完成仓网基线、单仓变化或多仓选址分析。
metadata:
  short-description: 单 Agent 仓网规划
---

# 单 Agent 仓网规划

只使用已经准备好的数据和规划工具返回的结果，不读取原始文件，不根据预览或模型判断重算业务数据。

## 业务判断

- 先确认用户要做基线、单仓变化，还是达到服务目标的多仓选址。
- 用户要求真实现状时需要当前分配关系；没有该数据，只能评估现有仓范围内的优化分配。
- 现有仓默认保留。只有用户明确允许时才关闭现有仓。
- 用户已经明确要真实导航时，直接按下面的“真实导航”路径执行；不得先用 `route_method="provided"` 或 Haversine 替代，也不得再次询问已经回答的路线方法。
- 只有用户尚未回答路线方法时，才先用 `route_method="provided"` 调用 `prepare_route_matrix` 核验已有路线。不得自行填写绕路系数、平均速度或导航费用来绕过数据缺口。
- 工具返回路线补齐选项且该选择仍未由当前用户意图回答时，用输入卡片等待用户选择。只有用户选择“估算路线”后，才按该选项给出的参数使用 Haversine 估算；选择上传路线时按对应流程继续。
- 得到完整路线结果后，再调用 `evaluate_network_baseline` 计算基线；缺少必要路线时不得把结果解释为“没有变化”。
- 单仓变化使用 `assess_facility_change`；多仓选址使用 `solve_p_median`，不要自行拆成多套重复计算。
- 用户只给服务目标、未指定仓数时，使用 `minimum_feasible` 求解一次“达到目标所需的最少新增仓”；用户明确给出仓数时按该仓数求解。不要循环试仓数。
- 报价均值由规划工具基于全部已准备数据计算，不能根据预览样本推导。用户明确要求脚本核算时，保存可复用的 `.py` 脚本，由它读取同一份已准备数据并按 `warehouse_quote_mean_calculation.v1` 写出 `.json` 证据，再把结果路径交给 `plan_cost_matrix` 复核。

## 真实导航

- 使用本次 Data 交接或规划 Tool 已确认的稳定仓库 ID 创建精确 `warehouse_scope={"kind":"selected_warehouses","warehouse_ids":[...]}`；不得从名称格式猜 ID，只分析这些 ID，绝不为了补齐 cross-dock 上游而自动扩大范围。若 Tool 返回 `warehouse_upstream_center_missing`，如实停止并报告该结果。
- 用该 exact scope 调用 `create_navigation_matrix_request`。只有 `execute_navigation_matrix` 在当前请求中确实未加载时，才正常调用 `tool_search`；不要因为新回合、恢复或历史结果而重复搜索。
- 随后调用 `execute_navigation_matrix`，保留 Runtime 原生 `prompt` 审批，不直接暴露、静默批准或模拟该 Tool。审批拒绝、取消、超时或外部失败都是终态。
- 成功后立即用返回的完整 Workspace 相对 JSON 路径调用 `import_navigation_matrix`，并保留 exact scope、输入身份和导入后的结果引用；不要复制整张矩阵到消息中。

## 完成或停止

- 只有当前用户意图尚未回答的真实业务歧义才返回 `needs_input`；此时用一次输入卡片原样展示每个问题的 `id`、标题、问题和选项，然后停止。不得把已经选择真实导航的路线方法再次变成输入卡片，也不得重调同一工具、改写问题、继续求解或画图。
- 成功时保留本次结论、关键数字、已确认事实、必要假设和现有结果无法确认的内容。
- 事实与推断分开。不要把地图归属或汇总成本当成某个城市选择某仓的直接原因。
- 仓型、启停状态等分类小计或分类标签，只有本次 Data 交接或规划工具明确返回时才能报告；只有总数或 ID 时，只报告总数或 ID，不根据名称、地图资料或原始数据补出分类。
- 在当前候选和假设下无法达到目标，是有效业务结果；权限、身份、取消、超时或能力不可用则是执行失败，应如实停止。
- 需要地图或报告时交给地图交付 Skill，不重复计算或拼接数据。
