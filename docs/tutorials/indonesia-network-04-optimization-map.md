# 印尼仓网 4：候选仓、p-median 和方案地图

本篇完成规划闭环：在已有仓保持固定的前提下，从候选仓中选择要新增的位置；如果用户明确指定已有仓可关闭，再把它们列入 optional existing；最后比较两个方案并生成地图 Artifact。

## 新增概念

| 对象 | 意义 |
| --- | --- |
| candidate warehouse | 允许算法选择的位置，不代表一定开仓 |
| fixed existing | 默认保持开启的已有仓 |
| optional existing | 用户明确允许关闭的已有仓 |
| p-median | 在有限候选集合中最小化需求加权成本 |
| service-constrained | 在满足时效覆盖率约束后最小化成本 |
| map Artifact | 由 Network Agent 从同一个 Case 生成的确定性 GeoJSON，不是 Visualization Agent 二次计算 |

本 fixture 的 `candidate-warehouses.csv` 使用行政区城市位置作为候选。它不是“任意经纬度连续空间”的最优点；p-median 的结论只对上传的候选集合负责。

## Web 操作

1. 上传 `candidate-warehouses.csv`。
2. 发送：

   ```text
   在现有印尼仓网基础上，从候选仓中选择新增仓。默认固定全部已有仓，只打开候选仓。
   目标是 12 小时需求覆盖率至少 85%，在满足目标的方案中总成本最小。
   请输出选中的仓、覆盖关系、总成本、分仓成本、时效覆盖率和与当前方案的变化，并创建地图卡片比较两个方案。
   ```

3. 如果用户希望已有仓可以关闭，必须明确给出 ID，例如：

   ```text
   允许关闭 existing-warehouse-07 和 existing-warehouse-08，其他已有仓固定。
   ```

   不要因为仓库名称中出现“前置仓”就自动决定它可以关闭。

## Agent 与 Tool 责任

- Data Agent：映射候选仓，补充坐标，调用边界校验，并更新 Case 的标准化输入。
- Network Agent：调用 `plan_route_matrix`、`plan_cost_matrix`，检查矩阵完整性。
- Network Agent：调用 `solve_p_median` 或 `solve_service_constrained_location`。求解使用 OR-Tools、固定随机种子、单线程和时间上限。
- Network Agent：调用 `prepare_network_comparison_map` 准备基线与选址结果的对比 GeoJSON，再由地图卡片工具展示；用户明确要求时才调用 `publish_network_planning_report`。
- Supervisor：只汇总方案和假设，不读取候选文件，也不替 Network Agent 选仓。

求解器返回 `timeout` 时，结果只能叫“当前可行解”，不能叫“最优解”；返回 `unavailable` 时不得切换到模型手算或旧算法。

## 地图卡片应显示什么

地图 Artifact 至少包含：

- 基线和候选方案标签；
- 需求城市点、已有仓、候选仓和选中仓；
- 当前方案与候选方案的服务连线；
- 方案标签、覆盖率变化和成本变化；
- 数据来源和估算方式。

地图只展示确定性结果，不调用导航、不重新计算成本、不接受浏览器传来的文件路径。浏览器只拿到有限 DTO 和已授权 Artifact，不拿原始表格。

## 真实验收

1. Case 的 `facility_location` facet 中，existing fixed 仓全部启用。
2. 每个需求城市恰好有一个分配仓；不可达城市必须显式列为未分配。
3. 服务约束的覆盖率不低于用户输入的目标，除非状态是 `infeasible` 或 `timeout`。
4. `network_comparison_map.v1` Artifact 可被浏览器读取，地图卡片能显示两方案差异。
5. `network_planning_report.v1` 的内容 hash 稳定，报告不含原始客户表和思维链。
6. 刷新后所有 Agent execution、输入请求、报告和地图仍然存在。

## 接入真实业务

把 fixture 替换成用户自己的需求城市、已有仓、候选仓、current coverage 和报价文件即可。国家不写死为印尼；国家由用户请求确定，行政区工具按国家加载。若真实客户量很大，优先上传聚合到城市的需求，不要为每个客户调用导航。只有在路线规模可接受、接口有缓存、费用已获许可时，才使用 Maps MCP 的批量 `distance_matrix`。
