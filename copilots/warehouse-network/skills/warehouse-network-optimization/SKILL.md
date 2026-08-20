---
name: warehouse-network-optimization
description: 仅供 network_agent 使用。用户明确要求新增仓或 p-median 规划时，为精确路线和成本矩阵求解并比较仓网方案。
metadata:
  short-description: 优化仓网选址方案
---

# 仓网选址优化

只在用户明确要求规划或选址时调用 p-median。默认保留全部已有仓；仅当用户明确允许时才将指定已有仓列为可关闭。用户给出新增仓数时传 `opening_policy={kind:"exact",number_to_open:n}`；未给新增仓数但要求达到时效/覆盖率时传一次 `opening_policy={kind:"minimum_feasible",maximum_number_to_open:候选仓数}`，并把时效和覆盖率传入 `service_constraints`。Planner 在一个总时间预算内完成从 0 开始的有界搜索，返回每个仓数的可行性、首个可行仓数、最终方案和 coverage；不要循环多次调用 p-median。只把 `infeasible` 当作可继续搜索的业务结果；权限、身份不一致、取消、超时、外部失败、不可用或输入无效都是当前请求终态。开始前确认优化目标、时效目标或约束，以及已授权的路线和成本矩阵。

成本结果必须标明这是“现有报价均值外推”的敏感性方案；缺少仓租、建设、容量或吞吐成本时，不得称为经济意义上的全局最优。

求解和比较必须使用 Tool 返回的精确 ResourceRef；不要从旧报告、地图或模型文本拼装方案。返回结果要区分基线、候选方案和真实现状，并保留后续分析或交付需要的下游引用。
