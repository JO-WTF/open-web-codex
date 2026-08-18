---
name: warehouse-network-optimization
description: 仅供 network_agent 使用。用户明确要求新增仓或 p-median 规划时，为精确路线和成本矩阵求解并比较仓网方案。
metadata:
  short-description: 优化仓网选址方案
---

# 仓网选址优化

只在用户明确要求规划或选址时调用 p-median。默认保留全部已有仓；仅当用户明确允许时才将指定已有仓列为可关闭。开始前确认新增仓数、优化目标、时效目标或约束，以及已授权的路线和成本矩阵。

求解和比较必须使用 Tool 返回的精确 ResourceRef；不要从旧报告、地图或模型文本拼装方案。返回结果要区分基线、候选方案和真实现状，并保留后续分析或交付需要的下游引用。
