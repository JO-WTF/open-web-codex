---
name: build-network-matrices
description: 使用 Network Case 中已确认的数据，构建并校验距离、时长和运输成本矩阵。
---

# 构建仓网矩阵

本 Skill 由 Network Agent 使用。只接收 `case_id` 和业务参数，不读取 Workspace 文件，也不要求 Data Agent 复制数据。

## 距离与时长

1. 调用 `plan_route_matrix`，先得到起点数、终点数、路线总数和导航预计计费次数。
2. 球面距离方案必须由用户或已确认上下文提供正数的绕路系数与平均速度。调用 `build_haversine_route_matrix` 后，距离为球面距离乘绕路系数，时长为调整后距离除以平均速度。
3. 导航方案必须先把路线数量和费用风险告诉用户，并通过官方 `request_user_input` 获得许可。必须使用批量矩阵能力，禁止逐条调用。
4. 调用 `validate_route_matrix`。缺失、不可达和错误路线必须显式保留，不能自动替换为球面距离。

## 成本

1. 调用 `plan_cost_matrix`。精确路线报价优先，报价单位和车辆容量使用 Case 中已确认的数据。
2. 缺少报价时，向用户询问币种、固定起步价、每公里费用及必要的区域附加规则。不得把缺失成本填成零。
3. `linehaul` 与 `last_mile` 分开计算，禁止重复计费。

## 交付

矩阵明细只保存在 Network Case。向 Supervisor 返回 `case_id`、路线数、计算方法、参数、完整性、币种和下一步，不返回矩阵行或内部组件 ID。
