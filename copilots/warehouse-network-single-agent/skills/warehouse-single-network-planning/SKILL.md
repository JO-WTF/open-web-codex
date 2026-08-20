---
name: warehouse-single-network-planning
description: 为单 Agent 准备路线与成本，并完成仓网基线、仓库变动和选址规划。
metadata:
  short-description: 单 Agent 路线成本与规划
---

# 单 Agent 仓网规划

按用户目标选择基线、单仓变动或多仓选址；复用同一份 Tool 结果，不重复计算同一矩阵。Data preparation Skill 交接的完整 prepared input 是本 Skill 的唯一业务输入。

路线与成本：

- 优先使用 `provided` 路线事实。只有用户确认绕路系数和平均速度后才用 haversine。
- 导航先生成 request，展示调用量和费用风险，取得原生许可后执行并导入。
- 同一任务既要基线又要候选仓方案时，第一次按 `all_warehouses` 准备路线，后续复用同一引用。
- 成本优先使用报价。按完整报价均值外推时调用 `plan_cost_matrix` 的 `observed_quote_mean`；Planner 负责完整数据聚合和证据校验，不能从 preview 推算，也不能把均值改写成另一份 explicit 事实。
- 只有用户明确要求脚本时，才可对 exact prepared input 执行有界 Python/shell 统计；脚本输出必须是 `warehouse_quote_mean_calculation.v1`，再把其 evidence path 交给 Planner 校验。脚本不是标准化数据 owner，也不能替代路线或求解 Tool。
- 报价均值外推是敏感性方案；缺少仓租、建设、容量或吞吐成本时，不称为经济意义上的全局最优。

分析与规划：

- 当前网络用 `evaluate_network_baseline` 的 `auto` 模式；没有 current assignments 时只能称为 `optimized_existing_footprint`。
- 新增、关闭或搬迁一个仓库用 `assess_facility_change`；关闭已有仓必须先得到用户许可。
- 给出新增仓数时用 `opening_policy.kind=exact`；只要求达到时效/需求加权覆盖率时，用一次 `opening_policy.kind=minimum_feasible` 和 `service_constraints`，让 Planner 在总时间预算内从 0 开始有界搜索，不循环调用多个 p-median。
- 默认保留所有已有仓；只有用户明确许可并给出可关闭的已有仓 ID 时才允许 closure policy。

Tool 返回的 structured result 和精确 ResourceRef 是后续工作的唯一输入。`infeasible` 只表示搜索中的业务结果；权限、身份不一致、取消、超时、外部失败、能力不可用或输入无效是 typed 终态，应停止并如实报告。
