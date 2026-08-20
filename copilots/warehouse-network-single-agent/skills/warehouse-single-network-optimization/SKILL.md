---
name: warehouse-single-network-optimization
description: 在用户明确要求新增仓或 p-median 规划时，为精确路线和成本矩阵求解并比较仓网方案。
metadata:
  short-description: 优化仓网选址方案
---

# 仓网选址优化

只在用户明确要求规划或选址时调用 p-median。默认保留全部已有仓；只有用户明确允许时才将指定已有仓列为可关闭。用户给出新增仓数时传 `opening_policy={kind:"exact",number_to_open:n}`；用户只要求“达到指定需求加权时效达标率的最优仓库分布”而未给新增仓数时，传一次 `opening_policy={kind:"minimum_feasible",maximum_number_to_open:候选仓数}`，并以用户时效和达标率传 `service_constraints`。Planner 在一个总 `time_limit_seconds` 内从 0 开始有界搜索，返回每个仓数的可行性、`first_feasible_number_to_open`、最终方案和 coverage；不要让模型循环调用多个 p-median。`infeasible` 是某个仓数的业务结果，Planner 会继续搜索；timeout、unavailable、权限/身份错误、取消、外部失败或候选仓耗尽则按原始 typed 终态停止。只有“最优”的业务目标确实无法从用户表述判断时才询问，不要求用户提供本应由完整报价派生的成本均值。

成本结果必须标明这是“现有报价均值外推”的敏感性方案；缺少仓租、建设、容量或吞吐成本时，不得称为经济意义上的全局最优。

求解与比较必须使用 Tool 返回的精确 ResourceRef；不要从旧报告、地图或模型文本拼装方案。返回结果应区分基线、候选方案与真实现状，并保留后续分析或交付所需的下游引用。
