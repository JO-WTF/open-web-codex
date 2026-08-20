---
name: warehouse-single-network-optimization
description: 在用户明确要求新增仓或 p-median 规划时，为精确路线和成本矩阵求解并比较仓网方案。
metadata:
  short-description: 优化仓网选址方案
---

# 仓网选址优化

只在用户明确要求规划或选址时调用 p-median。默认保留全部已有仓；只有用户明确允许时才将指定已有仓列为可关闭。用户给出新增仓数时按该数求解；用户只要求“达到指定需求加权时效达标率的最优仓库分布”而未给新增仓数时，默认目标为“最少启用候选仓，同一新增仓数下运输成本最低”：从 `number_to_open=0` 开始递增，以用户时效和达标率传 `service_constraints`，第一个成功满足约束的 proven solution 即停止。每个成功返回的 `infeasible` 只是该仓数不可行，可以继续下一个仓数；timeout、unavailable、Tool 失败或候选仓耗尽则原样停止。只有“最优”的业务目标确实无法从用户表述判断时才询问，不要求用户提供本应由完整报价派生的成本均值。

求解与比较必须使用 Tool 返回的精确 ResourceRef；不要从旧报告、地图或模型文本拼装方案。返回结果应区分基线、候选方案与真实现状，并保留后续分析或交付所需的下游引用。
