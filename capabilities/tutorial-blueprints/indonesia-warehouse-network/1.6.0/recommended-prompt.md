这是我主动安装的 Indonesia Network Planning Tutorial Blueprint。请只使用该 Blueprint 上传的合成教程数据，分析印尼现有仓网并给出建议；不要调用 Demo Tool，也不要把原始表格复制到消息中。请创建一个 Network Case（仓网案例），并让所有子 Agent 使用同一个 `case_id`。

先让 Network Agent 根据问题发布数据需求，让 Data Agent 检查、映射并标准化数据。使用球面距离乘绕路系数计算距离，使用调整后距离除以平均速度计算时效；缺少绕路系数、平均速度或时效目标时，通过 Web 输入卡片询问我。没有 current coverage 时，明确标记为现有仓范围内的优化基线，不要称为实际当前方案。

先完成基础时效覆盖分析，再按需计算两级网络成本、仓库增删或搬迁场景、p-median 和时效约束选址。中间数据只保存在 Network Case 中；子 Agent 之间只传 `case_id` 和有限状态。只有最终报告和地图发布为确定性 Artifact。
