# 印尼仓网 3：当前覆盖、基线和仓网模拟

本篇把“用户现在实际怎么送”加入分析。它重点不是再增加一个算法，而是避免把两种不同的问题混为一谈：实际当前覆盖关系，和在相同已有仓范围内重新分配后的优化基线。

## 新增数据

从 [current coverage extension](../../tools/supply-chain-network-planner/examples/indonesia-network/current-coverage-extension/current-coverage.csv) 上传 `current-coverage.csv`。它包含：

```text
demand_city_id,serving_warehouse_id,upstream_center_id
```

基础 fixture 故意不包含这份文件，所以前两篇只能报告 `optimized_existing_footprint`。有了这份文件后，才可以报告 `actual_current`。

## Web 操作

1. 在当前 Workspace 上传 `current-coverage.csv`。
2. 发送：

   ```text
   请先检查是否存在当前覆盖关系。计算实际当前方案的 6、12、18 小时需求覆盖率和全网成本，按省份和仓库展示。
   然后分别模拟：删除一个 cross-docking 仓、增加一个候选仓、以及把一个仓从城市 A 搬到城市 B。
   模拟默认按成本最优分配；如果要用时效最优，请先询问我。
   ```

3. 当用户明确要求“当前”但没有上传 `current-coverage.csv` 时，正确行为是先显示业务提示：

   > 没有发现当前覆盖方案。下面将计算现有仓范围内的时效最优或成本最优基线，不能把它称为实际当前方案。

   这不是静默兜底，而是不同标签的明确分析结果。

## 三种概念

| 标签 | 含义 | 能回答什么 |
| --- | --- | --- |
| `actual_current` | 按用户提供的覆盖关系原样评估，不重新分配 | 现在实际的时效和成本 |
| `optimized_existing_footprint` | 只在已有仓范围内按目标重新分配 | 如果不增删仓，仅优化分配能达到什么水平 |
| `scenario` | 用户明确指定增删或搬迁后的活动仓集合 | 这个具体方案的成本、时效和变化 |

每个需求城市只能分配给一个启用仓。cross-docking 的干线和末端仍分别计算；搬迁等价于关闭旧位置并启用新位置，不能只改显示名称。

## 预期 Agent 过程

1. Data Agent 将覆盖关系标准化为 `CurrentAssignment`，校验城市和仓库 ID 是否存在。
2. Network Agent 调用 `evaluate_network_baseline`。
3. 有覆盖文件时，工具返回实际覆盖；没有时，工具只返回已有仓优化基线并附带提示。
4. 用户指定方案后，调用 `evaluate_facility_scenario`，默认 objective 是 `min_cost`，但必须在结果中写明这个选择。
5. 用户指定的服务目标由场景工具一并计算；不应从模型文字中猜出目标小时数。
6. 需要可视化时，`render_network_comparison_map` 从同一个 Case 读取基线和场景，不把覆盖明细复制进上下文。

## 真实检查

- 当前 coverage 文件存在时，报告标签是 `actual_current`，且 assignment 行与文件一致。
- 当前 coverage 文件不存在时，报告标签是 `optimized_existing_footprint`，并显示缺口提示。
- 删除一个仓后，结果中不能继续把该仓作为 active warehouse。
- 增加一个候选仓时，只有显式加入的候选才能参与分配。
- 搬迁方案同时记录 removed 和 added 位置。
- 结果终态和 Agent execution 卡片可在刷新后恢复；连续 wait 不会生成几十条消息。

## 当前方案数据的注意点

当前覆盖关系不是仓库列表的替代品。它只回答“哪个需求城市当前由哪个仓服务”；需求城市、已有仓和坐标仍必须来自标准化输入。若一个覆盖行引用不存在的仓或城市，Data Agent 应返回 `needs_input`，不能删除该行或自动找相似名称。

下一篇将加入候选仓、p-median、时效约束选址和地图比较。
